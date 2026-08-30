use insta_dl_gui_lib::config::Config;
use insta_dl_gui_lib::proxy::{apply_proxy, normalize_proxy_url};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const INVALID_PROXY: &str = "Enter a valid HTTP, HTTPS, SOCKS5, or SOCKS5H proxy URL";

#[test]
fn legacy_config_without_proxy_url_defaults_to_disabled_proxy() {
    let config: Config =
        serde_json::from_str(r#"{"token":"token","dest_dir":"/downloads","sidecar":false}"#)
            .unwrap();

    assert_eq!(config.proxy_url, None);
    assert_eq!(config.proxy_hint(), None);
}

#[test]
fn normalizes_supported_proxy_urls() {
    let cases = [
        (" http://proxy.example:8080 ", "http://proxy.example:8080/"),
        ("https://proxy.example:8443", "https://proxy.example:8443/"),
        (
            "socks5://proxy.example:1080",
            "socks5://proxy.example:1080/",
        ),
        (
            "socks5h://user:secret@proxy.example:1080",
            "socks5h://user:secret@proxy.example:1080/",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(
            normalize_proxy_url(Some(input)),
            Ok(Some(expected.to_owned()))
        );
    }
}

#[test]
fn none_or_whitespace_disables_proxy() {
    assert_eq!(normalize_proxy_url(None), Ok(None));
    assert_eq!(normalize_proxy_url(Some(" \t\n ")), Ok(None));
}

#[test]
fn http_and_https_accept_known_default_ports_but_socks_requires_an_explicit_port() {
    assert_eq!(
        normalize_proxy_url(Some("http://proxy.example")),
        Ok(Some("http://proxy.example/".to_owned()))
    );
    assert_eq!(
        normalize_proxy_url(Some("https://proxy.example")),
        Ok(Some("https://proxy.example/".to_owned()))
    );

    for input in ["socks5://proxy.example", "socks5h://proxy.example"] {
        assert_eq!(
            normalize_proxy_url(Some(input)),
            Err(INVALID_PROXY.to_owned())
        );
    }
}

#[test]
fn explicit_zero_port_is_rejected_for_every_supported_proxy_scheme() {
    for input in [
        "http://proxy.example:0",
        "https://proxy.example:0",
        "socks5://proxy.example:0",
        "socks5h://proxy.example:0",
    ] {
        assert_eq!(
            normalize_proxy_url(Some(input)),
            Err(INVALID_PROXY.to_owned())
        );
    }
}

#[test]
fn accepts_bracketed_ipv6_and_explicit_http_defaults_but_rejects_malformed_ipv6() {
    assert_eq!(
        normalize_proxy_url(Some("http://[::1]:8080")),
        Ok(Some("http://[::1]:8080/".to_owned()))
    );
    assert!(matches!(
        normalize_proxy_url(Some("http://proxy.example:80")),
        Ok(Some(_))
    ));
    assert!(matches!(
        normalize_proxy_url(Some("https://proxy.example:443")),
        Ok(Some(_))
    ));
    assert_eq!(
        normalize_proxy_url(Some("http://[::1:8080")),
        Err(INVALID_PROXY.to_owned())
    );
}

#[test]
fn rejects_unsafe_or_unsupported_proxy_urls_without_echoing_input() {
    let cases = [
        "ftp://secret@proxy.example:21",
        "proxy.example:8080",
        "http://:secret@",
        "http://proxy.example:not-a-port",
        "http://proxy.example:8080/path",
        "http://proxy.example:8080/?query=secret",
        "http://proxy.example:8080/#secret",
    ];

    for input in cases {
        let error = normalize_proxy_url(Some(input)).unwrap_err();
        assert_eq!(error, INVALID_PROXY);
        assert!(!error.contains(input));
    }
}

#[test]
fn proxy_hint_redacts_all_userinfo_without_losing_connection_context() {
    let config = Config {
        proxy_url: Some("socks5h://user:secret@proxy.example:1080".to_owned()),
        ..Config::default()
    };

    let hint = config.proxy_hint().unwrap();
    assert!(hint.starts_with("socks5h://***@"));
    assert!(hint.contains("proxy.example:1080"));
    assert!(!hint.contains("user"));
    assert!(!hint.contains("secret"));
}

#[test]
fn proxy_hint_redacts_username_only_and_percent_encoded_userinfo() {
    for (raw_proxy_url, secrets) in [
        ("socks5://alice@proxy.example:1080", &["alice"][..]),
        (
            "socks5h://alice:secret@proxy.example:1080",
            &["alice", "secret"][..],
        ),
        (
            "https://alice%40example:secret@proxy.example:8443",
            &["alice%40example", "alice@example", "secret"][..],
        ),
    ] {
        let config = Config {
            proxy_url: Some(raw_proxy_url.to_owned()),
            ..Config::default()
        };
        let hint = config.proxy_hint().unwrap();

        assert!(hint.contains("://***@"));
        assert!(hint.contains("proxy.example"));
        for secret in secrets {
            assert!(!hint.contains(secret), "hint leaked {secret}");
            assert!(
                !format!("{config:?}").contains(secret),
                "debug output leaked {secret}"
            );
        }
    }
}

#[test]
fn proxy_hint_keeps_unauthenticated_connection_context_and_omits_malformed_values() {
    let unauthenticated = Config {
        proxy_url: Some("https://proxy.example:8443".to_owned()),
        ..Config::default()
    };
    assert_eq!(
        unauthenticated.proxy_hint().as_deref(),
        Some("https://proxy.example:8443/")
    );

    let malformed = Config {
        proxy_url: Some("http://[::1:8080".to_owned()),
        ..Config::default()
    };
    assert_eq!(malformed.proxy_hint(), None);
}

#[test]
fn config_proxy_url_round_trips_raw_value_while_hint_is_redacted() {
    let raw_proxy_url = "socks5h://user:secret@proxy.example:1080";
    let config = Config {
        proxy_url: Some(raw_proxy_url.to_owned()),
        ..Config::default()
    };

    let serialized = serde_json::to_string(&config).unwrap();
    assert!(serialized.contains(raw_proxy_url));

    let round_tripped: Config = serde_json::from_str(&serialized).unwrap();
    assert_eq!(round_tripped.proxy_url.as_deref(), Some(raw_proxy_url));
    assert!(!round_tripped.proxy_hint().unwrap().contains("user"));
    assert!(!round_tripped.proxy_hint().unwrap().contains("secret"));
}

#[test]
fn config_debug_output_does_not_expose_hiker_token_or_proxy_userinfo() {
    let config = Config {
        token: Some("raw-hiker-token".to_owned()),
        proxy_url: Some("socks5h://user:secret@proxy.example:1080".to_owned()),
        ..Config::default()
    };

    let debug = format!("{config:?}");
    assert!(!debug.contains("raw-hiker-token"));
    assert!(!debug.contains("user"));
    assert!(!debug.contains("secret"));
}

#[tokio::test]
async fn disabled_proxy_returns_direct_clients_that_reach_wiremock() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/direct"))
        .respond_with(ResponseTemplate::new(200).set_body_string("direct"))
        .expect(2)
        .mount(&server)
        .await;

    for proxy_url in [None, Some(" \t ")] {
        let client = apply_proxy(reqwest::Client::builder(), proxy_url)
            .unwrap()
            .build()
            .unwrap();
        let response = client
            .get(format!("{}/direct", server.uri()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "direct");
    }
}

#[tokio::test]
async fn explicit_http_proxy_is_used_for_non_resolving_target() {
    let proxy = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/through-proxy"))
        .respond_with(ResponseTemplate::new(200).set_body_string("proxied"))
        .expect(1)
        .mount(&proxy)
        .await;

    let client = apply_proxy(reqwest::Client::builder(), Some(&proxy.uri()))
        .unwrap()
        .build()
        .unwrap();
    let response = client
        .get("http://target.invalid/through-proxy")
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "proxied");
}
