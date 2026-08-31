use insta_dl_gui_lib::hiker::HikerClient;
use insta_dl_gui_lib::network::build_cdn_client;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn start_socks5_proxy() -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("SOCKS listener");
    let uri = format!("socks5h://{}", listener.local_addr().unwrap());
    let (target_tx, target_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("SOCKS connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
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
        assert_eq!(request[3], 3, "socks5h should pass a domain to the proxy");
        let mut length = [0; 1];
        stream.read_exact(&mut length).expect("domain length");
        let mut domain = vec![0; length[0] as usize];
        stream.read_exact(&mut domain).expect("domain name");
        let mut port = [0; 2];
        stream.read_exact(&mut port).expect("destination port");
        target_tx
            .send(String::from_utf8(domain).expect("UTF-8 domain"))
            .expect("target receiver");

        stream
            .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
            .expect("SOCKS connect reply");

        let mut request_bytes = Vec::new();
        let mut chunk = [0; 256];
        while !request_bytes.ends_with(b"\r\n\r\n") {
            let read = stream.read(&mut chunk).expect("proxied HTTP request");
            assert_ne!(read, 0, "HTTP request ended before its headers");
            request_bytes.extend_from_slice(&chunk[..read]);
        }
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nsocks5h-ok",
            )
            .expect("proxied HTTP response");
    });
    (uri, target_rx, server)
}

#[tokio::test]
async fn hiker_client_routes_requests_through_an_http_proxy() {
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
async fn cdn_client_negotiates_socks5h_with_remote_dns() {
    let (proxy_url, target_rx, server) = start_socks5_proxy();
    let client = build_cdn_client(Some(&proxy_url)).expect("SOCKS client");

    let response = client
        .get("http://cdn.invalid/asset.jpg")
        .send()
        .await
        .expect("SOCKS response");

    assert_eq!(response.text().await.unwrap(), "socks5h-ok");
    assert_eq!(
        target_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        "cdn.invalid"
    );
    server.join().expect("SOCKS server");
}
