use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaItemKind {
    Post,
    Reel,
    Story,
    Avatar,
}

impl MediaItemKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Post => "post",
            Self::Reel => "reel",
            Self::Story => "story",
            Self::Avatar => "avatar",
        }
    }

    pub(crate) fn from_db(value: &str) -> Option<Self> {
        match value {
            "post" => Some(Self::Post),
            "reel" => Some(Self::Reel),
            "story" => Some(Self::Story),
            "avatar" => Some(Self::Avatar),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaFileKind {
    Photo,
    Video,
    Metadata,
    Unknown,
}

impl MediaFileKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Video => "video",
            Self::Metadata => "metadata",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn from_db(value: &str) -> Option<Self> {
        match value {
            "photo" => Some(Self::Photo),
            "video" => Some(Self::Video),
            "metadata" => Some(Self::Metadata),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAvailability {
    Available,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySort {
    TakenAtDesc,
    ImportedAtDesc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryQuery {
    pub search: Option<String>,
    pub kinds: Vec<MediaItemKind>,
    pub source_id: Option<i64>,
    pub availability: Option<FileAvailability>,
    pub taken_after: Option<i64>,
    pub taken_before: Option<i64>,
    pub sort: LibrarySort,
    pub cursor: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryPage {
    pub items: Vec<LibraryCard>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LibraryCursor {
    pub version: u8,
    pub sort_value: i64,
    pub id: i64,
    pub scope: LibraryCursorScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LibraryCursorScope {
    pub sort: LibrarySort,
    pub search: Option<String>,
    pub kinds: Vec<MediaItemKind>,
    pub source_id: Option<i64>,
    pub availability: Option<FileAvailability>,
    pub taken_after: Option<i64>,
    pub taken_before: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryRoot {
    pub id: i64,
    pub path: PathBuf,
    pub label: String,
    pub created_at: i64,
    pub last_scan_started_at: Option<i64>,
    pub last_scan_completed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogMediaInput {
    pub remote_key: String,
    pub kind: MediaItemKind,
    pub remote_pk: Option<String>,
    pub shortcode: Option<String>,
    pub owner_pk: Option<String>,
    pub owner_username: Option<String>,
    pub taken_at: Option<i64>,
    pub caption: Option<String>,
    pub like_count: Option<i64>,
    pub comment_count: Option<i64>,
    pub imported_at: i64,
    pub updated_at: i64,
    pub files: Vec<CatalogFileInput>,
    pub source_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogFileInput {
    pub root_id: i64,
    pub relative_path: PathBuf,
    pub ordinal: i64,
    pub kind: MediaFileKind,
    pub byte_size: i64,
    pub mtime: i64,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpsertDisposition {
    Inserted,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertResult {
    pub media_item_id: i64,
    pub file_ids: Vec<i64>,
    pub disposition: UpsertDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryPreview {
    pub file_id: i64,
    pub kind: MediaFileKind,
    pub relative_path: String,
    pub exists_on_disk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryCard {
    pub id: i64,
    pub remote_key: String,
    pub kind: MediaItemKind,
    pub shortcode: Option<String>,
    pub owner_username: Option<String>,
    pub taken_at: Option<i64>,
    pub caption: Option<String>,
    pub imported_at: i64,
    pub updated_at: i64,
    pub preview: Option<LibraryPreview>,
    pub resource_count: u32,
    pub availability: FileAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryFile {
    pub id: i64,
    pub root_id: i64,
    pub relative_path: String,
    pub ordinal: i64,
    pub kind: MediaFileKind,
    pub byte_size: i64,
    pub mtime: i64,
    pub exists_on_disk: bool,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryItemDetail {
    pub id: i64,
    pub remote_key: String,
    pub kind: MediaItemKind,
    pub remote_pk: Option<String>,
    pub shortcode: Option<String>,
    pub owner_pk: Option<String>,
    pub owner_username: Option<String>,
    pub taken_at: Option<i64>,
    pub caption: Option<String>,
    pub like_count: Option<i64>,
    pub comment_count: Option<i64>,
    pub imported_at: i64,
    pub updated_at: i64,
    pub files: Vec<LibraryFile>,
    pub source_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCatalogFile {
    pub file_id: i64,
    pub media_item_id: i64,
    pub root_id: i64,
    pub root_path: PathBuf,
    pub relative_path: PathBuf,
    pub kind: MediaFileKind,
    pub exists_on_disk: bool,
}
