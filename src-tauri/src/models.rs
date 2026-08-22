use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Photo,
    Video,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaResource {
    pub url: String,
    pub kind: MediaKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct Post {
    pub pk: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub like_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_pk: Option<String>,
    pub resources: Vec<MediaResource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

/// Normalize `taken_at` which arrives either as epoch seconds (media info
/// endpoints) or ISO-8601 with Z (feed chunk items).
pub fn parse_taken_at(v: &serde_json::Value) -> Option<i64> {
    if let Some(epoch) = v.as_i64() {
        return Some(epoch);
    }
    if let Some(epoch) = v.as_str().and_then(|s| s.parse::<i64>().ok()) {
        return Some(epoch);
    }
    let iso = v.as_str()?;
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.timestamp())
}
