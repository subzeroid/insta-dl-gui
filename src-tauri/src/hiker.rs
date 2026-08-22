use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const BASE_URL: &str = "https://api.hikerapi.com";
const QUOTA_REMAINING_HEADERS: [&str; 2] = ["x-quota-remaining", "x-ratelimit-remaining"];

#[derive(Debug, thiserror::Error)]
pub enum HikerError {
    #[error("Invalid token — get a new one at hikerapi.com/tokens")]
    AuthInvalid,
    #[error("Quota exhausted — top up at hikerapi.com")]
    QuotaExhausted,
    #[error("Token banned by HikerAPI")]
    Banned,
    #[error("Not found on Instagram (private profile or deleted post)")]
    NotFound,
    #[error("Rate limited{}", retry_after.map(|s| format!("; retry in {s}s")).unwrap_or_default())]
    RateLimited { retry_after: Option<u64> },
    #[error("Temporary HikerAPI error: {0}")]
    Transient(String),
}

impl HikerError {
    pub fn from_status(status: u16, headers: &reqwest::header::HeaderMap, body: &str) -> Self {
        match status {
            401 => Self::AuthInvalid,
            402 => Self::QuotaExhausted,
            403 => Self::Banned,
            404 => Self::NotFound,
            429 => {
                let retry_after = parse_retry_after(headers);
                Self::RateLimited { retry_after }
            }
            s => Self::Transient(format!("HTTP {s}: {}", truncate(body, 200))),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::AuthInvalid => "AuthInvalid",
            Self::QuotaExhausted => "QuotaExhausted",
            Self::Banned => "Banned",
            Self::NotFound => "NotFound",
            Self::RateLimited { .. } => "RateLimited",
            Self::Transient(_) => "Transient",
        }
    }
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    for name in ["retry-after", "x-quota-reset", "x-ratelimit-reset"] {
        if let Some(v) = headers.get(name).and_then(|v| v.to_str().ok()) {
            if let Ok(secs) = v.trim().parse::<u64>() {
                return Some(secs);
            }
        }
    }
    None
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub requests: u64,
    pub rate: Option<u64>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
}

impl Balance {
    fn from_json(v: &Value) -> Self {
        Self {
            requests: v.get("requests").and_then(Value::as_u64).unwrap_or(0),
            rate: v.get("rate").and_then(Value::as_u64),
            amount: v.get("amount").and_then(Value::as_f64),
            currency: v.get("currency").and_then(Value::as_str).map(String::from),
        }
    }
}

/// Snapshot of quota captured from response headers.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct QuotaHeaders {
    pub remaining: Option<u64>,
}

pub struct HikerClient {
    http: reqwest::Client,
    token: String,
}

impl HikerClient {
    pub fn new(token: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("insta-dl-gui/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client");
        Self { http, token }
    }

    pub async fn get(&self, path: &str, params: &[(&str, &str)]) -> Result<(Value, QuotaHeaders), HikerError> {
        let mut req = self
            .http
            .get(format!("{BASE_URL}{path}"))
            .header("x-access-key", &self.token)
            .header("accept", "application/json");
        for (k, v) in params {
            if !v.is_empty() {
                req = req.query(&[(k, v)]);
            }
        }
        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                HikerError::Transient(e.to_string())
            } else {
                HikerError::Transient(e.to_string())
            }
        })?;
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = resp.text().await.unwrap_or_default();
        if status >= 400 {
            return Err(HikerError::from_status(status, &headers, &body));
        }
        let value: Value = serde_json::from_str(&body)
            .map_err(|e| HikerError::Transient(format!("bad JSON from {path}: {e}")))?;
        let quota = QuotaHeaders {
            remaining: QUOTA_REMAINING_HEADERS.iter().find_map(|h| {
                headers
                    .get(*h)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
            }),
        };
        Ok((value, quota))
    }

    /// GET /sys/balance — also serves as token validation.
    pub async fn balance(&self) -> Result<Balance, HikerError> {
        let (v, _) = self.get("/sys/balance", &[]).await?;
        Ok(Balance::from_json(&v))
    }

    pub async fn user_by_username(&self, username: &str) -> Result<Value, HikerError> {
        let (v, _) = self.get("/v2/user/by/username", &[("username", username)]).await?;
        Ok(v["user"].clone())
    }
}
