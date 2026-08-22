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

    /// GET /v2/media/info/by/code → {"media_or_ad": {...}}
    pub async fn media_by_code(&self, code: &str) -> Result<Value, HikerError> {
        let (v, _) = self.get("/v2/media/info/by/code", &[("code", code)]).await?;
        Ok(v["media_or_ad"].clone())
    }
}

fn str_or_num(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Best-quality URL from `image_versions2.candidates[]` (max width).
fn best_image(item: &serde_json::Value) -> Option<String> {
    item.get("image_versions2")?
        .get("candidates")?
        .as_array()?
        .iter()
        .filter_map(|c| {
            let url = c.get("url")?.as_str()?;
            let width = c.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
            Some((width, url))
        })
        .max_by_key(|(w, _)| *w)
        .map(|(_, u)| u.to_string())
}

/// Best-quality URL from `video_versions[]` (max width).
fn best_video(item: &serde_json::Value) -> Option<String> {
    item.get("video_versions")?
        .as_array()?
        .iter()
        .filter_map(|c| {
            let url = c.get("url")?.as_str()?;
            let width = c.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
            Some((width, url))
        })
        .max_by_key(|(w, _)| *w)
        .map(|(_, u)| u.to_string())
}

/// Collect every downloadable URL from one media object (post or carousel
/// child). Handles both current HikerAPI shapes (`image_versions2` /
/// `video_versions`) and legacy flat fields (`thumbnail_url` / `video_url`,
/// still used by feed chunk endpoints).
fn collect_resources(item: &serde_json::Value, is_video: bool) -> Vec<crate::models::MediaResource> {
    use crate::models::{MediaKind, MediaResource};
    let mut out = Vec::new();
    if is_video {
        if let Some(url) = best_video(item).or_else(|| {
            item.get("video_url")
                .and_then(|v| v.as_str())
                .map(String::from)
        }) {
            out.push(MediaResource { url, kind: MediaKind::Video });
        }
    } else if let Some(url) = best_image(item).or_else(|| {
        item.get("thumbnail_url")
            .and_then(|v| v.as_str())
            .map(String::from)
    }) {
        out.push(MediaResource { url, kind: MediaKind::Photo });
    }
    out
}

/// Map a raw HikerAPI media object onto our Post DTO.
/// Shapes verified live by insta-dl: media_type 1=photo, 2=video,
/// 8=album (`carousel_media` preferred, `resources` fallback).
pub fn map_post(media: &serde_json::Value) -> Option<crate::models::Post> {
    use crate::models::{MediaKind, MediaResource};

    let pk = str_or_num(media.get("pk")?)?;
    let code = media.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let media_type = media.get("media_type").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut resources: Vec<MediaResource> = Vec::new();
    let carousel = media
        .get("carousel_media")
        .and_then(|v| v.as_array())
        .or_else(|| media.get("resources").and_then(|v| v.as_array()));
    match carousel {
        Some(items) => {
            for item in items {
                let is_video = item.get("media_type").and_then(|v| v.as_u64()) == Some(2);
                resources.extend(collect_resources(item, is_video));
            }
        }
        None => {
            resources.extend(collect_resources(media, media_type == 2));
        }
    }

    let thumbnail_url = best_image(media).or_else(|| {
        media
            .get("thumbnail_url")
            .and_then(|v| v.as_str())
            .map(String::from)
    });

    Some(crate::models::Post {
        pk,
        code,
        taken_at: crate::models::parse_taken_at(media.get("taken_at")?),
        caption: media.get("caption_text").and_then(|v| v.as_str()).map(String::from),
        like_count: media.get("like_count").and_then(|v| v.as_u64()),
        comment_count: media.get("comment_count").and_then(|v| v.as_u64()),
        owner_username: media["user"]["username"].as_str().map(String::from),
        owner_pk: media["user"]["pk"].as_str().map(String::from),
        resources,
        thumbnail_url,
    })
}
