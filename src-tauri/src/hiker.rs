use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const BASE_URL: &str = "https://api.hikerapi.com";
const QUOTA_REMAINING_HEADERS: [&str; 2] = ["x-quota-remaining", "x-ratelimit-remaining"];

#[derive(Debug, thiserror::Error)]
pub enum HikerError {
    #[error("Invalid token — get a new one at https://hikerapi.com/p/uk064a1b")]
    AuthInvalid,
    #[error("Quota exhausted — top up at https://hikerapi.com/p/uk064a1b")]
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

fn parse_post_chunk(v: Value, label: &str) -> Result<crate::models::PostPage, HikerError> {
    let arr = v
        .as_array()
        .ok_or_else(|| HikerError::Transient(format!("{label}: expected [items, cursor]")))?;
    let items = arr
        .first()
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let end_cursor = arr.get(1).and_then(Value::as_str).map(String::from);
    Ok(crate::models::PostPage {
        posts: items.iter().filter_map(map_post).collect(),
        end_cursor,
    })
}

fn parse_user_chunk(v: Value, label: &str) -> Result<crate::models::UserPage, HikerError> {
    let arr = v
        .as_array()
        .ok_or_else(|| HikerError::Transient(format!("{label}: expected [users, cursor]")))?;
    let users = arr
        .first()
        .and_then(Value::as_array)
        .ok_or_else(|| HikerError::Transient(format!("{label}: expected users array")))?;
    let next_cursor = arr
        .get(1)
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.trim().is_empty())
        .map(String::from);
    Ok(crate::models::UserPage {
        users: users.iter().filter_map(map_search_user).collect(),
        next_cursor,
    })
}

fn parse_users(v: Value, label: &str) -> Result<Vec<crate::models::SearchUser>, HikerError> {
    let users = v
        .as_array()
        .ok_or_else(|| HikerError::Transient(format!("{label}: expected users array")))?;
    Ok(users.iter().filter_map(map_search_user).collect())
}

/// Snapshot of quota captured from response headers.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct QuotaHeaders {
    pub remaining: Option<u64>,
}

pub struct HikerClient {
    http: reqwest::Client,
    token: String,
    base_url: String,
}

impl HikerClient {
    pub fn new(token: String) -> Self {
        Self::with_base_url(token, BASE_URL.to_string())
    }

    pub fn with_base_url(token: String, base_url: String) -> Self {
        Self::with_base_url_and_proxy(token, base_url, None).expect("reqwest client")
    }

    pub fn with_proxy(token: String, proxy_url: Option<&str>) -> Result<Self, String> {
        Self::with_base_url_and_proxy(token, BASE_URL.to_string(), proxy_url)
    }

