use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::catalog::{
    Catalog, CatalogFileInput, CatalogItemMetadata, MediaFileKind, MediaItemKind,
};
use crate::cdn::{self, CdnError};
use crate::config::Config;
use crate::hiker::{map_post, map_profile, map_search_user};
use crate::jobs::JobRegistry;
use crate::models::{DirectItem, Post, Profile, ProfileOptions, SearchUser, StoryItem};
use crate::targets::Target;
use crate::AppState;

const DOWNLOAD_ATTEMPTS: usize = 3;
#[allow(dead_code)]
const MAX_FETCHED_POSTS: usize = 500;
#[allow(dead_code)]
const MAX_RESOURCES_PER_POST: usize = 20;
#[allow(dead_code)]
const MAX_SHORTCODE_BYTES: usize = 256;

#[allow(dead_code)]
fn validate_fetched_posts(posts: Vec<Post>, allow_loopback: bool) -> Result<Vec<Post>, String> {
    if posts.is_empty() {
        return Err("Fetched post batch must not be empty".into());
    }
    if posts.len() > MAX_FETCHED_POSTS {
        return Err("Fetched post batch exceeds maximum of 500 posts".into());
    }

    let mut seen = HashSet::new();
    let mut validated = Vec::new();
    for post in posts {
        if post.pk.is_empty() || !post.pk.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("Fetched post PK must contain only ASCII digits".into());
        }
        if post.code.is_empty() {
            return Err("Fetched post shortcode must not be empty".into());
        }
        if post.code.len() > MAX_SHORTCODE_BYTES {
            return Err("Fetched post shortcode exceeds maximum of 256 bytes".into());
        }
        if post.resources.is_empty() || post.resources.len() > MAX_RESOURCES_PER_POST {
            return Err("Fetched post must contain between 1 and 20 resources".into());
        }
        for resource in &post.resources {
            cdn::validate_remote_url(&resource.url, allow_loopback)
                .map_err(|_| "Fetched post contains an invalid media URL".to_string())?;
        }

        if seen.insert(post.pk.clone()) {
            validated.push(post);
        }
    }

    Ok(validated)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
enum JobState {
    Downloading {
        current_file: usize,
        total_files: usize,
        bytes_done: u64,
        file_name: String,
    },
    Done {
        count: usize,
        dir: String,
        #[serde(skip_serializing_if = "is_zero")]
        catalog_warnings: usize,
        #[serde(skip_serializing_if = "is_zero")]
        resource_failures: usize,
    },
    Failed {
        error: String,
    },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
struct JobProgress {
    job_id: String,
    label: String,
    #[serde(flatten)]
    state: JobState,
}

/// Owns the job identity and emits typed progress events for it.
struct JobEvents {
    app: AppHandle,
    job_id: String,
    label: String,
}

impl JobEvents {
    fn new(app: &AppHandle, job_id: String, label: String) -> Self {
        Self {
            app: app.clone(),
            job_id,
            label,
        }
    }

    fn progress(&self, current_file: usize, total_files: usize, bytes_done: u64, file_name: &str) {
        self.app
            .emit(
                "job-progress",
                &JobProgress {
                    job_id: self.job_id.clone(),
                    label: self.label.clone(),
                    state: JobState::Downloading {
                        current_file,
                        total_files,
                        bytes_done,
                        file_name: file_name.to_string(),
                    },
                },
            )
            .ok();
    }

    fn done(&self, outcome: JobOutcome, resource_failures: usize, dir: &Path) {
        self.app
            .emit(
                "job-progress",
                &JobProgress {
                    job_id: self.job_id.clone(),
                    label: self.label.clone(),
                    state: JobState::Done {
                        count: outcome.files_written,
                        dir: dir.to_string_lossy().into_owned(),
                        catalog_warnings: outcome.catalog_warnings,
                        resource_failures,
                    },
                },
            )
            .ok();
    }

    fn failed(&self, error: String) {
        self.app
            .emit(
                "job-progress",
                &JobProgress {
                    job_id: self.job_id.clone(),
                    label: self.label.clone(),
                    state: JobState::Failed { error },
                },
            )
            .ok();
    }

    fn cancelled(&self) {
        self.app
            .emit(
                "job-progress",
                &JobProgress {
                    job_id: self.job_id.clone(),
                    label: self.label.clone(),
                    state: JobState::Cancelled,
                },
            )
            .ok();
    }
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

trait ProgressSink: Sync {
    fn progress(&self, current_file: usize, total_files: usize, bytes_done: u64, file_name: &str);
}

impl ProgressSink for JobEvents {
    fn progress(&self, current_file: usize, total_files: usize, bytes_done: u64, file_name: &str) {
        JobEvents::progress(self, current_file, total_files, bytes_done, file_name);
    }
}

#[cfg(test)]
struct NoopProgress;

#[cfg(test)]
impl ProgressSink for NoopProgress {
    fn progress(
        &self,
        _current_file: usize,
        _total_files: usize,
        _bytes_done: u64,
        _file_name: &str,
    ) {
    }
}

fn taken_at_name(ts: Option<i64>, fallback_code: &str) -> String {
    let base = match ts {
        Some(unix) => chrono::DateTime::<chrono::Utc>::from_timestamp(unix, 0).map(|u| {
            u.with_timezone(&chrono::Local)
                .format("%Y-%m-%d_%H-%M-%S")
                .to_string()
        }),
        None => None,
    }
    .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string());
    format!("{base}_{}", safe_segment(fallback_code))
}

fn safe_segment(s: &str) -> String {
    let sanitized: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.@".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "_".to_string()
    } else {
        sanitized
    }
}

/// Collect existing file stems so already-downloaded items are skipped.
fn existing_stems(dir: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                set.insert(stem.to_string());
            }
        }
    }
    set
}

fn stem_exists(set: &HashSet<String>, base: &str) -> bool {
    if set.contains(base) {
        return true;
    }
    set.iter()
        .any(|s| s.len() > base.len() && s.starts_with(base) && s[base.len()..].starts_with('_'))
}

fn write_sidecar(
    cfg: &Config,
    dir: &Path,
    post: &Post,
    first_file: &Path,
    catalog_item: &CatalogItemMetadata,
) -> Result<Option<PathBuf>, String> {
    if !cfg.sidecar {
        return Ok(None);
    }
    let stem = first_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("post");
    let sidecar = serde_json::json!({
        "pk": post.pk,
        "code": post.code,
        "taken_at": post.taken_at,
        "caption": post.caption,
        "like_count": post.like_count,
        "comment_count": post.comment_count,
        "owner": {
            "pk": post.owner_pk,
            "username": post.owner_username,
        },
        "catalog": {
            "version": 1,
            "remote_key": catalog_item.remote_key,
            "item_kind": catalog_item.kind,
        },
    });
    let path = dir.join(format!("{stem}.json"));
    let json = serde_json::to_vec_pretty(&sidecar).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(Some(path))
}

#[derive(Debug, Clone)]
struct DownloadedFile {
    path: PathBuf,
    bytes: u64,
    kind: MediaFileKind,
    ordinal: u32,
}

