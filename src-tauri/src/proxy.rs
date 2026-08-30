use reqwest::{ClientBuilder, Proxy};
use url::Url;

const INVALID_PROXY: &str = "Enter a valid HTTP, HTTPS, SOCKS5, or SOCKS5H proxy URL";

pub fn normalize_proxy_url(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let mut parsed = Url::parse(value).map_err(|_| INVALID_PROXY.to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https" | "socks5" | "socks5h")
        || parsed.host_str().is_none()
        || parsed.port_or_known_default().is_none()
        || parsed.port() == Some(0)
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(INVALID_PROXY.to_owned());
    }

    parsed.set_path("/");
    let normalized = parsed.to_string();
    Proxy::all(&normalized).map_err(|_| INVALID_PROXY.to_owned())?;
    Ok(Some(normalized))
}

pub fn redact_proxy_url(value: &str) -> Option<String> {
    let normalized = normalize_proxy_url(Some(value)).ok().flatten()?;
    let mut parsed = Url::parse(&normalized).ok()?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        parsed.set_username("***").ok()?;
        parsed.set_password(None).ok()?;
    }
    Some(parsed.to_string())
}

pub fn apply_proxy(
    builder: ClientBuilder,
    proxy_url: Option<&str>,
) -> Result<ClientBuilder, String> {
    let Some(proxy_url) = normalize_proxy_url(proxy_url)? else {
        return Ok(builder);
    };
    let proxy = Proxy::all(proxy_url).map_err(|_| INVALID_PROXY.to_owned())?;
    Ok(builder.proxy(proxy))
}
