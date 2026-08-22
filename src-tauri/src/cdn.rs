//! CDN streamer: the single place every Instagram CDN download goes through.
//! Port of insto's `_cdn.py` safety rules:
//! HTTPS-only, host allowlist, manual redirects (≤5), MIME magic-byte
//! cross-check, extension allowlist, byte budget, disk guard, atomic
//! `.part` writes with collision suffixes, mtime preservation.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;

pub const ALLOWED_HOST_SUFFIXES: [&str; 2] = ["cdninstagram.com", "fbcdn.net"];
const MAX_REDIRECTS: usize = 5;
const SNIFF_SIZE: usize = 512;
const DEFAULT_BYTE_BUDGET: u64 = 500 * 1024 * 1024;
const MIN_FREE_DISK: u64 = 1024 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum CdnError {
    #[error("CDN URL must be https")]
    NotHttps,
    #[error("CDN host {0} is not allowed")]
    HostNotAllowed(String),
    #[error("Too many redirects")]
    TooManyRedirects,
    #[error("Redirect without Location header")]
    BadRedirect,
    #[error("CDN GET failed: HTTP {0}")]
    Http(u16),
    #[error("Empty response body")]
    EmptyBody,
    #[error("Unknown content type (sniffed {sniffed:?}, declared {declared:?})")]
    UnknownContent {
        sniffed: Option<String>,
        declared: Option<String>,
    },
    #[error("File exceeds byte budget ({budget} bytes)")]
    BudgetExceeded { budget: u64 },
    #[error("Not enough free disk space in {}", .0.display())]
    NoDiskSpace(PathBuf),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Sniffed {
    Jpeg,
    Png,
    Webp,
    Mp4,
}

impl Sniffed {
    fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
            return Some(Self::Jpeg);
        }
        if bytes.len() >= 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
            return Some(Self::Png);
        }
        if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            return Some(Self::Webp);
        }
        // ISOBMFF: box size at [0..4], brand at [4..8]
        if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
            return Some(Self::Mp4);
        }
        None
    }

    fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Mp4 => "video/mp4",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Mp4 => "mp4",
        }
    }
}

