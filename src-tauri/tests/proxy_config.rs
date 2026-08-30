use insta_dl_gui_lib::config::Config;
use insta_dl_gui_lib::proxy::normalize_proxy_url;

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
fn proxy_hint_redacts_password_without_losing_connection_context() {
    let config = Config {
        proxy_url: Some("socks5h://user:secret@proxy.example:1080".to_owned()),
        ..Config::default()
    };

    let hint = config.proxy_hint().unwrap();
    assert!(hint.starts_with("socks5h://user:"));
    assert!(hint.contains("proxy.example:1080"));
    assert!(!hint.contains("secret"));
}

#[test]
fn config_debug_output_does_not_expose_proxy_password() {
    let config = Config {
        proxy_url: Some("socks5h://user:secret@proxy.example:1080".to_owned()),
        ..Config::default()
    };

    assert!(!format!("{config:?}").contains("secret"));
}