#[derive(Debug, Clone)]
struct DownloadedMedia {
    item: CatalogItemMetadata,
    files: Vec<DownloadedFile>,
    resource_errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct JobOutcome {
    files_written: usize,
    catalog_warnings: usize,
}

struct CompletedJob {
    outcome: JobOutcome,
    resource_errors: Vec<String>,
}

fn job_outcome(files_written: usize, catalog_warnings: usize) -> JobOutcome {
    JobOutcome {
        files_written,
        catalog_warnings,
    }
}

enum JobFail {
    Cancelled,
    Fatal(String),
}

impl From<CdnError> for JobFail {
    fn from(e: CdnError) -> Self {
        match e {
            CdnError::Cancelled => Self::Cancelled,
            other => Self::Fatal(other.to_string()),
        }
    }
}

impl From<std::io::Error> for JobFail {
    fn from(e: std::io::Error) -> Self {
        Self::Fatal(e.to_string())
    }
}

impl From<String> for JobFail {
    fn from(e: String) -> Self {
        Self::Fatal(e)
    }
}

fn finish_downloads(
    outcome: JobOutcome,
    last_error: Option<String>,
) -> Result<JobOutcome, JobFail> {
    match (outcome.files_written, last_error) {
        (0, Some(error)) => Err(JobFail::Fatal(error)),
        _ => Ok(outcome),
    }
}

fn finish_completed_job(
    outcome: JobOutcome,
    last_error: Option<String>,
    resource_errors: Vec<String>,
    has_successful_resource: bool,
) -> Result<CompletedJob, JobFail> {
    let outcome = if has_successful_resource || outcome.files_written > 0 {
        outcome
    } else {
        finish_downloads(outcome, last_error)?
    };
    Ok(CompletedJob {
        outcome,
        resource_errors,
    })
}

fn post_job_key(code: &str) -> String {
    format!("post:{code}")
}

fn direct_job_key(label: &str, subfolder: &str, items: &[DirectItem]) -> String {
    let mut item_pks: Vec<&str> = items.iter().map(|item| item.pk.as_str()).collect();
    item_pks.sort_unstable();
    let item_key = serde_json::to_string(&item_pks).expect("serializing string identifiers");
    format!(
        "direct:{}:{}:{}",
        safe_segment(&label.to_ascii_lowercase()),
        safe_segment(&subfolder.to_ascii_lowercase()),
        item_key
    )
}

fn is_fatal_api_error(e: &crate::hiker::HikerError) -> bool {
    matches!(
        e.code(),
        "AuthInvalid" | "QuotaExhausted" | "Banned" | "NotFound" | "RateLimited"
    )
}

/// Stream one resource into `dest_base` with transient-error retries,
/// forwarding progress and honouring cancellation.
#[allow(clippy::too_many_arguments)]
async fn download_one(
    cdn_http: &reqwest::Client,
    url: &str,
    dest_base: &Path,
    taken_at: Option<i64>,
    em: &dyn ProgressSink,
    file_no: usize,
    ordinal: u32,
    bytes_so_far: &mut u64,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
    allow_loopback: bool,
) -> Result<DownloadedFile, JobFail> {
    let name = dest_base
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let progress = |bytes| em.progress(file_no, 0, *bytes_so_far + bytes, &name);
    #[cfg(test)]
    let outcome = if allow_loopback {
        cdn::stream_to_file_retried_for_test(
            cdn_http,
            url,
            dest_base,
            taken_at,
            progress,
            cancel,
            DOWNLOAD_ATTEMPTS,
        )
        .await
    } else {
        cdn::stream_to_file_retried(
            cdn_http,
            url,
            dest_base,
            taken_at,
            progress,
            cancel,
            DOWNLOAD_ATTEMPTS,
        )
        .await
    };
    #[cfg(not(test))]
    let outcome = {
        debug_assert!(
            !allow_loopback,
            "release downloads must use the CDN allowlist"
        );
        cdn::stream_to_file_retried(
            cdn_http,
            url,
            dest_base,
            taken_at,
            progress,
            cancel,
            DOWNLOAD_ATTEMPTS,
        )
        .await
    };
    let outcome = outcome.map_err(JobFail::from)?;
    *bytes_so_far += outcome.bytes;
    Ok(DownloadedFile {
        kind: media_file_kind_from_path(&outcome.path),
        path: outcome.path,
        bytes: outcome.bytes,
        ordinal,
    })
}

fn media_file_kind_from_path(path: &Path) -> MediaFileKind {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg" | "png" | "webp") => MediaFileKind::Photo,
        Some("mp4" | "mov") => MediaFileKind::Video,
        _ => MediaFileKind::Unknown,
    }
}

async fn recover_downloaded_file(
    catalog: &Catalog,
    destination_root: &Path,
    remote_key: &str,
    ordinal: u32,
) -> Option<DownloadedFile> {
    let catalog = catalog.clone();
    let destination_root = destination_root.to_path_buf();
    let remote_key = remote_key.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        recover_downloaded_file_blocking(&catalog, &destination_root, &remote_key, ordinal)
    })
    .await
    .ok()
    .flatten()
}

fn recover_downloaded_file_blocking(
    catalog: &Catalog,
    destination_root: &Path,
    remote_key: &str,
    ordinal: u32,
) -> Option<DownloadedFile> {
    let ordinal_i64 = i64::from(ordinal);
    let evidence = catalog.recovery_file(remote_key, ordinal_i64).ok()??;
    if evidence.ordinal != ordinal_i64 {
        return None;
    }
    let canonical_root = destination_root.canonicalize().ok()?;
    let registered_root = evidence.root_path.canonicalize().ok()?;
    if registered_root != canonical_root {
        return None;
    }
    let candidate = canonical_root.join(&evidence.relative_path);
    let metadata = std::fs::symlink_metadata(&candidate).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    let canonical_file = candidate.canonicalize().ok()?;
    canonical_file.strip_prefix(&canonical_root).ok()?;
    let expected_bytes = u64::try_from(evidence.byte_size).ok()?;
    if metadata.len() != expected_bytes
        || media_file_kind_from_path(&canonical_file) != evidence.kind
    {
        return None;
    }
    Some(DownloadedFile {
        path: canonical_file,
        bytes: expected_bytes,
        kind: evidence.kind,
        ordinal,
    })
}

fn recover_sidecar(
    files: &[DownloadedFile],
    ordinal: u32,
    catalog_item: &CatalogItemMetadata,
) -> Option<DownloadedFile> {
    files.iter().find_map(|file| {
        let path = file.path.with_extension("json");
        (path.is_file() && sidecar_catalog_hint_matches(&path, catalog_item))
            .then(|| downloaded_sidecar(path, ordinal).ok())
            .flatten()
    })
}

fn sidecar_catalog_hint_matches(path: &Path, catalog_item: &CatalogItemMetadata) -> bool {
    const MAX_SIDECAR_BYTES: u64 = 4 * 1024 * 1024;

    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if metadata.len() > MAX_SIDECAR_BYTES {
        return false;
    }
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or_default());
    if file
        .take(MAX_SIDECAR_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_SIDECAR_BYTES
    {
        return false;
    }
    let Ok(sidecar) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    if sidecar.get("pk").and_then(serde_json::Value::as_str) != catalog_item.remote_pk.as_deref() {
        return false;
    }
    let Some(hint) = sidecar.get("catalog") else {
        return false;
    };
    hint.get("version").and_then(serde_json::Value::as_u64) == Some(1)
        && hint.get("remote_key").and_then(serde_json::Value::as_str)
            == Some(catalog_item.remote_key.as_str())
        && hint.get("item_kind").and_then(serde_json::Value::as_str)
            == Some(catalog_item.kind.as_str())
}

fn unix_now() -> i64 {
    system_time_seconds(SystemTime::now())
}

