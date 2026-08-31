use insta_dl_gui_lib::config::Config;
use insta_dl_gui_lib::hiker::HikerClient;
use insta_dl_gui_lib::network::{build_cdn_client, NetworkClients};
use insta_dl_gui_lib::proxy::apply_proxy;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, MutexGuard};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEST_TIMEOUT: Duration = Duration::from_secs(3);
const PROXY_ENV_VARS: [&str; 3] = ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"];

static PROXY_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct ProxyEnvironmentGuard {
    _lock: MutexGuard<'static, ()>,
    original: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl ProxyEnvironmentGuard {
    async fn with_dead_proxies() -> Self {
        let lock = lock_proxy_environment().await;
        let original = PROXY_ENV_VARS
            .into_iter()
            .map(|name| (name, std::env::var_os(name)))
            .collect();
        for name in PROXY_ENV_VARS {
            std::env::set_var(name, "http://127.0.0.1:9");
        }
        Self {
            _lock: lock,
            original,
        }
    }
}

async fn lock_proxy_environment() -> MutexGuard<'static, ()> {
    PROXY_ENV_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

impl Drop for ProxyEnvironmentGuard {
    fn drop(&mut self) {
        for (name, value) in &self.original {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn accept_with_timeout(listener: &TcpListener) -> std::io::Result<TcpStream> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + TEST_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                return Ok(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "proxy did not receive a connection",
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

fn read_headers(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    stream.set_read_timeout(Some(TEST_TIMEOUT))?;
    let mut headers = Vec::new();
    let mut chunk = [0; 256];
    while !headers.ends_with(b"\r\n\r\n") {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "proxy request ended before headers",
            ));
        }
        headers.extend_from_slice(&chunk[..read]);
    }
    Ok(headers)
}

fn start_socks5_proxy() -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("SOCKS listener");
    let uri = format!("socks5h://{}", listener.local_addr().unwrap());
    let (target_tx, target_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener).expect("SOCKS connection");
        stream
            .set_read_timeout(Some(TEST_TIMEOUT))
            .expect("read timeout");

        let mut greeting = [0; 2];
        stream.read_exact(&mut greeting).expect("SOCKS greeting");
        let mut methods = vec![0; greeting[1] as usize];
        stream.read_exact(&mut methods).expect("SOCKS methods");
        stream.write_all(&[5, 0]).expect("SOCKS no-auth reply");

        let mut request = [0; 4];
        stream
            .read_exact(&mut request)
            .expect("SOCKS connect request");
        assert_eq!(request, [5, 1, 0, 3], "SOCKS5 CONNECT request prefix");
        let mut length = [0; 1];
        stream.read_exact(&mut length).expect("domain length");
        let mut domain = vec![0; length[0] as usize];
        stream.read_exact(&mut domain).expect("domain name");
        let mut port = [0; 2];
        stream.read_exact(&mut port).expect("destination port");
        assert_eq!(u16::from_be_bytes(port), 80, "SOCKS destination port");
        target_tx
            .send(String::from_utf8(domain).expect("UTF-8 domain"))
            .expect("target receiver");

        stream
            .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
            .expect("SOCKS connect reply");

        read_headers(&mut stream).expect("proxied HTTP request");
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nsocks5h-ok",
            )
            .expect("proxied HTTP response");
    });
    (uri, target_rx, server)
}

fn start_connect_proxy() -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("CONNECT listener");
    let uri = format!("http://{}", listener.local_addr().unwrap());
    let (headers_tx, headers_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let mut stream = accept_with_timeout(&listener).expect("CONNECT connection");
            let headers = read_headers(&mut stream).expect("CONNECT headers");
            headers_tx
                .send(String::from_utf8(headers).expect("UTF-8 headers"))
                .expect("header receiver");
            stream
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .expect("CONNECT response");
        }
    });
    (uri, headers_rx, server)
}