    pub fn with_base_url_and_proxy(
        token: String,
        base_url: String,
        proxy_url: Option<&str>,
    ) -> Result<Self, String> {
        let builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("insta-dl-gui/", env!("CARGO_PKG_VERSION")));
        let http = crate::proxy::apply_proxy(builder, proxy_url)?
            .build()
            .map_err(|_| "Could not configure the HikerAPI client".to_owned())?;
        Ok(Self {
            http,
            token,
            base_url,
        })
    }

    pub async fn get(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<(Value, QuotaHeaders), HikerError> {
        let mut req = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .header("x-access-key", &self.token)
            .header("accept", "application/json");
        for (k, v) in params {
            if !v.is_empty() {
                req = req.query(&[(k, v)]);
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| HikerError::Transient(e.to_string()))?;
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
        let (v, _) = self
            .get("/v2/user/by/username", &[("username", username)])
            .await?;
        Ok(v["user"].clone())
    }

    /// GET /v1/user/medias/chunk → `[items, end_cursor]` (a list, not dict).
    pub async fn user_medias_chunk(
        &self,
        user_id: &str,
        end_cursor: Option<&str>,
    ) -> Result<crate::models::PostPage, HikerError> {
        let (v, _) = self
            .get(
                "/v1/user/medias/chunk",
                &[
                    ("user_id", user_id),
                    ("end_cursor", end_cursor.unwrap_or("")),
                ],
            )
            .await?;
        parse_post_chunk(v, "medias/chunk")
    }

    /// GET /v1/user/clips/chunk → `[items, end_cursor]`.
    pub async fn user_clips_chunk(
        &self,
        user_id: &str,
        end_cursor: Option<&str>,
    ) -> Result<crate::models::PostPage, HikerError> {
        let (v, _) = self
            .get(
                "/v1/user/clips/chunk",
                &[
                    ("user_id", user_id),
                    ("end_cursor", end_cursor.unwrap_or("")),
                ],
            )
            .await?;
        parse_post_chunk(v, "clips/chunk")
    }

    pub async fn user_followers_chunk(
        &self,
        user_id: &str,
        max_id: Option<&str>,
    ) -> Result<crate::models::UserPage, HikerError> {
        let (v, _) = self
            .get(
                "/v1/user/followers/chunk",
                &[("user_id", user_id), ("max_id", max_id.unwrap_or(""))],
            )
            .await?;
        parse_user_chunk(v, "followers/chunk")
    }

    pub async fn user_following_chunk(
        &self,
        user_id: &str,
        max_id: Option<&str>,
    ) -> Result<crate::models::UserPage, HikerError> {
        let (v, _) = self
            .get(
                "/v1/user/following/chunk",
                &[("user_id", user_id), ("max_id", max_id.unwrap_or(""))],
            )
            .await?;
        parse_user_chunk(v, "following/chunk")
    }

    pub async fn search_followers(
        &self,
        user_id: &str,
        query: &str,
    ) -> Result<Vec<crate::models::SearchUser>, HikerError> {
        let (v, _) = self
            .get(
                "/v1/user/search/followers",
                &[("user_id", user_id), ("query", query)],
            )
            .await?;
        parse_users(v, "search/followers")
    }

    pub async fn search_following(
        &self,
        user_id: &str,
        query: &str,
    ) -> Result<Vec<crate::models::SearchUser>, HikerError> {
        let (v, _) = self
            .get(
                "/v1/user/search/following",
                &[("user_id", user_id), ("query", query)],
            )
            .await?;
        parse_users(v, "search/following")
    }

    /// GET /v2/user/stories (billed 2 requests) → `{"reel": {"items": [...]}}`.
    pub async fn user_stories(&self, user_id: &str) -> Result<Vec<Value>, HikerError> {
        let (v, _) = self
            .get("/v2/user/stories", &[("user_id", user_id)])
            .await?;
        let empty = Vec::new();
        Ok(v.get("reel")
            .and_then(|r| r.get("items"))
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or(empty))
    }

    /// GET /v3/fbsearch/accounts — account autocomplete. Order is the API's
    /// own (same surface Instagram's search uses); we deliberately do NOT
    /// rerank it client-side so results feel identical to the real app.
    pub async fn search_accounts(&self, query: &str) -> Result<Vec<Value>, HikerError> {
        let (v, _) = self
            .get("/v3/fbsearch/accounts", &[("query", query)])
            .await?;
        let empty = Vec::new();
        Ok(v.get("users")
            .and_then(|u| u.as_array())
            .cloned()
            .unwrap_or(empty))
    }

    /// GET /v2/user/highlights (billed 2 requests) → `{"response": {"tray": [...]}}`.
    pub async fn user_highlights(&self, user_id: &str) -> Result<Vec<Value>, HikerError> {
        let (v, _) = self
            .get("/v2/user/highlights", &[("user_id", user_id)])
            .await?;
        let empty = Vec::new();
        Ok(v["response"]["tray"].as_array().cloned().unwrap_or(empty))
    }

    /// GET /v2/highlight/by/id (id = bare numeric pk) →
    /// `{"response": {"reels": {"highlight:<pk>": {"items": [...]}}}}`.
    pub async fn highlight_items(&self, highlight_pk: &str) -> Result<Vec<Value>, HikerError> {
        let (v, _) = self
            .get("/v2/highlight/by/id", &[("id", highlight_pk)])
            .await?;
        let empty = Vec::new();
        let reels = v["response"]["reels"].as_object();
        Ok(reels
            .and_then(|m| m.values().next())
            .and_then(|reel| reel.get("items"))
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or(empty))
    }

    /// GET /v2/media/info/by/code → {"media_or_ad": {...}}
    pub async fn media_by_code(&self, code: &str) -> Result<Value, HikerError> {
        let (v, _) = self
            .get("/v2/media/info/by/code", &[("code", code)])
            .await?;
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
pub(crate) fn best_image(item: &serde_json::Value) -> Option<String> {
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
pub(crate) fn best_video(item: &serde_json::Value) -> Option<String> {
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
pub fn collect_resources(
    item: &serde_json::Value,
    is_video: bool,
) -> Vec<crate::models::MediaResource> {
    use crate::models::{MediaKind, MediaResource};
    let mut out = Vec::new();
    if is_video {
        if let Some(url) = best_video(item).or_else(|| {
            item.get("video_url")
                .and_then(|v| v.as_str())
                .map(String::from)
        }) {
            out.push(MediaResource {
                url,
                kind: MediaKind::Video,
            });
        }
    } else if let Some(url) = best_image(item).or_else(|| {
        item.get("thumbnail_url")
            .and_then(|v| v.as_str())
            .map(String::from)
    }) {
        out.push(MediaResource {
            url,
            kind: MediaKind::Photo,
        });
    }
    out
}

/// Map a raw HikerAPI user object onto our Profile DTO.
pub fn map_profile(user: &serde_json::Value) -> Option<crate::models::Profile> {
    let pk = str_or_num(user.get("pk")?)?;
    let username = user.get("username").and_then(|v| v.as_str())?.to_string();
    let avatar_url = user
        .get("profile_pic_url_hd")
        .and_then(|v| v.as_str())
        .or_else(|| user.get("profile_pic_url").and_then(|v| v.as_str()))
        .map(String::from);
    Some(crate::models::Profile {
        pk,
        username,
        full_name: user
            .get("full_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        media_count: user
            .get("media_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        follower_count: user.get("follower_count").and_then(|v| v.as_u64()),
        following_count: user.get("following_count").and_then(|v| v.as_u64()),
        is_private: user
            .get("is_private")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        is_verified: user
            .get("is_verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        avatar_url,
    })
}

/// Map a raw HikerAPI media object onto our Post DTO.
/// Shapes verified live by insta-dl: media_type 1=photo, 2=video,
/// 8=album (`carousel_media` preferred, `resources` fallback).
pub fn map_post(media: &serde_json::Value) -> Option<crate::models::Post> {
    use crate::models::MediaResource;

    let pk = str_or_num(media.get("pk")?)?;
    let code = media
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let media_type = media
        .get("media_type")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut resources: Vec<MediaResource> = Vec::new();
    let carousel = media
        .get("carousel_media")
        .and_then(|v| v.as_array())
        .filter(|items| !items.is_empty())
        .or_else(|| {
            media
                .get("resources")
                .and_then(|v| v.as_array())
                .filter(|items| !items.is_empty())
        });
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
        caption: media
            .get("caption_text")
            .and_then(|v| v.as_str())
            .map(String::from),
        like_count: media.get("like_count").and_then(|v| v.as_u64()),
        comment_count: media.get("comment_count").and_then(|v| v.as_u64()),
        owner_username: media["user"]["username"].as_str().map(String::from),
        owner_pk: media["user"]["pk"].as_str().map(String::from),
        resources,
        thumbnail_url,
    })
}

/// Map an fbsearch user object onto our SearchUser DTO.
pub fn map_search_user(u: &Value) -> Option<crate::models::SearchUser> {
    let pk = str_or_num(u.get("pk")?)?;
    let username = u.get("username").and_then(|v| v.as_str())?.to_string();
    Some(crate::models::SearchUser {
        pk,
        username,
        full_name: u
            .get("full_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        is_verified: u
            .get("is_verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        is_private: u
            .get("is_private")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        avatar_url: u
            .get("profile_pic_url")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn followers_chunk_uses_max_id_and_maps_the_safe_user_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/user/followers/chunk"))
            .and(query_param("user_id", "42"))
            .and(query_param("max_id", "cursor-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                [
                    {
                        "pk": 7,
                        "username": "runner",
                        "full_name": "Runner",
                        "profile_pic_url": "https://cdninstagram.com/runner.jpg",
                        "is_private": false,
                        "is_verified": true
                    },
                    { "pk": "invalid-without-username" }
                ],
                "cursor-2"
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let client = HikerClient::with_base_url("token".into(), server.uri());

        let page = client
            .user_followers_chunk("42", Some("cursor-1"))
            .await
            .unwrap();

        assert_eq!(page.next_cursor.as_deref(), Some("cursor-2"));
        assert_eq!(page.users.len(), 1);
        assert_eq!(page.users[0].pk, "7");
        assert_eq!(page.users[0].username, "runner");
        assert!(page.users[0].is_verified);
    }

    #[tokio::test]
    async fn following_chunk_uses_its_dedicated_endpoint_and_empty_initial_cursor() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/user/following/chunk"))
            .and(query_param("user_id", "42"))
            .and(query_param_is_missing("max_id"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                [{ "pk": "8", "username": "sprinter", "is_private": true }],
                null
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let client = HikerClient::with_base_url("token".into(), server.uri());

        let page = client.user_following_chunk("42", None).await.unwrap();

        assert_eq!(page.next_cursor, None);
        assert_eq!(page.users.len(), 1);
        assert_eq!(page.users[0].username, "sprinter");
        assert!(page.users[0].is_private);
    }

    #[tokio::test]
    async fn follower_search_uses_the_dedicated_server_side_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/user/search/followers"))
            .and(query_param("user_id", "42"))
            .and(query_param("query", "run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "pk": "7", "username": "runner", "full_name": "Runner" }
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let client = HikerClient::with_base_url("token".into(), server.uri());

        let users = client.search_followers("42", "run").await.unwrap();

        assert_eq!(users.len(), 1);
        assert_eq!(users[0].username, "runner");
    }

    #[tokio::test]
    async fn following_search_uses_the_dedicated_server_side_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/user/search/following"))
            .and(query_param("user_id", "42"))
            .and(query_param("query", "meta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "pk": "9", "username": "meta", "full_name": "Meta" }
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let client = HikerClient::with_base_url("token".into(), server.uri());

        let users = client.search_following("42", "meta").await.unwrap();

        assert_eq!(users.len(), 1);
        assert_eq!(users[0].username, "meta");
    }

    #[test]
    fn profile_mapping_includes_the_following_count() {
        let profile = map_profile(&serde_json::json!({
            "pk": "42",
            "username": "nike",
            "media_count": 10,
            "follower_count": 20,
            "following_count": 30
        }))
        .unwrap();

        assert_eq!(profile.follower_count, Some(20));
        assert_eq!(profile.following_count, Some(30));
    }
}