fn system_time_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(error) => -i64::try_from(error.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

fn downloaded_sidecar(path: PathBuf, ordinal: u32) -> Result<DownloadedFile, String> {
    let bytes = std::fs::metadata(&path)
        .map_err(|error| error.to_string())?
        .len();
    Ok(DownloadedFile {
        path,
        bytes,
        kind: MediaFileKind::Metadata,
        ordinal,
    })
}

fn post_catalog_metadata(post: &Post) -> CatalogItemMetadata {
    // `Post` has no explicit remote media_type. A single verified API video
    // resource is the narrow reel signal available here; multiple resources
    // always describe one carousel post, even when some or all are videos.
    let kind = if matches!(
        post.resources.as_slice(),
        [resource] if resource.kind == crate::models::MediaKind::Video
    ) {
        MediaItemKind::Reel
    } else {
        MediaItemKind::Post
    };
    CatalogItemMetadata {
        // The remote pk is the durable identity. Shortcodes can change in
        // mapped fixtures and must never be substituted here.
        remote_key: format!("post:{}", post.pk),
        kind,
        remote_pk: Some(post.pk.clone()),
        shortcode: Some(post.code.clone()),
        owner_pk: post.owner_pk.clone(),
        owner_username: post.owner_username.clone(),
        taken_at: post.taken_at,
        caption: post.caption.clone(),
        like_count: post
            .like_count
            .map(|count| i64::try_from(count).unwrap_or(i64::MAX)),
        comment_count: post
            .comment_count
            .map(|count| i64::try_from(count).unwrap_or(i64::MAX)),
    }
}

fn direct_catalog_metadata(
    label: &str,
    subfolder: &str,
    item: &DirectItem,
) -> Option<CatalogItemMetadata> {
    if item.pk.is_empty() || !item.pk.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let context = subfolder.to_ascii_lowercase();
    let (namespace, kind) = match context.as_str() {
        "stories" | "story" => ("story", MediaItemKind::Story),
        "avatar" | "propic" => ("avatar", MediaItemKind::Avatar),
        // Highlights and generic direct downloads do not carry enough
        // trustworthy identity. The archive rescan will group them safely.
        _ => return None,
    };
    Some(CatalogItemMetadata {
        remote_key: format!("{namespace}:{}", item.pk),
        kind,
        remote_pk: Some(item.pk.clone()),
        shortcode: None,
        owner_pk: None,
        owner_username: Some(label.to_owned()),
        taken_at: item.taken_at,
        caption: None,
        like_count: None,
        comment_count: None,
    })
}

fn avatar_catalog_metadata(profile: &Profile) -> CatalogItemMetadata {
    CatalogItemMetadata {
        remote_key: format!("avatar:{}", profile.pk),
        kind: MediaItemKind::Avatar,
        remote_pk: Some(profile.pk.clone()),
        shortcode: None,
        owner_pk: Some(profile.pk.clone()),
        owner_username: Some(profile.username.clone()),
        taken_at: None,
        caption: None,
        like_count: None,
        comment_count: None,
    }
}

fn story_catalog_metadata(
    profile: &Profile,
    pk: &str,
    taken_at: Option<i64>,
) -> Option<CatalogItemMetadata> {
    if pk.is_empty() {
        return None;
    }
    Some(CatalogItemMetadata {
        remote_key: format!("story:{pk}"),
        kind: MediaItemKind::Story,
        remote_pk: Some(pk.to_owned()),
        shortcode: None,
        owner_pk: Some(profile.pk.clone()),
        owner_username: Some(profile.username.clone()),
        taken_at,
        caption: None,
        like_count: None,
        comment_count: None,
    })
}

fn root_label(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Downloads")
}

fn persist_downloaded_media(
    catalog: &Catalog,
    destination_root: &Path,
    media: &DownloadedMedia,
) -> Result<(), String> {
    let root = catalog
        .register_root(destination_root, root_label(destination_root))
        .map_err(|error| error.to_string())?;
    let observed_at = unix_now();
    let files = media
        .files
        .iter()
        .map(|file| {
            let metadata = std::fs::metadata(&file.path).map_err(|error| error.to_string())?;
            if metadata.len() != file.bytes {
                return Err(format!(
                    "downloaded file changed before cataloging: {}",
                    file.path.display()
                ));
            }
            let canonical_file = file
                .path
                .canonicalize()
                .map_err(|error| error.to_string())?;
            let relative_path = canonical_file
                .strip_prefix(&root.path)
                .map_err(|_| {
                    "downloaded file is outside the configured destination root".to_string()
                })?
                .to_path_buf();
            Ok(CatalogFileInput {
                root_id: root.id,
                relative_path,
                ordinal: i64::from(file.ordinal),
                kind: file.kind,
                byte_size: i64::try_from(file.bytes)
                    .map_err(|_| "downloaded file size exceeds catalog range".to_string())?,
                mtime: metadata
                    .modified()
                    .map(system_time_seconds)
                    .map_err(|error| error.to_string())?,
                last_seen_at: observed_at,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    catalog
        .upsert_media(&media.item.clone().into_catalog_input(files, observed_at))
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn catalog_downloaded_media(
    catalog: &Catalog,
    destination_root: &Path,
    media: &DownloadedMedia,
) -> Result<(), String> {
    let catalog = catalog.clone();
    let destination_root = destination_root.to_path_buf();
    let media = media.clone();
    tauri::async_runtime::spawn_blocking(move || {
        persist_downloaded_media(&catalog, &destination_root, &media)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Run an optional section fetch: quota/auth failures abort the whole
/// enqueue, transient ones degrade to an empty list.
async fn fetch_soft<F, Fut>(f: F) -> Result<Vec<serde_json::Value>, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<serde_json::Value>, crate::hiker::HikerError>>,
{
    match f().await {
        Ok(items) => Ok(items),
        Err(e) if is_fatal_api_error(&e) => Err(e.to_string()),
        Err(_) => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn resolve_input(input: String) -> Result<Target, String> {
    Target::parse(&input).ok_or_else(|| format!("Unrecognized input: {input}"))
}

async fn client(state: &State<'_, AppState>) -> Result<Arc<crate::hiker::HikerClient>, String> {
    state
        .client
        .read()
        .await
        .clone()
        .ok_or_else(|| "No HikerAPI token configured".into())
}

/// Account autocomplete — same order the API returns (Instagram's own
/// ranking); no client-side reranking by design.
#[tauri::command]
pub async fn search_users(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<SearchUser>, String> {
    let q = query.trim();
    if q.len() < 2 {
        return Ok(Vec::new());
    }
    let client = client(&state).await?;
    let users = client.search_accounts(q).await.map_err(|e| e.to_string())?;
    Ok(users.iter().filter_map(map_search_user).collect())
}

#[derive(Serialize)]
pub struct ProfilePreview {
    pub profile: Profile,
    pub recent_posts: Vec<Post>,
    pub end_cursor: Option<String>,
}

#[tauri::command]
pub async fn fetch_profile(
    username: String,
    end_cursor: Option<String>,
    state: State<'_, AppState>,
) -> Result<ProfilePreview, String> {
    let client = client(&state).await?;
    let user = client
        .user_by_username(&username)
        .await
        .map_err(|e| e.to_string())?;
    let profile = map_profile(&user).ok_or("Could not parse profile payload")?;
    if profile.is_private || (profile.media_count == 0 && end_cursor.is_none()) {
        return Ok(ProfilePreview {
            profile,
            recent_posts: Vec::new(),
            end_cursor: None,
        });
    }
    let page = client
        .user_medias_chunk(&profile.pk, end_cursor.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(ProfilePreview {
        profile,
        recent_posts: page.posts,
        end_cursor: page.end_cursor,
    })
}

/// One cursor-paged batch from the profile's dedicated Reels feed.
#[tauri::command]
pub async fn fetch_reels(
    user_id: String,
    end_cursor: Option<String>,
    state: State<'_, AppState>,
) -> Result<crate::models::PostPage, String> {
    client(&state)
        .await?
        .user_clips_chunk(&user_id, end_cursor.as_deref())
        .await
        .map_err(|error| error.to_string())
}

/// Active stories of a profile for the Explorer grid (billed 2 requests).
#[tauri::command]
pub async fn fetch_stories(
    username: String,
    state: State<'_, AppState>,
) -> Result<Vec<StoryItem>, String> {
    let client = client(&state).await?;
    let user = client
        .user_by_username(&username)
        .await
        .map_err(|e| e.to_string())?;
    let profile = map_profile(&user).ok_or("Could not parse profile payload")?;
    let items = client
        .user_stories(&profile.pk)
        .await
        .map_err(|e| e.to_string())?;
    Ok(items
        .iter()
        .filter_map(|item| {
            let pk = match item.get("pk") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Number(n)) => n.to_string(),
                _ => return None,
            };
            let is_video = item
                .get("video_versions")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            let media_url = if is_video {
                crate::hiker::best_video(item)
            } else {
                crate::hiker::best_image(item)
            }?;
            let thumb_url = crate::hiker::best_image(item).or_else(|| Some(media_url.clone()));
            Some(StoryItem {
                pk,
                taken_at: parse_ts(item),
                kind: if is_video {
                    "video".into()
                } else {
                    "photo".into()
                },
                media_url,
                thumb_url,
            })
        })
        .collect())
}

/// Download already-fetched resources (e.g. a single story) without
/// re-fetching them from HikerAPI. Emits `job-progress`.
#[tauri::command]
pub async fn download_direct(
    label: String,
    subfolder: String,
    items: Vec<DirectItem>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    if items.is_empty() {
        return Err("Nothing to download".into());
    }
    let cfg = state.cfg.read().await.clone();
    let cdn_http = state.cdn_http.clone();
    let catalog = state.catalog.clone();
    let jobs: Arc<JobRegistry> = state.jobs.clone();
    let in_flight = state.in_flight.clone();
    let key = direct_job_key(&label, &subfolder, &items);
    {
        let mut guard = in_flight.lock().unwrap();
        if !guard.insert(key.clone()) {
            return Err(format!(
                "A download for @{label}/{subfolder} is already running"
            ));
        }
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let cancel_rx = jobs.register(&job_id);
    let em = JobEvents::new(&app, job_id.clone(), label.clone());
    let job_id_task = job_id.clone();

    tauri::async_runtime::spawn(async move {
        let job_id = job_id_task;
        let destination_root = PathBuf::from(&cfg.dest_dir);
        let dir = destination_root
            .join(safe_segment(&label))
            .join(safe_segment(&subfolder));
        let result = run_direct_job(
            &cdn_http,
            &catalog,
            &destination_root,
            &em,
            &dir,
            &label,
            &subfolder,
            &items,
            Some(cancel_rx),
            false,
        )
        .await;
        match result {
            Ok(completed) => em.done(completed.outcome, completed.resource_errors.len(), &dir),
            Err(JobFail::Cancelled) => em.cancelled(),
            Err(JobFail::Fatal(e)) => em.failed(e),
        }
        jobs.finish(&job_id);
        in_flight.lock().unwrap().remove(&key);
    });

    Ok(job_id)
}

#[allow(clippy::too_many_arguments)]
async fn run_direct_job(
    cdn_http: &reqwest::Client,
    catalog: &Catalog,
    destination_root: &Path,
    em: &dyn ProgressSink,
    dir: &Path,
    label: &str,
    subfolder: &str,
    items: &[DirectItem],
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
    allow_loopback: bool,
) -> Result<CompletedJob, JobFail> {
    std::fs::create_dir_all(dir)?;
    let skip = existing_stems(dir);
    let mut files_done = 0usize;
    let mut catalog_warnings = 0usize;
    let mut last_error = None;
    let mut resource_errors = Vec::new();
    let mut has_successful_resource = false;
    let mut bytes_total = 0u64;

    for item in items {
        if cancel.as_ref().map(|c| *c.borrow()).unwrap_or(false) {
            return Err(JobFail::Cancelled);
        }
        let base = if matches!(subfolder.to_ascii_lowercase().as_str(), "avatar" | "propic") {
            format!("avatar_{}", safe_segment(&item.pk))
        } else {
            taken_at_name(item.taken_at, &item.pk)
        };
        if stem_exists(&skip, &base) {
            has_successful_resource = true;
            continue;
        }
        match download_one(
            cdn_http,
            &item.url,
            &dir.join(&base),
            item.taken_at,
            em,
            files_done + 1,
            0,
            &mut bytes_total,
            cancel.as_ref().cloned(),
            allow_loopback,
        )
        .await
        {
            Ok(file) => {
                files_done += 1;
                has_successful_resource = true;
                if let Some(metadata) = direct_catalog_metadata(label, subfolder, item) {
                    let media = DownloadedMedia {
                        item: metadata,
                        files: vec![file],
                        resource_errors: Vec::new(),
                    };
                    if catalog_downloaded_media(catalog, destination_root, &media)
                        .await
                        .is_err()
                    {
                        catalog_warnings += 1;
                    }
                }
            }
            Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
            Err(JobFail::Fatal(error)) => {
                last_error = Some(error.clone());
                resource_errors.push(error);
            }
        }
    }
    finish_completed_job(
        JobOutcome {
            files_written: files_done,
            catalog_warnings,
        },
        last_error,
        resource_errors,
        has_successful_resource,
    )
}

/// Download everything selected from a profile. Emits `job-progress`.
#[tauri::command]
pub async fn enqueue_profile_download(
    username: String,
    opts: ProfileOptions,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = client(&state).await?;
    let cfg = state.cfg.read().await.clone();
    let cdn_http = state.cdn_http.clone();
    let catalog = state.catalog.clone();
    let jobs: Arc<JobRegistry> = state.jobs.clone();

    // Backend dedup: never run two profile jobs for the same username.
    let in_flight = state.in_flight.clone();
    let key = format!("profile:{}", username.to_lowercase());
    {
        let mut guard = state.in_flight.lock().unwrap();
        if !guard.insert(key.clone()) {
            return Err(format!("A download for @{username} is already running"));
        }
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let username = username.to_lowercase();

    // Resolve profile first so stories/highlights can reuse its pk.
    let user = match client.user_by_username(&username).await {
        Ok(u) => u,
        Err(e) => {
            in_flight.lock().unwrap().remove(&key);
            return Err(e.to_string());
        }
    };
    let profile = match map_profile(&user) {
        Some(p) => p,
        None => {
            in_flight.lock().unwrap().remove(&key);
            return Err("Could not parse profile payload".into());
        }
    };

    // Stories (2 req) / highlights (2 req + 1 per reel) fetched up front so
    // a cancelled job never wastes their quota mid-run. Fatal API errors
    // abort the enqueue; anything transient degrades to an empty section.
    let stories_items = if opts.stories {
        fetch_soft(|| client.user_stories(&profile.pk))
            .await
            .inspect_err(|_| {
                in_flight.lock().unwrap().remove(&key);
            })?
    } else {
        Vec::new()
    };
    let highlights_tray = if opts.highlights {
        fetch_soft(|| client.user_highlights(&profile.pk))
            .await
            .inspect_err(|_| {
                in_flight.lock().unwrap().remove(&key);
            })?
    } else {
        Vec::new()
    };

    let cancel_rx = jobs.register(&job_id);
    let em = JobEvents::new(&app, job_id.clone(), format!("@{}", profile.username));
    let destination_root = PathBuf::from(&cfg.dest_dir);
    let dir = destination_root.join(safe_segment(&profile.username));
    let job_id_task = job_id.clone();

    tauri::async_runtime::spawn(async move {
        let job_id = job_id_task;
        let result = run_profile_job(
            &client,
            &cdn_http,
            &catalog,
            &destination_root,
            &cfg,
            &em,
            &dir,
            &profile,
            &opts,
            stories_items,
            highlights_tray,
            false,
            Some(cancel_rx),
        )
        .await;
        match result {
            Ok(completed) => em.done(completed.outcome, completed.resource_errors.len(), &dir),
            Err(JobFail::Cancelled) => em.cancelled(),
            Err(JobFail::Fatal(e)) => em.failed(e),
        }
        jobs.finish(&job_id);
        in_flight.lock().unwrap().remove(&key);
    });

    Ok(job_id)
}

#[allow(clippy::too_many_arguments)]
async fn run_profile_job(
    client: &Arc<crate::hiker::HikerClient>,
    cdn_http: &reqwest::Client,
    catalog: &Catalog,
    destination_root: &Path,
    cfg: &Config,
    em: &dyn ProgressSink,
    dir: &Path,
    profile: &Profile,
    opts: &ProfileOptions,
    stories_items: Vec<serde_json::Value>,
    highlights_tray: Vec<serde_json::Value>,
    allow_loopback: bool,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<CompletedJob, JobFail> {
    std::fs::create_dir_all(dir)?;
    let skip = existing_stems(dir);
    let mut files_done = 0usize;
    let mut catalog_warnings = 0usize;
    let mut last_error = None;
    let mut all_resource_errors = Vec::new();
    let mut has_successful_resource = false;
    let mut bytes_total = 0u64;
    let is_cancelled = || cancel.as_ref().map(|c| *c.borrow()).unwrap_or(false);

    // Per-file failures are logged-and-skipped so one dead CDN link or a
    // flaky network never kills the whole archive run. But if EVERYTHING
    // failed, the job must not masquerade as a successful "Done: 0".
    macro_rules! try_file {
        ($fut:expr) => {
            match $fut.await {
                Ok(path) => path,
                Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
                Err(JobFail::Fatal(error)) => {
                    last_error = Some(error.clone());
                    all_resource_errors.push(error);
                    continue;
                }
            }
        };
    }

    // ---- avatar ----
    if opts.avatar {
        if let Some(url) = &profile.avatar_url {
            let base = dir.join(format!("avatar_{}", safe_segment(&profile.pk)));
            if !stem_exists(&skip, base.file_name().unwrap().to_string_lossy().as_ref()) {
                match download_one(
                    cdn_http,
                    url,
                    &base,
                    None,
                    em,
                    files_done + 1,
                    0,
                    &mut bytes_total,
                    cancel.as_ref().cloned(),
                    allow_loopback,
                )
                .await
                {
                    Ok(file) => {
                        files_done += 1;
                        has_successful_resource = true;
                        let media = DownloadedMedia {
                            item: avatar_catalog_metadata(profile),
                            files: vec![file],
                            resource_errors: Vec::new(),
                        };
                        if catalog_downloaded_media(catalog, destination_root, &media)
                            .await
                            .is_err()
                        {
                            catalog_warnings += 1;
                        }
                    }
                    Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
                    Err(JobFail::Fatal(error)) => {
                        last_error = Some(error.clone());
                        all_resource_errors.push(error);
                    }
                }
            } else {
                has_successful_resource = true;
            }
        }
    }
    if is_cancelled() {
        return Err(JobFail::Cancelled);
    }

    // ---- feed posts / reels ----
    if opts.posts || opts.reels {
        let reels_only = opts.reels && !opts.posts;
        let posts_dir = dir.join("posts");
        if let Err(error) = std::fs::create_dir_all(&posts_dir) {
            return finish_completed_job(
                job_outcome(files_done, catalog_warnings),
                Some(error.to_string()),
                all_resource_errors,
                has_successful_resource,
            );
        }
        let mut cursor: Option<String> = None;
        let mut considered: u64 = 0;
        let mut seen_post_pks = HashSet::new();
        let mut seen_cursors = HashSet::new();
        loop {
            if is_cancelled() {
                return Err(JobFail::Cancelled);
            }
            let page_result = if reels_only {
                client
                    .user_clips_chunk(&profile.pk, cursor.as_deref())
                    .await
            } else {
                client
                    .user_medias_chunk(&profile.pk, cursor.as_deref())
                    .await
            };
            let page = match page_result {
                Ok(page) => page,
                Err(error) => {
                    return finish_completed_job(
                        job_outcome(files_done, catalog_warnings),
                        Some(error.to_string()),
                        all_resource_errors,
                        has_successful_resource,
                    )
                }
            };
            for post in &page.posts {
                if seen_post_pks.contains(&post.pk) {
                    continue;
                }
                if let Some(max) = opts.max_posts {
                    if considered >= max {
                        break;
                    }
                }
                seen_post_pks.insert(post.pk.clone());
                considered += 1;
                let base = taken_at_name(post.taken_at, &post.code);
                let item_metadata = post_catalog_metadata(post);
                let total = post.resources.len();
                let mut downloaded = Vec::new();
                let mut media_files_written = 0usize;
                let mut resource_errors = Vec::new();
                for (idx, resource) in post.resources.iter().enumerate() {
                    let dest_base = if total > 1 {
                        posts_dir.join(format!("{base}_{}", idx + 1))
                    } else {
                        posts_dir.join(&base)
                    };
                    let ordinal = u32::try_from(idx).unwrap_or(u32::MAX);
                    if let Some(file) = recover_downloaded_file(
                        catalog,
                        destination_root,
                        &item_metadata.remote_key,
                        ordinal,
                    )
                    .await
                    {
                        has_successful_resource = true;
                        downloaded.push(file);
                        continue;
                    }
                    match download_one(
                        cdn_http,
                        &resource.url,
                        &dest_base,
                        post.taken_at,
                        em,
                        files_done + idx + 1,
                        ordinal,
                        &mut bytes_total,
                        cancel.as_ref().cloned(),
                        allow_loopback,
                    )
                    .await
                    {
                        Ok(file) => {
                            media_files_written += 1;
                            has_successful_resource = true;
                            downloaded.push(file);
                        }
                        Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
                        Err(JobFail::Fatal(error)) => {
                            last_error = Some(error.clone());
                            all_resource_errors.push(error.clone());
                            resource_errors.push(error);
                        }
                    }
                }
                let sidecar_ordinal = u32::try_from(total).unwrap_or(u32::MAX);
                let mut sidecar_written = false;
                if cfg.sidecar && !downloaded.is_empty() {
                    if let Some(sidecar) =
                        recover_sidecar(&downloaded, sidecar_ordinal, &item_metadata)
                    {
                        downloaded.push(sidecar);
                    } else if let Some(first_file) = downloaded.first() {
                        match write_sidecar(cfg, &posts_dir, post, &first_file.path, &item_metadata)
                        {
                            Ok(Some(path)) => match downloaded_sidecar(path, sidecar_ordinal) {
                                Ok(sidecar) => downloaded.push(sidecar),
                                Err(error) => {
                                    last_error = Some(error.clone());
                                    all_resource_errors.push(error.clone());
                                    resource_errors.push(error);
                                }
                            },
                            Ok(None) => {}
                            Err(error) => {
                                last_error = Some(error.clone());
                                all_resource_errors.push(error.clone());
                                resource_errors.push(error);
                            }
                        }
                        sidecar_written = true;
                    }
                }
                files_done += media_files_written;
                if !downloaded.is_empty()
                    && (media_files_written > 0 || sidecar_written || !resource_errors.is_empty())
                {
                    let media = DownloadedMedia {
                        item: item_metadata,
                        files: downloaded,
                        resource_errors,
                    };
                    if (media_files_written > 0 || sidecar_written)
                        && catalog_downloaded_media(catalog, destination_root, &media)
                            .await
                            .is_err()
                    {
                        catalog_warnings += 1;
                    }
                }
            }
            let Some(next_cursor) = page.end_cursor.filter(|value| !value.trim().is_empty()) else {
                break;
            };
            if !seen_cursors.insert(next_cursor.clone()) {
                break;
            }
            cursor = Some(next_cursor);
            if let Some(max) = opts.max_posts {
                if considered >= max {
                    break;
                }
            }
        }
    }

    // ---- stories ----
    if opts.stories && !stories_items.is_empty() {
        let stories_dir = dir.join("stories");
        if let Err(error) = std::fs::create_dir_all(&stories_dir) {
            return finish_completed_job(
                job_outcome(files_done, catalog_warnings),
                Some(error.to_string()),
                all_resource_errors,
                has_successful_resource,
            );
        }
        for item in &stories_items {
            if is_cancelled() {
                return Err(JobFail::Cancelled);
            }
            let pk = value_pk(item);
            let taken_at = parse_ts(item);
            let base = taken_at_name(taken_at, &pk);
            if stem_exists(&skip, &base) {
                has_successful_resource = true;
                continue;
            }
            let mut downloaded = Vec::new();
            let mut resource_errors = Vec::new();
            for (ordinal, resource) in crate::hiker::collect_resources(item, infer_video(item))
                .iter()
                .enumerate()
            {
                match download_one(
                    cdn_http,
                    &resource.url,
                    &stories_dir.join(&base),
                    taken_at,
                    em,
                    files_done + 1,
                    u32::try_from(ordinal).unwrap_or(u32::MAX),
                    &mut bytes_total,
                    cancel.as_ref().cloned(),
                    allow_loopback,
                )
                .await
                {
                    Ok(file) => {
                        files_done += 1;
                        has_successful_resource = true;
                        downloaded.push(file);
                    }
                    Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
                    Err(JobFail::Fatal(error)) => {
                        last_error = Some(error.clone());
                        all_resource_errors.push(error.clone());
                        resource_errors.push(error);
                    }
                }
            }
            if !downloaded.is_empty() {
                if let Some(metadata) = story_catalog_metadata(profile, &pk, taken_at) {
                    let media = DownloadedMedia {
                        item: metadata,
                        files: downloaded,
                        resource_errors,
                    };
                    if catalog_downloaded_media(catalog, destination_root, &media)
                        .await
                        .is_err()
                    {
                        catalog_warnings += 1;
                    }
                }
            }
        }
    }

    // ---- highlights ----
    if opts.highlights && !highlights_tray.is_empty() {
        let hl_root = dir.join("highlights");
        if let Err(error) = std::fs::create_dir_all(&hl_root) {
            return finish_completed_job(
                job_outcome(files_done, catalog_warnings),
                Some(error.to_string()),
                all_resource_errors,
                has_successful_resource,
            );
        }
        for tray in &highlights_tray {
            if is_cancelled() {
                return Err(JobFail::Cancelled);
            }
            let Some(hl_pk) = tray.get("pk").and_then(|v| v.as_str()) else {
                continue;
            };
            let title = tray
                .get("title")
                .and_then(|v| v.as_str())
                .map(safe_segment)
                .filter(|t| !t.trim_matches('_').is_empty())
                .unwrap_or_else(|| hl_pk.to_string());
            let hl_dir = hl_root.join(format!("{}_{}", safe_segment(hl_pk), title));
            if let Err(error) = std::fs::create_dir_all(&hl_dir) {
                return finish_completed_job(
                    job_outcome(files_done, catalog_warnings),
                    Some(error.to_string()),
                    all_resource_errors,
                    has_successful_resource,
                );
            }
            let items = match client.highlight_items(hl_pk).await {
                Ok(items) => items,
                Err(error) => {
                    return finish_completed_job(
                        job_outcome(files_done, catalog_warnings),
                        Some(error.to_string()),
                        all_resource_errors,
                        has_successful_resource,
                    )
                }
            };
            for item in &items {
                if is_cancelled() {
                    return Err(JobFail::Cancelled);
                }
                let pk = value_pk(item);
                let taken_at = parse_ts(item);
                let base = taken_at_name(taken_at, &pk);
                if stem_exists(&skip, &base) {
                    has_successful_resource = true;
                    continue;
                }
                for (ordinal, resource) in crate::hiker::collect_resources(item, infer_video(item))
                    .iter()
                    .enumerate()
                {
                    try_file!(download_one(
                        cdn_http,
                        &resource.url,
                        &hl_dir.join(&base),
                        taken_at,
                        em,
                        files_done + 1,
                        u32::try_from(ordinal).unwrap_or(u32::MAX),
                        &mut bytes_total,
                        cancel.as_ref().cloned(),
                        allow_loopback,
                    ));
                    files_done += 1;
                    has_successful_resource = true;
                }
            }
        }
    }

    finish_completed_job(
        job_outcome(files_done, catalog_warnings),
        last_error,
        all_resource_errors,
        has_successful_resource,
    )
}

fn value_pk(item: &serde_json::Value) -> String {
    match item.get("pk") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn parse_ts(item: &serde_json::Value) -> Option<i64> {
    crate::models::parse_taken_at(item.get("taken_at").unwrap_or(&serde_json::Value::Null))
}

fn infer_video(item: &serde_json::Value) -> bool {
    match item.get("media_type").and_then(|v| v.as_u64()) {
        Some(t) => t == 2,
        None => item
            .get("video_versions")
            .and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false),
    }
}

/// Single post/reel by shortcode. Emits `job-progress` events.
#[tauri::command]
pub async fn download_post(
    code: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let client = client(&state).await?;
    let cfg = state.cfg.read().await.clone();
    let cdn_http = state.cdn_http.clone();
    let catalog = state.catalog.clone();
    let jobs: Arc<JobRegistry> = state.jobs.clone();

    // Backend dedup: same shortcode cannot run twice at once.
    // Shortcodes are case-sensitive — keep the exact form.
    let in_flight = state.in_flight.clone();
    let key = post_job_key(&code);
    {
        let mut guard = state.in_flight.lock().unwrap();
        if !guard.insert(key.clone()) {
            return Err(format!("Post {code} is already downloading"));
        }
    }

    let media = match client.media_by_code(&code).await {
        Ok(m) => m,
        Err(e) => {
            in_flight.lock().unwrap().remove(&key);
            return Err(e.to_string());
        }
    };
    let post = match map_post(&media) {
        Some(p) => p,
        None => {
            state.in_flight.lock().unwrap().remove(&key);
            return Err("Could not parse media payload".into());
        }
    };
    let job_id = uuid::Uuid::new_v4().to_string();

    let cancel_rx = jobs.register(&job_id);
    let em = JobEvents::new(
        &app,
        job_id.clone(),
        format!(
            "@{}",
            post.owner_username
                .clone()
                .unwrap_or_else(|| post.code.clone())
        ),
    );
    let job_id_task = job_id.clone();

    tauri::async_runtime::spawn(async move {
        let job_id = job_id_task;
        let destination_root = PathBuf::from(&cfg.dest_dir);
        let dir = destination_root.join(safe_segment(
            post.owner_username
                .as_deref()
                .or(post.owner_pk.as_deref())
                .unwrap_or("unknown"),
        ));
        let result = run_single_post(
            &cdn_http,
            &catalog,
            &destination_root,
            &cfg,
            &em,
            &dir,
            &post,
            Some(cancel_rx),
            false,
        )
        .await;
        match result {
            Ok(completed) => {
                let resource_failures = completed
                    .media
                    .as_ref()
                    .map_or(0, |media| media.resource_errors.len());
                em.done(completed.outcome, resource_failures, &dir);
            }
            Err(JobFail::Cancelled) => em.cancelled(),
            Err(JobFail::Fatal(e)) => em.failed(e),
        }
        jobs.finish(&job_id);
        in_flight.lock().unwrap().remove(&key);
    });

    Ok(job_id)
}

struct CompletedPostDownload {
    outcome: JobOutcome,
    media: Option<DownloadedMedia>,
}

#[allow(clippy::too_many_arguments)]
async fn run_single_post(
    cdn_http: &reqwest::Client,
    catalog: &Catalog,
    destination_root: &Path,
    cfg: &Config,
    em: &dyn ProgressSink,
    dir: &Path,
    post: &Post,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
    allow_loopback: bool,
) -> Result<CompletedPostDownload, JobFail> {
    std::fs::create_dir_all(dir)?;
    let base = taken_at_name(post.taken_at, &post.code);
    let item_metadata = post_catalog_metadata(post);
    let total = post.resources.len();
    let mut bytes_total = 0u64;
    let mut downloaded = Vec::new();
    let mut media_files_written = 0usize;
    let mut resource_errors = Vec::new();
    let mut last_error = None;

    for (idx, resource) in post.resources.iter().enumerate() {
        if cancel.as_ref().map(|c| *c.borrow()).unwrap_or(false) {
            return Err(JobFail::Cancelled);
        }
        let dest_base = if total > 1 {
            dir.join(format!("{base}_{}", idx + 1))
        } else {
            dir.join(&base)
        };
        let ordinal = u32::try_from(idx).unwrap_or(u32::MAX);
        if let Some(file) = recover_downloaded_file(
            catalog,
            destination_root,
            &item_metadata.remote_key,
            ordinal,
        )
        .await
        {
            downloaded.push(file);
            continue;
        }
        match download_one(
            cdn_http,
            &resource.url,
            &dest_base,
            post.taken_at,
            em,
            idx + 1,
            ordinal,
            &mut bytes_total,
            cancel.as_ref().cloned(),
            allow_loopback,
        )
        .await
        {
            Ok(file) => {
                media_files_written += 1;
                downloaded.push(file);
            }
            Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
            Err(JobFail::Fatal(error)) => {
                last_error = Some(error.clone());
                resource_errors.push(error);
            }
        }
    }
    let sidecar_ordinal = u32::try_from(total).unwrap_or(u32::MAX);
    let mut sidecar_written = false;
    if cfg.sidecar && !downloaded.is_empty() {
        if let Some(sidecar) = recover_sidecar(&downloaded, sidecar_ordinal, &item_metadata) {
            downloaded.push(sidecar);
        } else if let Some(first_file) = downloaded.first() {
            match write_sidecar(cfg, dir, post, &first_file.path, &item_metadata) {
                Ok(Some(path)) => match downloaded_sidecar(path, sidecar_ordinal) {
                    Ok(sidecar) => downloaded.push(sidecar),
                    Err(error) => {
                        last_error = Some(error.clone());
                        resource_errors.push(error);
                    }
                },
                Ok(None) => {}
                Err(error) => {
                    last_error = Some(error.clone());
                    resource_errors.push(error);
                }
            }
            sidecar_written = true;
        }
    }

    if media_files_written == 0 && !sidecar_written && resource_errors.is_empty() {
        return Ok(CompletedPostDownload {
            outcome: JobOutcome::default(),
            media: None,
        });
    }

    let mut outcome = job_outcome(media_files_written, 0);
    let media = if downloaded.is_empty() {
        None
    } else {
        let media = DownloadedMedia {
            item: item_metadata,
            files: downloaded,
            resource_errors,
        };
        if (media_files_written > 0 || sidecar_written)
            && catalog_downloaded_media(catalog, destination_root, &media)
                .await
                .is_err()
        {
            outcome.catalog_warnings += 1;
        }
        Some(media)
    };
    let outcome = if media.is_some() {
        outcome
    } else {
        finish_downloads(outcome, last_error)?
    };
    Ok(CompletedPostDownload { outcome, media })
}

#[tauri::command]
pub async fn cancel_job(job_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.jobs.cancel(&job_id))
}

#[cfg(test)]
mod download_catalog_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FetchedPostCategory, FetchedPostScope, MediaKind, MediaResource};

    fn direct_item(pk: &str) -> DirectItem {
        DirectItem {
            url: format!("https://cdninstagram.com/{pk}.jpg"),
            taken_at: None,
            pk: pk.to_string(),
        }
    }

    fn post_with_resource_kinds(kinds: &[MediaKind]) -> Post {
        Post {
            pk: "remote-pk".into(),
            code: "SHORTCODE".into(),
            taken_at: Some(1_700_000_000),
            caption: None,
            like_count: None,
            comment_count: None,
            owner_username: Some("owner".into()),
            owner_pk: Some("owner-pk".into()),
            resources: kinds
                .iter()
                .enumerate()
                .map(|(index, kind)| MediaResource {
                    url: format!("https://cdninstagram.com/{index}"),
                    kind: *kind,
                })
                .collect(),
            thumbnail_url: None,
        }
    }

    fn fetched_post(pk: &str, code: &str, url: &str) -> Post {
        Post {
            pk: pk.into(),
            code: code.into(),
            taken_at: Some(1_700_000_000),
            caption: Some("A complete fetched post".into()),
            like_count: Some(42),
            comment_count: Some(7),
            owner_username: Some("owner".into()),
            owner_pk: Some("9001".into()),
            resources: vec![MediaResource {
                url: url.into(),
                kind: MediaKind::Video,
            }],
            thumbnail_url: Some("https://cdninstagram.com/thumb.jpg".into()),
        }
    }

    #[test]
    fn fetched_batch_validation_deserializes_the_closed_contract() {
        let post: Post = serde_json::from_value(serde_json::json!({
            "pk": "100",
            "code": "SHORTCODE",
            "taken_at": 1_700_000_000,
            "caption": "caption",
            "like_count": 42,
            "comment_count": 7,
            "owner_username": "owner",
            "owner_pk": "9001",
            "resources": [{
                "url": "https://cdninstagram.com/video.mp4",
                "kind": "video"
            }],
            "thumbnail_url": "https://cdninstagram.com/thumb.jpg"
        }))
        .unwrap();
        assert_eq!(post.pk, "100");
        assert_eq!(post.resources[0].kind, MediaKind::Video);

        assert_eq!(
            serde_json::from_str::<FetchedPostCategory>(r#""posts""#).unwrap(),
            FetchedPostCategory::Posts
        );
        assert_eq!(
            serde_json::from_str::<FetchedPostCategory>(r#""reels""#).unwrap(),
            FetchedPostCategory::Reels
        );
        assert_eq!(
            serde_json::from_str::<FetchedPostScope>(r#""shown""#).unwrap(),
            FetchedPostScope::Shown
        );
        assert_eq!(
            serde_json::from_str::<FetchedPostScope>(r#""selected""#).unwrap(),
            FetchedPostScope::Selected
        );
        assert!(serde_json::from_str::<FetchedPostCategory>(r#""stories""#).is_err());
        assert!(serde_json::from_str::<FetchedPostScope>(r#""all""#).is_err());
    }

    #[test]
    fn fetched_batch_validation_deduplicates_by_pk_in_first_seen_order() {
        let first = fetched_post("100", "FIRST", "https://cdninstagram.com/first.mp4");
        let duplicate = fetched_post("100", "DUPLICATE", "https://cdninstagram.com/duplicate.mp4");
        let second = fetched_post("200", "SECOND", "https://fbcdn.net/second.mp4");

        let validated = validate_fetched_posts(vec![first, duplicate, second], false).unwrap();

        assert_eq!(
            validated
                .iter()
                .map(|post| (post.pk.as_str(), post.code.as_str()))
                .collect::<Vec<_>>(),
            vec![("100", "FIRST"), ("200", "SECOND")]
        );
    }

    #[test]
    fn fetched_batch_validation_rejects_empty_and_oversized_batches() {
        assert_eq!(MAX_FETCHED_POSTS, 500);
        assert_eq!(
            validate_fetched_posts(Vec::new(), false).unwrap_err(),
            "Fetched post batch must not be empty"
        );

        let maximum = vec![
            fetched_post("100", "SHORTCODE", "https://cdninstagram.com/video.mp4");
            MAX_FETCHED_POSTS
        ];
        assert_eq!(
            validate_fetched_posts(maximum.clone(), false)
                .unwrap()
                .len(),
            1
        );

        let mut oversized = maximum;
        oversized.push(fetched_post(
            "200",
            "SECOND",
            "https://cdninstagram.com/second.mp4",
        ));
        assert_eq!(
            validate_fetched_posts(oversized, false).unwrap_err(),
            "Fetched post batch exceeds maximum of 500 posts"
        );
    }

    #[test]
    fn fetched_batch_validation_rejects_invalid_post_identifiers() {
        for pk in ["", "../escape", "123a", "١٢٣"] {
            let post = fetched_post(pk, "SHORTCODE", "https://cdninstagram.com/video.mp4");
            assert_eq!(
                validate_fetched_posts(vec![post], false).unwrap_err(),
                "Fetched post PK must contain only ASCII digits"
            );
        }
    }

    #[test]
    fn fetched_batch_validation_enforces_shortcode_byte_bounds() {
        assert_eq!(MAX_SHORTCODE_BYTES, 256);

        let empty = fetched_post("100", "", "https://cdninstagram.com/video.mp4");
        assert_eq!(
            validate_fetched_posts(vec![empty], false).unwrap_err(),
            "Fetched post shortcode must not be empty"
        );

        let maximum = fetched_post(
            "100",
            &"a".repeat(MAX_SHORTCODE_BYTES),
            "https://cdninstagram.com/video.mp4",
        );
        assert!(validate_fetched_posts(vec![maximum], false).is_ok());

        let oversized = fetched_post(
            "100",
            &"a".repeat(MAX_SHORTCODE_BYTES + 1),
            "https://cdninstagram.com/video.mp4",
        );
        assert_eq!(
            validate_fetched_posts(vec![oversized], false).unwrap_err(),
            "Fetched post shortcode exceeds maximum of 256 bytes"
        );
    }

    #[test]
    fn fetched_batch_validation_enforces_resource_count_bounds() {
        assert_eq!(MAX_RESOURCES_PER_POST, 20);

        let mut empty = fetched_post("100", "SHORTCODE", "https://cdninstagram.com/video.mp4");
        empty.resources.clear();
        assert_eq!(
            validate_fetched_posts(vec![empty], false).unwrap_err(),
            "Fetched post must contain between 1 and 20 resources"
        );

        let mut maximum = fetched_post("100", "SHORTCODE", "https://cdninstagram.com/video.mp4");
        maximum.resources = vec![maximum.resources[0].clone(); MAX_RESOURCES_PER_POST];
        assert!(validate_fetched_posts(vec![maximum.clone()], false).is_ok());

        maximum.resources.push(MediaResource {
            url: "https://cdninstagram.com/extra.mp4".into(),
            kind: MediaKind::Video,
        });
        assert_eq!(
            validate_fetched_posts(vec![maximum], false).unwrap_err(),
            "Fetched post must contain between 1 and 20 resources"
        );
    }

    #[test]
    fn fetched_batch_validation_rejects_unsafe_urls_without_leaking_details() {
        let unsafe_post = fetched_post("100", "SHORTCODE", "http://127.0.0.1/private");
        assert_eq!(
            validate_fetched_posts(vec![unsafe_post], false).unwrap_err(),
            "Fetched post contains an invalid media URL"
        );
    }

    #[test]
    fn fetched_batch_validation_validates_duplicates_before_deduplication() {
        let valid = fetched_post("100", "FIRST", "https://cdninstagram.com/first.mp4");
        let unsafe_duplicate = fetched_post("100", "DUPLICATE", "http://127.0.0.1/private");

        assert_eq!(
            validate_fetched_posts(vec![valid, unsafe_duplicate], false).unwrap_err(),
            "Fetched post contains an invalid media URL"
        );
    }

    #[test]
    fn full_failure_preserves_the_concrete_error() {
        assert!(matches!(
            finish_downloads(job_outcome(0, 0), Some("HTTP 403".into())),
            Err(JobFail::Fatal(error)) if error.contains("HTTP 403")
        ));
    }

    #[test]
    fn partial_success_reports_only_written_files() {
        assert!(matches!(
            finish_downloads(job_outcome(2, 0), Some("HTTP 403".into())),
            Ok(JobOutcome {
                files_written: 2,
                catalog_warnings: 0
            })
        ));
    }

    #[test]
    fn completed_job_retains_each_concrete_resource_error() {
        let errors: Vec<String> = vec![
            "HTTP 403 on ordinal 0".into(),
            "disk full on ordinal 2".into(),
        ];
        let completed = finish_completed_job(
            job_outcome(1, 0),
            Some(errors[1].clone()),
            errors.clone(),
            true,
        )
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("job was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("job failed: {error}"),
        });

        assert_eq!(completed.outcome.files_written, 1);
        assert_eq!(completed.resource_errors, errors);
    }

    #[test]
    fn recovered_resource_keeps_a_zero_write_retry_successful() {
        let completed = finish_completed_job(
            job_outcome(0, 0),
            Some("HTTP 403 on missing ordinal".into()),
            vec!["HTTP 403 on missing ordinal".into()],
            true,
        )
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("job was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("job failed: {error}"),
        });

        assert_eq!(completed.outcome.files_written, 0);
        assert_eq!(completed.resource_errors.len(), 1);
    }

    #[test]
    fn recovered_sidecar_requires_matching_top_level_remote_pk() {
        let temp = tempfile::tempdir().unwrap();
        let sidecar = temp.path().join("reel.json");
        std::fs::write(
            &sidecar,
            br#"{
              "pk":"123",
              "catalog":{
                "version":1,
                "remote_key":"post:990011",
                "item_kind":"reel"
              }
            }"#,
        )
        .unwrap();
        let metadata = CatalogItemMetadata {
            remote_key: "post:990011".into(),
            kind: MediaItemKind::Reel,
            remote_pk: Some("990011".into()),
            shortcode: None,
            owner_pk: None,
            owner_username: None,
            taken_at: None,
            caption: None,
            like_count: None,
            comment_count: None,
        };

        assert!(!sidecar_catalog_hint_matches(&sidecar, &metadata));
    }

    #[test]
    fn all_skipped_downloads_are_a_successful_noop() {
        assert!(matches!(
            finish_downloads(job_outcome(0, 0), None),
            Ok(JobOutcome {
                files_written: 0,
                catalog_warnings: 0
            })
        ));
    }

    #[test]
    fn done_progress_omits_zero_warning_counts_and_never_serializes_resource_errors() {
        let legacy = serde_json::to_value(JobProgress {
            job_id: "job".into(),
            label: "label".into(),
            state: JobState::Done {
                count: 2,
                dir: "/configured/root".into(),
                catalog_warnings: 0,
                resource_failures: 0,
            },
        })
        .unwrap();
        assert_eq!(legacy["state"], "done");
        assert_eq!(legacy["count"], 2);
        assert_eq!(legacy["dir"], "/configured/root");
        assert!(legacy.get("catalog_warnings").is_none());
        assert!(legacy.get("resource_failures").is_none());

        let warning = serde_json::to_value(JobProgress {
            job_id: "job".into(),
            label: "label".into(),
            state: JobState::Done {
                count: 2,
                dir: "/configured/root".into(),
                catalog_warnings: 1,
                resource_failures: 2,
            },
        })
        .unwrap();
        assert_eq!(warning["catalog_warnings"], 1);
        assert_eq!(warning["resource_failures"], 2);
        assert!(!warning.to_string().contains("secret-token"));
    }

    #[test]
    fn cdn_cancellation_stays_typed() {
        assert!(matches!(
            JobFail::from(CdnError::Cancelled),
            JobFail::Cancelled
        ));
    }

    #[test]
    fn direct_dedupe_is_case_and_order_independent() {
        let first = vec![direct_item("2"), direct_item("1")];
        let second = vec![direct_item("1"), direct_item("2")];

        assert_eq!(
            direct_job_key("Nike", "Stories", &first),
            direct_job_key("nike", "stories", &second)
        );
    }

    #[test]
    fn direct_dedupe_distinguishes_destinations_and_item_sets() {
        let first = vec![direct_item("1")];
        let second = vec![direct_item("2")];

        assert_ne!(
            direct_job_key("nike", "stories", &first),
            direct_job_key("nike", "stories", &second)
        );
        assert_ne!(
            direct_job_key("nike", "stories", &first),
            direct_job_key("nike", "avatar", &first)
        );
        assert_ne!(
            direct_job_key("nike", "stories", &first),
            direct_job_key("adidas", "stories", &first)
        );
    }

    #[test]
    fn direct_dedupe_has_unambiguous_item_boundaries() {
        let one_item = vec![direct_item("a,b")];
        let two_items = vec![direct_item("a"), direct_item("b")];

        assert_ne!(
            direct_job_key("nike", "stories", &one_item),
            direct_job_key("nike", "stories", &two_items)
        );
    }

    #[test]
    fn direct_catalog_keys_only_use_trustworthy_command_context() {
        let item = direct_item("123");
        let story = direct_catalog_metadata("owner", "stories", &item).unwrap();
        assert_eq!(story.remote_key, "story:123");
        assert_eq!(story.kind, MediaItemKind::Story);

        let avatar = direct_catalog_metadata("owner", "propic", &item).unwrap();
        assert_eq!(avatar.remote_key, "avatar:123");
        assert_eq!(avatar.kind, MediaItemKind::Avatar);

        assert!(direct_catalog_metadata("owner", "highlights", &item).is_none());
        assert!(direct_catalog_metadata("owner", "downloads", &item).is_none());
        assert!(direct_catalog_metadata("owner", "stories", &direct_item(" ")).is_none());
        assert!(direct_catalog_metadata("owner", "propic", &direct_item("../escape")).is_none());
    }

    #[test]
    fn post_catalog_kind_never_reclassifies_a_carousel_as_a_reel() {
        for kinds in [
            vec![MediaKind::Photo, MediaKind::Video],
            vec![MediaKind::Video, MediaKind::Video],
        ] {
            assert_eq!(
                post_catalog_metadata(&post_with_resource_kinds(&kinds)).kind,
                MediaItemKind::Post
            );
        }
    }

    #[test]
    fn post_catalog_kind_uses_single_resource_as_the_narrow_reel_signal() {
        assert_eq!(
            post_catalog_metadata(&post_with_resource_kinds(&[MediaKind::Photo])).kind,
            MediaItemKind::Post
        );
        assert_eq!(
            post_catalog_metadata(&post_with_resource_kinds(&[MediaKind::Video])).kind,
            MediaItemKind::Reel
        );
    }

    #[test]
    fn post_dedupe_preserves_shortcode_case() {
        assert_ne!(post_job_key("AbC123"), post_job_key("abc123"));
    }

    #[test]
    fn path_segments_cannot_escape_the_download_directory() {
        assert_eq!(safe_segment("."), "_");
        assert_eq!(safe_segment(".."), "_");
        assert_eq!(safe_segment(""), "_");
        assert_eq!(safe_segment("valid.name"), "valid.name");
    }
}