#[tokio::test]
async fn hiker_client_routes_requests_through_an_http_proxy() {
    let _environment_lock = lock_proxy_environment().await;
    let proxy = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sys/balance"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "requests": 42 })),
        )
        .mount(&proxy)
        .await;

    let client = HikerClient::with_base_url_and_proxy(
        "token".to_owned(),
        "http://hiker.invalid".to_owned(),
        Some(&proxy.uri()),
    )
    .expect("proxy client");

    let balance = client.balance().await.expect("balance via proxy");

    assert_eq!(balance.requests, 42);
    assert_eq!(proxy.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn cdn_client_routes_requests_through_an_http_proxy() {
    let _environment_lock = lock_proxy_environment().await;
    let proxy = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/asset.jpg"))
        .respond_with(ResponseTemplate::new(200).set_body_string("proxy-cdn"))
        .mount(&proxy)
        .await;

    let client = build_cdn_client(Some(&proxy.uri())).expect("proxy client");
    let response = client
        .get("http://cdn.invalid/asset.jpg")
        .send()
        .await
        .expect("cdn response");

    assert_eq!(response.text().await.unwrap(), "proxy-cdn");
    assert_eq!(proxy.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn hiker_client_without_proxy_reaches_direct_origin() {
    let _environment_lock = lock_proxy_environment().await;
    let origin = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sys/balance"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "requests": 7 })),
        )
        .mount(&origin)
        .await;

    let balance = HikerClient::with_base_url("token".to_owned(), origin.uri())
        .balance()
        .await
        .expect("direct balance");

    assert_eq!(balance.requests, 7);
    assert_eq!(origin.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn cdn_client_without_proxy_reaches_direct_origin() {
    let _environment_lock = lock_proxy_environment().await;
    let origin = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/asset.jpg"))
        .respond_with(ResponseTemplate::new(200).set_body_string("direct-cdn"))
        .mount(&origin)
        .await;

    let client = build_cdn_client(None).expect("direct client");
    let response = client
        .get(format!("{}/asset.jpg", origin.uri()))
        .send()
        .await
        .expect("cdn response");

    assert_eq!(response.text().await.unwrap(), "direct-cdn");
    assert_eq!(origin.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn disabled_proxy_ignores_proxy_environment_for_direct_clients() {
    let _environment = ProxyEnvironmentGuard::with_dead_proxies().await;
    let origin = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/plain"))
        .respond_with(ResponseTemplate::new(200).set_body_string("direct"))
        .expect(1)
        .mount(&origin)
        .await;
    Mock::given(method("GET"))
        .and(path("/sys/balance"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "requests": 7 })),
        )
        .expect(1)
        .mount(&origin)
        .await;
    Mock::given(method("GET"))
        .and(path("/cdn"))
        .respond_with(ResponseTemplate::new(200).set_body_string("direct"))
        .expect(1)
        .mount(&origin)
        .await;

    let plain = apply_proxy(reqwest::Client::builder(), None)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        plain
            .get(format!("{}/plain", origin.uri()))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "direct"
    );

    let hiker = HikerClient::with_base_url("token".to_owned(), origin.uri());
    assert_eq!(hiker.balance().await.unwrap().requests, 7);

    let cdn = build_cdn_client(None).unwrap();
    assert_eq!(
        cdn.get(format!("{}/cdn", origin.uri()))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "direct"
    );
}

#[tokio::test]
async fn network_clients_route_hiker_and_cdn_https_requests_through_connect_proxy() {
    let _environment_lock = lock_proxy_environment().await;
    let (proxy_url, headers_rx, server) = start_connect_proxy();
    let clients = NetworkClients::from_config(&Config {
        token: Some("token".to_owned()),
        proxy_url: Some(proxy_url),
        ..Config::default()
    })
    .expect("configured clients");

    let hiker_result = tokio::time::timeout(TEST_TIMEOUT, clients.hiker.unwrap().balance()).await;
    let cdn_result = tokio::time::timeout(
        TEST_TIMEOUT,
        clients.cdn.get("https://cdn.invalid/asset.jpg").send(),
    )
    .await;

    let first = headers_rx.recv_timeout(TEST_TIMEOUT);
    let second = headers_rx.recv_timeout(TEST_TIMEOUT);
    let server_result = server.join();

    assert!(hiker_result.is_ok(), "Hiker request timeout");
    assert!(
        hiker_result.unwrap().is_err(),
        "Hiker request unexpectedly succeeded"
    );
    assert!(cdn_result.is_ok(), "CDN request timeout");
    assert!(
        cdn_result.unwrap().is_err(),
        "CDN request unexpectedly succeeded"
    );
    assert!(server_result.is_ok(), "CONNECT server failed");

    let requests = [
        first.expect("first CONNECT"),
        second.expect("second CONNECT"),
    ];
    assert!(requests
        .iter()
        .any(|headers| headers.starts_with("CONNECT api.hikerapi.com:443 HTTP/1.1\r\n")));
    assert!(requests
        .iter()
        .any(|headers| headers.starts_with("CONNECT cdn.invalid:443 HTTP/1.1\r\n")));
}

#[test]
fn network_clients_reject_invalid_proxy_configuration() {
    let result = NetworkClients::from_config(&Config {
        token: Some("token".to_owned()),
        proxy_url: Some("not a proxy URL".to_owned()),
        ..Config::default()
    });

    assert!(result.is_err());
}

#[tokio::test]
async fn cdn_client_negotiates_socks5h_with_remote_dns() {
    let _environment_lock = lock_proxy_environment().await;
    let (proxy_url, target_rx, server) = start_socks5_proxy();
    let client = build_cdn_client(Some(&proxy_url)).expect("SOCKS client");

    let response_result = tokio::time::timeout(
        TEST_TIMEOUT,
        client.get("http://cdn.invalid/asset.jpg").send(),
    )
    .await;
    let target = target_rx.recv_timeout(TEST_TIMEOUT);
    let server_result = server.join();

    assert!(response_result.is_ok(), "SOCKS request timeout");
    assert!(server_result.is_ok(), "SOCKS server failed");
    let response = response_result
        .expect("SOCKS request timeout")
        .expect("SOCKS response");

    assert_eq!(
        tokio::time::timeout(TEST_TIMEOUT, response.text())
            .await
            .expect("SOCKS body timeout")
            .unwrap(),
        "socks5h-ok"
    );
    assert_eq!(target.expect("SOCKS target"), "cdn.invalid");
}
