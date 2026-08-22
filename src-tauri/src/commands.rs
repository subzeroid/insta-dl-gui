use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::cdn::{self, CdnError};
use crate::config::Config;
use crate::hiker::{map_post, map_profile, map_search_user};
use crate::jobs::JobRegistry;
use crate::models::{DirectItem, Post, Profile, ProfileOptions, SearchUser, StoryItem};
use crate::targets::Target;
use crate::AppState;

const DOWNLOAD_ATTEMPTS: usize = 3;

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

    fn done(&self, count: usize, dir: &Path) {
        self.app
            .emit(
                "job-progress",
                &JobProgress {
                    job_id: self.job_id.clone(),
                    label: self.label.clone(),
                    state: JobState::Done {
                        count,
                        dir: dir.to_string_lossy().into_owned(),
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

fn write_sidecar(cfg: &Config, dir: &Path, post: &Post, first_file: &Path) -> Result<(), String> {
    if !cfg.sidecar {
        return Ok(());
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
    });
    let path = dir.join(format!("{stem}.json"));
    let json = serde_json::to_vec_pretty(&sidecar).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
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

fn finish_downloads(succeeded: usize, last_error: Option<String>) -> Result<usize, JobFail> {
    match (succeeded, last_error) {
        (0, Some(error)) => Err(JobFail::Fatal(error)),
        _ => Ok(succeeded),
    }
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
    em: &JobEvents,
    file_no: usize,
    bytes_so_far: &mut u64,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<PathBuf, JobFail> {
    let name = dest_base
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let outcome = cdn::stream_to_file_retried(
        cdn_http,
        url,
        dest_base,
        taken_at,
        |bytes| em.progress(file_no, 0, *bytes_so_far + bytes, &name),
        cancel,
        DOWNLOAD_ATTEMPTS,
    )
    .await
    .map_err(JobFail::from)?;
    *bytes_so_far += outcome.bytes;
    Ok(outcome.path)
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
        let dir = Path::new(&cfg.dest_dir)
            .join(safe_segment(&label))
            .join(safe_segment(&subfolder));
        let result = run_direct_job(&cdn_http, &em, &dir, &items, Some(cancel_rx)).await;
        match result {
            Ok(count) => em.done(count, &dir),
            Err(JobFail::Cancelled) => em.cancelled(),
            Err(JobFail::Fatal(e)) => em.failed(e),
        }
        jobs.finish(&job_id);
        in_flight.lock().unwrap().remove(&key);
    });

    Ok(job_id)
}

async fn run_direct_job(
    cdn_http: &reqwest::Client,
    em: &JobEvents,
    dir: &Path,
    items: &[DirectItem],
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<usize, JobFail> {
    std::fs::create_dir_all(dir)?;
    let skip = existing_stems(dir);
    let mut files_done = 0usize;
    let mut last_error = None;
    let mut bytes_total = 0u64;

    for item in items {
        if cancel.as_ref().map(|c| *c.borrow()).unwrap_or(false) {
            return Err(JobFail::Cancelled);
        }
        let base = taken_at_name(item.taken_at, &item.pk);
        if stem_exists(&skip, &base) {
            continue;
        }
        match download_one(
            cdn_http,
            &item.url,
            &dir.join(&base),
            item.taken_at,
            em,
            files_done + 1,
            &mut bytes_total,
            cancel.as_ref().cloned(),
        )
        .await
        {
            Ok(_) => files_done += 1,
            Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
            Err(JobFail::Fatal(error)) => last_error = Some(error),
        }
    }
    finish_downloads(files_done, last_error)
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
    let dir = Path::new(&cfg.dest_dir).join(safe_segment(&profile.username));
    let job_id_task = job_id.clone();

    tauri::async_runtime::spawn(async move {
        let job_id = job_id_task;
        let result = run_profile_job(
            &client,
            &cdn_http,
            &cfg,
            &em,
            &dir,
            &profile,
            &opts,
            stories_items,
            highlights_tray,
            Some(cancel_rx),
        )
        .await;
        match result {
            Ok(count) => em.done(count, &dir),
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
    cfg: &Config,
    em: &JobEvents,
    dir: &Path,
    profile: &Profile,
    opts: &ProfileOptions,
    stories_items: Vec<serde_json::Value>,
    highlights_tray: Vec<serde_json::Value>,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<usize, JobFail> {
    std::fs::create_dir_all(dir)?;
    let skip = existing_stems(dir);
    let mut files_done = 0usize;
    let mut last_error = None;
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
                    last_error = Some(error);
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
                    &mut bytes_total,
                    cancel.as_ref().cloned(),
                )
                .await
                {
                    Ok(_) => files_done += 1,
                    Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
                    Err(JobFail::Fatal(error)) => last_error = Some(error),
                }
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
            return finish_downloads(files_done, Some(error.to_string()));
        }
        let mut cursor: Option<String> = None;
        let mut considered: u64 = 0;
        loop {
            if is_cancelled() {
                return Err(JobFail::Cancelled);
            }
            let page = match client
                .user_medias_chunk(&profile.pk, cursor.as_deref())
                .await
            {
                Ok(page) => page,
                Err(error) => return finish_downloads(files_done, Some(error.to_string())),
            };
            for post in &page.posts {
                if let Some(max) = opts.max_posts {
                    if considered >= max {
                        break;
                    }
                }
                considered += 1;
                if reels_only
                    && !post
                        .resources
                        .iter()
                        .any(|r| r.kind == crate::models::MediaKind::Video)
                {
                    continue;
                }
                let base = taken_at_name(post.taken_at, &post.code);
                if stem_exists(&skip, &base) {
                    continue;
                }
                let total = post.resources.len();
                let mut got = 0usize;
                for (idx, resource) in post.resources.iter().enumerate() {
                    let dest_base = if total > 1 {
                        posts_dir.join(format!("{base}_{}", idx + 1))
                    } else {
                        posts_dir.join(&base)
                    };
                    let out = try_file!(download_one(
                        cdn_http,
                        &resource.url,
                        &dest_base,
                        post.taken_at,
                        em,
                        files_done + idx + 1,
                        &mut bytes_total,
                        cancel.as_ref().cloned(),
                    ));
                    got += 1;
                    if idx == 0 {
                        if let Err(error) = write_sidecar(cfg, &posts_dir, post, &out) {
                            last_error = Some(error);
                        }
                    }
                }
                files_done += got;
            }
            cursor = page.end_cursor;
            if cursor.is_none() {
                break;
            }
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
            return finish_downloads(files_done, Some(error.to_string()));
        }
        for item in &stories_items {
            if is_cancelled() {
                return Err(JobFail::Cancelled);
            }
            let pk = value_pk(item);
            let taken_at = parse_ts(item);
            let base = taken_at_name(taken_at, &pk);
            if stem_exists(&skip, &base) {
                continue;
            }
            for resource in &crate::hiker::collect_resources(item, infer_video(item)) {
                try_file!(download_one(
                    cdn_http,
                    &resource.url,
                    &stories_dir.join(&base),
                    taken_at,
                    em,
                    files_done + 1,
                    &mut bytes_total,
                    cancel.as_ref().cloned(),
                ));
                files_done += 1;
            }
        }
    }

    // ---- highlights ----
    if opts.highlights && !highlights_tray.is_empty() {
        let hl_root = dir.join("highlights");
        if let Err(error) = std::fs::create_dir_all(&hl_root) {
            return finish_downloads(files_done, Some(error.to_string()));
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
                return finish_downloads(files_done, Some(error.to_string()));
            }
            let items = match client.highlight_items(hl_pk).await {
                Ok(items) => items,
                Err(error) => return finish_downloads(files_done, Some(error.to_string())),
            };
            for item in &items {
                if is_cancelled() {
                    return Err(JobFail::Cancelled);
                }
                let pk = value_pk(item);
                let taken_at = parse_ts(item);
                let base = taken_at_name(taken_at, &pk);
                if stem_exists(&skip, &base) {
                    continue;
                }
                for resource in &crate::hiker::collect_resources(item, infer_video(item)) {
                    try_file!(download_one(
                        cdn_http,
                        &resource.url,
                        &hl_dir.join(&base),
                        taken_at,
                        em,
                        files_done + 1,
                        &mut bytes_total,
                        cancel.as_ref().cloned(),
                    ));
                    files_done += 1;
                }
            }
        }
    }

    finish_downloads(files_done, last_error)
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
        let dir = Path::new(&cfg.dest_dir).join(safe_segment(
            post.owner_username
                .as_deref()
                .or(post.owner_pk.as_deref())
                .unwrap_or("unknown"),
        ));
        let result = run_single_post(&cdn_http, &cfg, &em, &dir, &post, Some(cancel_rx)).await;
        match result {
            Ok(count) => em.done(count, &dir),
            Err(JobFail::Cancelled) => em.cancelled(),
            Err(JobFail::Fatal(e)) => em.failed(e),
        }
        jobs.finish(&job_id);
        in_flight.lock().unwrap().remove(&key);
    });

    Ok(job_id)
}

async fn run_single_post(
    cdn_http: &reqwest::Client,
    cfg: &Config,
    em: &JobEvents,
    dir: &Path,
    post: &Post,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<usize, JobFail> {
    std::fs::create_dir_all(dir)?;
    let base = taken_at_name(post.taken_at, &post.code);
    let total = post.resources.len();
    let mut bytes_total = 0u64;
    let mut downloaded = 0usize;
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
        match download_one(
            cdn_http,
            &resource.url,
            &dest_base,
            post.taken_at,
            em,
            idx + 1,
            &mut bytes_total,
            cancel.as_ref().cloned(),
        )
        .await
        {
            Ok(out) => {
                downloaded += 1;
                if idx == 0 {
                    if let Err(error) = write_sidecar(cfg, dir, post, &out) {
                        last_error = Some(error);
                    }
                }
            }
            Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
            Err(JobFail::Fatal(error)) => last_error = Some(error),
        }
    }
    finish_downloads(downloaded, last_error)
}

#[tauri::command]
pub async fn cancel_job(job_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.jobs.cancel(&job_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_item(pk: &str) -> DirectItem {
        DirectItem {
            url: format!("https://cdninstagram.com/{pk}.jpg"),
            taken_at: None,
            pk: pk.to_string(),
        }
    }

    #[test]
    fn full_failure_preserves_the_concrete_error() {
        assert!(matches!(
            finish_downloads(0, Some("HTTP 403".into())),
            Err(JobFail::Fatal(error)) if error.contains("HTTP 403")
        ));
    }

    #[test]
    fn partial_success_reports_only_written_files() {
        assert!(matches!(
            finish_downloads(2, Some("HTTP 403".into())),
            Ok(2)
        ));
    }

    #[test]
    fn all_skipped_downloads_are_a_successful_noop() {
        assert!(matches!(finish_downloads(0, None), Ok(0)));
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