fn normalize_ct(ct: &str) -> String {
    ct.split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// Content-Type → extension map used when the sniff is inconclusive for ext
/// decisions but valid for cross-checking. jpeg/jpg treated as aliases.
fn ct_compatible(declared: &str, sniffed: Sniffed) -> bool {
    let d = normalize_ct(declared);
    if d.is_empty() || d == "application/octet-stream" || d == "binary/octet-stream" {
        return true; // no usable declaration to contradict the sniff
    }
    let s = sniffed.mime();
    if d == s {
        return true;
    }
    if sniffed == Sniffed::Jpeg && (d == "image/jpg" || d == "image/pjpeg") {
        return true;
    }
    if sniffed == Sniffed::Mp4 && (d == "video/quicktime" || d == "video/mp4") {
        return true;
    }
    false
}

fn validate_url(url: &str) -> Result<(), CdnError> {
    let parsed = url::Url::parse(url).map_err(|_| CdnError::Network(format!("bad url: {url}")))?;
    if parsed.scheme() != "https" {
        // Unit tests drive the streamer against a local plain-HTTP mock.
        #[cfg(test)]
        if parsed.scheme() == "http" && parsed.host_str() == Some("127.0.0.1") {
            return Ok(());
        }
        return Err(CdnError::NotHttps);
    }
    let host = match parsed.host() {
        Some(h) => h.to_string(),
        None => return Err(CdnError::HostNotAllowed(url.into())),
    };
    #[cfg(test)]
    if host == "127.0.0.1" {
        return Ok(());
    }
    if !ALLOWED_HOST_SUFFIXES
        .iter()
        .any(|s| host == *s || host.ends_with(&format!(".{s}")))
    {
        return Err(CdnError::HostNotAllowed(host));
    }
    Ok(())
}

static RESERVED_PATHS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

struct PathReservation {
    path: PathBuf,
}

impl PathReservation {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PathReservation {
    fn drop(&mut self) {
        if let Some(reservations) = RESERVED_PATHS.get() {
            reservations.lock().unwrap().remove(&self.path);
        }
    }
}

fn reserve_collision(dest: &Path) -> PathReservation {
    let stem = dest.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let parent = dest.parent().unwrap_or(Path::new("."));
    let ext = dest.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mut reservations = RESERVED_PATHS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap();

    for i in 0..=u32::MAX {
        let candidate = match (i, ext.is_empty()) {
            (0, _) => dest.to_path_buf(),
            (_, true) => parent.join(format!("{stem}_{i}")),
            (_, false) => parent.join(format!("{stem}_{i}.{ext}")),
        };
        if !candidate.exists() && reservations.insert(candidate.clone()) {
            return PathReservation { path: candidate };
        }
    }
    unreachable!("exhausted collision suffix space")
}

fn check_disk_space(dir: &Path) -> Result<(), CdnError> {
    if let Some(parent) = dir.parent() {
        let _ = fs::create_dir_all(parent);
        let target = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        if let Ok(free) = fs2::free_space(&target) {
            if free < MIN_FREE_DISK {
                return Err(CdnError::NoDiskSpace(target));
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct DownloadOutcome {
    pub path: PathBuf,
    pub bytes: u64,
}

/// Public entry point with the production byte budget.
#[allow(clippy::too_many_arguments)]
pub async fn stream_to_file<F>(
    http: &reqwest::Client,
    url: &str,
    dest_base: &Path,
    taken_at_unix: Option<i64>,
    progress: F,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<DownloadOutcome, CdnError>
where
    F: FnMut(u64),
{
    stream_to_file_with_budget(
        http,
        url,
        dest_base,
        taken_at_unix,
        progress,
        cancel,
        DEFAULT_BYTE_BUDGET,
    )
    .await
}

/// Stream `url` into `<dest_base>.<ext>` enforcing all safety rules.
/// `progress` receives cumulative byte counts per chunk.
/// Dropping or signalling `cancel` aborts the download (stale `.part`
/// removed by the caller-visible error path).
pub async fn stream_to_file_with_budget<F>(
    http: &reqwest::Client,
    url: &str,
    dest_base: &Path,
    taken_at_unix: Option<i64>,
    mut progress: F,
    mut cancel: Option<tokio::sync::watch::Receiver<bool>>,
    byte_budget: u64,
) -> Result<DownloadOutcome, CdnError>
where
    F: FnMut(u64),
{
    if cancel.as_ref().map(|c| *c.borrow()).unwrap_or(false) {
        return Err(CdnError::Cancelled);
    }
    validate_url(url)?;

    let mut current = url.to_string();
    let mut hops = 0usize;
    let resp = loop {
        let req = http.get(&current).timeout(Duration::from_secs(30));
        let r = tokio::select! {
            biased;
            _ = cancel_wait(&mut cancel), if cancel.is_some() => return Err(CdnError::Cancelled),
            r = req.send() => r.map_err(|e| CdnError::Network(e.to_string()))?,
        };
        if r.status().is_redirection() {
            hops += 1;
            if hops > MAX_REDIRECTS {
                return Err(CdnError::TooManyRedirects);
            }
            let loc = r
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or(CdnError::BadRedirect)?
                .to_string();
            validate_url(&loc)?;
            current = loc;
            continue;
        }
        break r;
    };

    if !resp.status().is_success() {
        return Err(CdnError::Http(resp.status().as_u16()));
    }

    let declared_ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(normalize_ct);

    // Buffer up to SNIFF_SIZE before touching disk so MIME checks happen
    // before any file exists.
    let mut stream = resp.bytes_stream();
    let mut sniff = Vec::with_capacity(SNIFF_SIZE);
    while sniff.len() < SNIFF_SIZE {
        match stream.next().await {
            Some(Ok(chunk)) => {
                let take = (SNIFF_SIZE - sniff.len()).min(chunk.len());
                sniff.extend_from_slice(&chunk[..take]);
                if chunk.len() > take {
                    // keep remainder for writing below
                    sniff.extend_from_slice(&chunk[take..]);
                    break;
                }
            }
            Some(Err(e)) => return Err(CdnError::Network(e.to_string())),
            None => break,
        }
    }
    if sniff.is_empty() {
        return Err(CdnError::EmptyBody);
    }
    let kind = Sniffed::detect(&sniff).ok_or(CdnError::UnknownContent {
        sniffed: None,
        declared: declared_ct.clone(),
    })?;
    if let Some(ct) = &declared_ct {
        if !ct_compatible(ct, kind) {
            return Err(CdnError::UnknownContent {
                sniffed: Some(kind.mime().into()),
                declared: Some(ct.clone()),
            });
        }
    }

    let reservation = reserve_collision(&dest_base.with_extension(kind.ext()));
    let final_path = reservation.path().to_path_buf();
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent).map_err(CdnError::Io)?;
    }
    check_disk_space(&final_path)?;

    let part_path = final_path.with_extension(format!(
        "{}.{}.part",
        final_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(""),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    ));
    let _ = fs::remove_file(&part_path);

    let result = write_stream(
        stream,
        sniff,
        &part_path,
        &final_path,
        byte_budget,
        &mut progress,
        &mut cancel,
    )
    .await;
    match result {
        Ok(bytes) => {
            if let Some(unix) = taken_at_unix {
                if let Some(ts) = chrono::DateTime::<chrono::Utc>::from_timestamp(unix, 0) {
                    set_mtime(&final_path, ts.timestamp());
                }
            }
            Ok(DownloadOutcome {
                path: final_path,
                bytes,
            })
        }
        Err(e) => {
            let _ = fs::remove_file(&part_path);
            Err(e)
        }
    }
}

async fn cancel_wait(cancel: &mut Option<tokio::sync::watch::Receiver<bool>>) {
    match cancel {
        Some(rx) => {
            if *rx.borrow() {
                return;
            }
            let _ = rx.changed().await;
        }
        None => std::future::pending::<()>().await,
    }
}

async fn write_stream<S, F>(
    mut stream: S,
    first_chunk: Vec<u8>,
    part_path: &Path,
    final_path: &Path,
    byte_budget: u64,
    progress: &mut F,
    cancel: &mut Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<u64, CdnError>
where
    S: futures_util::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
    F: FnMut(u64),
{
    let mut file = fs::File::create(part_path).map_err(CdnError::Io)?;
    let mut written: u64 = 0;

    macro_rules! budget_check {
        () => {
            if written > byte_budget {
                return Err(CdnError::BudgetExceeded {
                    budget: byte_budget,
                });
            }
        };
    }

    file.write_all(&first_chunk).map_err(CdnError::Io)?;
    written += first_chunk.len() as u64;
    budget_check!();
    progress(written);

    loop {
        let chunk = tokio::select! {
            biased;
            _ = cancel_wait(cancel), if cancel.is_some() => return Err(CdnError::Cancelled),
            c = stream.next() => c,
        };
        let Some(chunk) = chunk else { break };
        let chunk = chunk.map_err(|e| CdnError::Network(e.to_string()))?;
        file.write_all(&chunk).map_err(CdnError::Io)?;
        written += chunk.len() as u64;
        budget_check!();
        progress(written);
    }
    file.flush().map_err(CdnError::Io)?;
    file.sync_all().map_err(CdnError::Io)?;
    drop(file);
    fs::rename(part_path, final_path).map_err(CdnError::Io)?;
    Ok(written)
}

#[cfg(unix)]
fn set_mtime(path: &Path, unix_ts: i64) {
    let t = filetime::FileTime::from_unix_time(unix_ts, 0);
    let _ = filetime::set_file_times(path, t, t);
}

#[cfg(not(unix))]
fn set_mtime(_path: &Path, _unix_ts: i64) {}

/// `stream_to_file` with transient-error retries: network failures and
/// HTTP 5xx are retried with linear backoff; safety rejections
/// (MIME, budget, allowlist, cancel) are never retried.
pub async fn stream_to_file_retried<F>(
    http: &reqwest::Client,
    url: &str,
    dest_base: &Path,
    taken_at_unix: Option<i64>,
    mut progress: F,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
    attempts: usize,
) -> Result<DownloadOutcome, CdnError>
where
    F: FnMut(u64),
{
    let mut last: Option<CdnError> = None;
    for attempt in 0..attempts.max(1) {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(600 * attempt as u64)).await;
        }
        let result = stream_to_file(
            http,
            url,
            dest_base,
            taken_at_unix,
            &mut progress,
            cancel.clone(),
        )
        .await;
        match result {
            Ok(outcome) => return Ok(outcome),
            Err(e @ CdnError::Network(_)) => last = Some(e),
            Err(e @ CdnError::Http(status)) if status >= 500 => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or(CdnError::Network("download failed".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const JPEG: [u8; 8] = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4];
    const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("idlg-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn jpeg_sniffs_to_jpg_extension() {
        let server = MockServer::start().await;
        Mock::given(path("/img"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/jpeg")
                    .set_body_bytes(JPEG),
            )
            .mount(&server)
            .await;
        let out = stream_to_file_with_budget(
            &client(),
            &format!("{}/img", server.uri()),
            &tmp_dir("jpg").join("2026-01-01_00-00-00_abc"),
            None,
            |_| {},
            None,
            1024 * 1024,
        )
        .await
        .unwrap();
        assert!(out.path.ends_with("2026-01-01_00-00-00_abc.jpg"));
        assert_eq!(out.bytes, 8);
    }

    #[tokio::test]
    async fn content_type_mismatch_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(path("/liar"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "video/mp4")
                    .set_body_bytes(JPEG),
            )
            .mount(&server)
            .await;
        let err = stream_to_file_with_budget(
            &client(),
            &format!("{}/liar", server.uri()),
            &tmp_dir("mismatch").join("x"),
            None,
            |_| {},
            None,
            1024 * 1024,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CdnError::UnknownContent { .. }), "{err}");
    }

    #[tokio::test]
    async fn unknown_magic_bytes_are_rejected() {
        let server = MockServer::start().await;
        Mock::given(path("/weird"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/octet-stream")
                    .set_body_bytes(b"NOT A REAL FILE FORMAT AT ALL........"),
            )
            .mount(&server)
            .await;
        let err = stream_to_file_with_budget(
            &client(),
            &format!("{}/weird", server.uri()),
            &tmp_dir("weird").join("x"),
            None,
            |_| {},
            None,
            1024 * 1024,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CdnError::UnknownContent { .. }), "{err}");
    }

    #[tokio::test]
    async fn empty_body_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(path("/empty"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes([]))
            .mount(&server)
            .await;
        let err = stream_to_file_with_budget(
            &client(),
            &format!("{}/empty", server.uri()),
            &tmp_dir("empty").join("x"),
            None,
            |_| {},
            None,
            1024 * 1024,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CdnError::EmptyBody), "{err}");
    }

    #[tokio::test]
    async fn redirects_are_followed_within_allowlist() {
        let server = MockServer::start().await;
        Mock::given(path("/jump"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/final", server.uri())),
            )
            .mount(&server)
            .await;
        Mock::given(path("/final"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/png")
                    .set_body_bytes(PNG),
            )
            .mount(&server)
            .await;
        let out = stream_to_file_with_budget(
            &client(),
            &format!("{}/jump", server.uri()),
            &tmp_dir("redir").join("y"),
            None,
            |_| {},
            None,
            1024 * 1024,
        )
        .await
        .unwrap();
        assert!(out.path.ends_with("y.png"));
    }

    #[tokio::test]
    async fn cross_host_redirect_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(path("/evil"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", "https://evil.example.com/x.jpg"),
            )
            .mount(&server)
            .await;
        let err = stream_to_file_with_budget(
            &client(),
            &format!("{}/evil", server.uri()),
            &tmp_dir("evil").join("z"),
            None,
            |_| {},
            None,
            1024 * 1024,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CdnError::HostNotAllowed(_)), "{err}");
    }

    #[tokio::test]
    async fn redirect_loop_hits_limit() {
        let server = MockServer::start().await;
        Mock::given(path("/loop"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/loop", server.uri())),
            )
            .mount(&server)
            .await;
        let err = stream_to_file_with_budget(
            &client(),
            &format!("{}/loop", server.uri()),
            &tmp_dir("loop").join("l"),
            None,
            |_| {},
            None,
            1024 * 1024,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CdnError::TooManyRedirects), "{err}");
    }

    #[tokio::test]
    async fn byte_budget_aborts_large_body() {
        let dir = tmp_dir("budget");
        let server = MockServer::start().await;
        let mut big = vec![0xFF, 0xD8, 0xFF, 0xE0];
        big.resize(4096, 0xEE);
        Mock::given(path("/big"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/jpeg")
                    .set_body_bytes(big),
            )
            .mount(&server)
            .await;
        let err = stream_to_file_with_budget(
            &client(),
            &format!("{}/big", server.uri()),
            &dir.join("b"),
            None,
            |_| {},
            None,
            100,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CdnError::BudgetExceeded { .. }), "{err}");
        assert!(!dir.join("b.part").exists());
    }

    #[tokio::test]
    async fn collision_gets_numeric_suffix() {
        let dir = tmp_dir("collision");
        let existing = dir.join("c.jpg");
        fs::write(&existing, b"old").unwrap();

        let server = MockServer::start().await;
        Mock::given(path("/img"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/jpeg")
                    .set_body_bytes(JPEG),
            )
            .mount(&server)
            .await;

        let out = stream_to_file_with_budget(
            &client(),
            &format!("{}/img", server.uri()),
            &dir.join("c"),
            None,
            |_| {},
            None,
            1024 * 1024,
        )
        .await
        .unwrap();
        assert!(out.path.ends_with("c_1.jpg"), "{:?}", out.path);
        assert_eq!(fs::read(&existing).unwrap(), b"old");
    }

    #[test]
    fn simultaneous_reservations_get_distinct_paths() {
        let dir = tmp_dir("reservations");
        let base = dir.join("c.jpg");

        let first = reserve_collision(&base);
        let second = reserve_collision(&base);

        assert_eq!(first.path(), base);
        assert_eq!(second.path(), dir.join("c_1.jpg"));
        drop(first);
        drop(second);

        let again = reserve_collision(&base);
        assert_eq!(again.path(), base);
    }

    #[tokio::test]
    async fn cancel_aborts_before_start() {
        let dir = tmp_dir("cancel");
        let server = MockServer::start().await;
        let big = PNG.repeat(64);
        Mock::given(path("/slow"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/png")
                    .set_body_bytes(big),
            )
            .mount(&server)
            .await;

        let (tx, rx) = tokio::sync::watch::channel(false);
        tx.send_replace(true);

        let err = stream_to_file_with_budget(
            &client(),
            &format!("{}/slow", server.uri()),
            &dir.join("s"),
            None,
            |_| {},
            Some(rx),
            1024 * 1024,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CdnError::Cancelled), "{err}");
    }

    #[tokio::test]
    async fn mtime_is_preserved_from_taken_at() {
        let dir = tmp_dir("mtime");
        let server = MockServer::start().await;
        Mock::given(path("/old"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/jpeg")
                    .set_body_bytes(JPEG),
            )
            .mount(&server)
            .await;
        let taken_at = 1_700_000_000; // Nov 2023
        let out = stream_to_file_with_budget(
            &client(),
            &format!("{}/old", server.uri()),
            &dir.join("m"),
            Some(taken_at),
            |_| {},
            None,
            1024 * 1024,
        )
        .await
        .unwrap();
        use std::os::unix::fs::MetadataExt;
        assert_eq!(out.path.metadata().unwrap().mtime(), taken_at);
    }
}
