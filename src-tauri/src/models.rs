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
pub struct Profile {
    pub pk: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    pub media_count: u64,
    pub follower_count: Option<u64>,
    pub is_private: bool,
    pub is_verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchUser {
    pub pk: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    pub is_verified: bool,
    pub is_private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostPage {
    pub posts: Vec<Post>,
    pub end_cursor: Option<String>,
}

/// What the user asked to download from a profile.
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileOptions {
    pub posts: bool,
    pub reels: bool,
    pub stories: bool,
    pub highlights: bool,
    pub avatar: bool,
    #[serde(default)]
    pub max_posts: Option<u64>,
}

/// One active-story item for the Explorer grid.
#[derive(Debug, Clone, Serialize)]
pub struct StoryItem {
    pub pk: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_at: Option<i64>,
    pub kind: String,
    pub media_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
}

/// An already-fetched resource the frontend wants on disk (single story,
/// avatar, …) without another HikerAPI call.
#[derive(Debug, Clone, Deserialize)]
pub struct DirectItem {
    pub url: String,
    #[serde(default)]
    pub taken_at: Option<i64>,
    pub pk: String,
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
