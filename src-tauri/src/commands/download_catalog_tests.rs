use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;

use tempfile::TempDir;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::catalog::{
    FileAvailability, LibraryFile, LibraryItemDetail, LibraryQuery, LibrarySort, MediaFileKind,
};
use crate::models::{MediaKind, MediaResource};

const JPEG: [u8; 8] = [0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 4];
const MP4: [u8; 16] = [
    0, 0, 0, 16, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0, 0, 0, 0,
];

#[derive(Default)]
struct RecordingProgress {
    updates: Mutex<Vec<(usize, usize, u64)>>,
}

impl RecordingProgress {
    fn updates(&self) -> Vec<(usize, usize, u64)> {
        self.updates.lock().unwrap().clone()
    }
}

impl ProgressSink for RecordingProgress {
    fn progress(&self, current_file: usize, total_files: usize, bytes_done: u64, _file_name: &str) {
        self.updates
            .lock()
            .unwrap()
            .push((current_file, total_files, bytes_done));
    }
}

struct TestResult {
    outcome: JobOutcome,
    resource_errors: Vec<String>,
    relative_paths: Vec<String>,
}

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    database_path: PathBuf,
    catalog: Catalog,
    http: reqwest::Client,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("archive");
        fs::create_dir_all(&root).unwrap();
        let database_path = temp.path().join("catalog.sqlite3");
        let catalog = Catalog::open(&database_path).unwrap();
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        Self {
            _temp: temp,
            root,
            database_path,
            catalog,
            http,
        }
    }

    async fn run(&self, post: &Post, sidecar: bool) -> TestResult {
        let cfg = Config {
            token: None,
            dest_dir: self.root.to_string_lossy().into_owned(),
            sidecar,
        };
        let completed = run_single_post(
            &self.http,
            &self.catalog,
            &self.root,
            &cfg,
            &NoopProgress,
            &self.root.join("nested/profile"),
            post,
            None,
            true,
        )
        .await
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("download was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("download failed: {error}"),
        });
        let (resource_errors, relative_paths) = match &completed.media {
            Some(media) => (
                media.resource_errors.clone(),
                relative_paths(&self.root, &media.files),
            ),
            None => (Vec::new(), Vec::new()),
        };
        TestResult {
            outcome: completed.outcome,
            resource_errors,
            relative_paths,
        }
    }

    async fn run_fetched_posts(
        &self,
        posts: &[Post],
        sidecar: bool,
        progress: &dyn ProgressSink,
        cancel: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<CompletedJob, JobFail> {
        let cfg = Config {
            token: None,
            dest_dir: self.root.to_string_lossy().into_owned(),
            sidecar,
        };
        run_fetched_posts_job(
            &self.http,
            &self.catalog,
            &self.root,
            &cfg,
            progress,
            &self.root.join("nike/posts"),
            posts,
            cancel,
            true,
        )
        .await
    }

    fn all_items(&self) -> Vec<LibraryItemDetail> {
        self.catalog
            .query_library(&LibraryQuery {
                search: None,
                kinds: Vec::new(),
                source_id: None,
                availability: Some(FileAvailability::Available),
                taken_after: None,
                taken_before: None,
                sort: LibrarySort::ImportedAtDesc,
                cursor: None,
                limit: 60,
            })
            .unwrap()
            .items
            .into_iter()
            .map(|item| self.catalog.get_library_item(item.id).unwrap().unwrap())
            .collect()
    }

    fn only_item(&self) -> LibraryItemDetail {
        let page = self
            .catalog
            .query_library(&LibraryQuery {
                search: None,
                kinds: Vec::new(),
                source_id: None,
                availability: Some(FileAvailability::Available),
                taken_after: None,
                taken_before: None,
                sort: LibrarySort::ImportedAtDesc,
                cursor: None,
                limit: 60,
            })
            .unwrap();
        assert_eq!(page.items.len(), 1, "expected one logical media item");
        self.catalog
            .get_library_item(page.items[0].id)
            .unwrap()
            .unwrap()
    }

    async fn run_profile(
        &self,
        client: Arc<crate::hiker::HikerClient>,
        profile: &Profile,
        opts: &ProfileOptions,
    ) -> CompletedJob {
        let cfg = Config {
            token: None,
            dest_dir: self.root.to_string_lossy().into_owned(),
            sidecar: false,
        };
        run_profile_job(
            &client,
            &self.http,
            &self.catalog,
            &self.root,
            &cfg,
            &NoopProgress,
            &self.root.join(&profile.username),
            profile,
            opts,
            Vec::new(),
            Vec::new(),
            true,
            None,
        )
        .await
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("profile download was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("profile download failed: {error}"),
        })
    }
}

fn profile() -> Profile {
    Profile {
        pk: "42".into(),
        username: "nike".into(),
        full_name: Some("Nike".into()),
        media_count: 3,
        follower_count: Some(100),
        is_private: false,
        is_verified: true,
        avatar_url: None,
    }
}

fn reel_payload(pk: &str, code: &str) -> serde_json::Value {
    serde_json::json!({
        "pk": pk,
        "code": code,
        "media_type": 2,
        "taken_at": 1_700_000_000,
        "video_versions": [],
        "image_versions2": {"candidates": []}
    })
}

fn downloadable_reel_payload(server: &MockServer, pk: &str, code: &str) -> serde_json::Value {
    serde_json::json!({
        "pk": pk,
        "code": code,
        "media_type": 2,
        "taken_at": 1_700_000_000,
        "video_versions": [{
            "url": format!("{}/cdn/{pk}", server.uri()),
            "width": 1080
        }],
        "image_versions2": {"candidates": []}
    })
}

fn reels_only(max_posts: Option<u64>) -> ProfileOptions {
    ProfileOptions {
        posts: false,
        reels: true,
        stories: false,
        highlights: false,
        avatar: false,
        max_posts,
    }
}

#[tokio::test]
async fn reels_only_profile_job_uses_clips_and_caps_by_reel_count() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/user/medias/chunk"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/user/clips/chunk"))
        .and(query_param("user_id", "42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            [
                reel_payload("101", "ONE"),
                reel_payload("102", "TWO"),
                reel_payload("103", "THREE")
            ],
            "next-page"
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let fixture = Fixture::new();
    let client = Arc::new(crate::hiker::HikerClient::with_base_url(
        "token".into(),
        server.uri(),
    ));

    let completed = fixture
        .run_profile(client, &profile(), &reels_only(Some(2)))
        .await;

    assert_eq!(completed.outcome.files_written, 0);
    assert!(completed.resource_errors.is_empty());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/v1/user/clips/chunk")
            .count(),
        1
    );
    assert!(requests
        .iter()
        .all(|request| request.url.path() != "/cdn/103"));
}

#[tokio::test]
async fn reels_only_profile_job_stops_when_clips_cursor_repeats() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/user/clips/chunk"))
        .and(query_param_is_missing("end_cursor"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([[reel_payload("201", "FIRST")], "same"])),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/user/clips/chunk"))
        .and(query_param("end_cursor", "same"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([[reel_payload("202", "SECOND")], "same"])),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    let fixture = Fixture::new();
    let client = Arc::new(crate::hiker::HikerClient::with_base_url(
        "token".into(),
        server.uri(),
    ));

    let completed = fixture
        .run_profile(client, &profile(), &reels_only(None))
        .await;

    assert!(completed.resource_errors.is_empty());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/v1/user/clips/chunk")
            .count(),
        2
    );
}

#[tokio::test]
async fn reels_only_profile_job_stops_when_clips_cursors_cycle() {
    let server = MockServer::start().await;
    for (cursor, pk, next) in [
        (None, "301", "a"),
        (Some("a"), "302", "b"),
        (Some("b"), "303", "a"),
    ] {
        let mut mock = Mock::given(method("GET"))
            .and(path("/v1/user/clips/chunk"))
            .and(query_param("user_id", "42"));
        mock = match cursor {
            Some(value) => mock.and(query_param("end_cursor", value)),
            None => mock.and(query_param_is_missing("end_cursor")),
        };
        mock.respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([[reel_payload(pk, pk)], next])),
        )
        .expect(1)
        .mount(&server)
        .await;
    }
    let fixture = Fixture::new();
    let client = Arc::new(crate::hiker::HikerClient::with_base_url(
        "token".into(),
        server.uri(),
    ));

    let completed = fixture
        .run_profile(client, &profile(), &reels_only(None))
        .await;

    assert!(completed.resource_errors.is_empty());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/v1/user/clips/chunk")
            .count(),
        3
    );
}

#[tokio::test]
async fn reels_only_profile_job_caps_by_unique_reel_count_across_overlapping_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/user/clips/chunk"))
        .and(query_param_is_missing("end_cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            [
                downloadable_reel_payload(&server, "401", "ONE"),
                downloadable_reel_payload(&server, "402", "TWO")
            ],
            "next"
        ])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/user/clips/chunk"))
        .and(query_param("end_cursor", "next"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            [
                downloadable_reel_payload(&server, "402", "TWO"),
                downloadable_reel_payload(&server, "403", "THREE")
            ],
            null
        ])))
        .expect(1)
        .mount(&server)
        .await;
    for pk in ["401", "402", "403"] {
        Mock::given(method("GET"))
            .and(path(format!("/cdn/{pk}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(MP4))
            .expect(1)
            .mount(&server)
            .await;
    }
    let fixture = Fixture::new();
    let client = Arc::new(crate::hiker::HikerClient::with_base_url(
        "token".into(),
        server.uri(),
    ));

    let completed = fixture
        .run_profile(client, &profile(), &reels_only(Some(3)))
        .await;

    assert_eq!(completed.outcome.files_written, 3);
    assert!(completed.resource_errors.is_empty());
}

fn relative_paths(root: &Path, files: &[DownloadedFile]) -> Vec<String> {
    let canonical_root = root.canonicalize().unwrap();
    files
        .iter()
        .map(|file| {
            file.path
                .canonicalize()
                .unwrap()
                .strip_prefix(&canonical_root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn resource(server: &MockServer, route: &str, kind: MediaKind) -> MediaResource {
    MediaResource {
        url: format!("{}{route}", server.uri()),
        kind,
    }
}

fn post(pk: &str, code: &str, resources: Vec<MediaResource>) -> Post {
    Post {
        pk: pk.into(),
        code: code.into(),
        taken_at: Some(1_700_000_000),
        caption: Some("catalog integration".into()),
        like_count: Some(41),
        comment_count: Some(7),
        owner_username: Some("owner".into()),
        owner_pk: Some("9001".into()),
        resources,
        thumbnail_url: None,
    }
}

async fn mount_media(
    server: &MockServer,
    route: &'static str,
    content_type: &'static str,
    body: &[u8],
) {
    Mock::given(path(route))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", content_type)
                .set_body_bytes(body),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn fetched_batch_preserves_carousels_catalog_sidecars_and_global_progress() {
    let server = MockServer::start().await;
    mount_media(&server, "/carousel-photo", "image/jpeg", &JPEG).await;
    mount_media(&server, "/carousel-video", "video/mp4", &MP4).await;
    mount_media(&server, "/single-photo", "image/jpeg", &JPEG).await;
    let fixture = Fixture::new();
    let mut carousel = post(
        "batch-1",
        "BATCH-CAROUSEL",
        vec![
            resource(&server, "/carousel-photo", MediaKind::Photo),
            resource(&server, "/carousel-video", MediaKind::Video),
        ],
    );
    carousel.owner_username = Some("nike".into());
    carousel.owner_pk = Some("123".into());
    let mut single = post(
        "batch-2",
        "BATCH-SINGLE",
        vec![resource(&server, "/single-photo", MediaKind::Photo)],
    );
    single.owner_username = Some("nike".into());
    single.owner_pk = Some("123".into());
    let progress = RecordingProgress::default();

    let completed = fixture
        .run_fetched_posts(&[carousel, single], true, &progress, None)
        .await
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("fetched batch was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("fetched batch failed: {error}"),
        });

    assert_eq!(completed.outcome.files_written, 3);
    assert_eq!(completed.outcome.catalog_warnings, 0);
    assert!(completed.resource_errors.is_empty());
    let items = fixture.all_items();
    assert_eq!(items.len(), 2, "expected one catalog item per post");
    for item in items {
        assert_eq!(item.owner_username.as_deref(), Some("nike"));
        assert_eq!(item.owner_pk.as_deref(), Some("123"));
        let sidecars = item
            .files
            .iter()
            .filter(|file| file.kind == MediaFileKind::Metadata)
            .collect::<Vec<_>>();
        assert_eq!(sidecars.len(), 1, "expected one sidecar per post");
        assert!(absolute(&fixture.root, &sidecars[0].relative_path).is_file());
    }
    let updates = progress.updates();
    assert!(!updates.is_empty());
    assert!(updates.iter().all(|(_, total, _)| *total == 3));
    assert!(updates
        .windows(2)
        .all(|pair| { pair[0].0 <= pair[1].0 && pair[0].2 <= pair[1].2 }));
    assert_eq!(updates.last().map(|update| update.0), Some(3));
    assert_eq!(updates.last().map(|update| update.2), Some(32));
}

#[tokio::test]
async fn fetched_batch_recovered_success_and_later_failure_completes_with_error() {
    let server = MockServer::start().await;
    mount_media(&server, "/already-downloaded", "image/jpeg", &JPEG).await;
    Mock::given(path("/forbidden"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let fixture = Fixture::new();
    let recovered = post(
        "batch-recovered",
        "BATCH-RECOVERED",
        vec![resource(&server, "/already-downloaded", MediaKind::Photo)],
    );
    let failing = post(
        "batch-failing",
        "BATCH-FAILING",
        vec![resource(&server, "/forbidden", MediaKind::Photo)],
    );
    let seeded = fixture
        .run_fetched_posts(std::slice::from_ref(&recovered), false, &NoopProgress, None)
        .await
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("seed batch was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("seed batch failed: {error}"),
        });
    assert_eq!(seeded.outcome.files_written, 1);
    let before = fixture.all_items();

    let completed = fixture
        .run_fetched_posts(&[recovered, failing], false, &NoopProgress, None)
        .await
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("retry batch was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("retry batch failed: {error}"),
        });

    assert_eq!(completed.outcome.files_written, 0);
    assert_eq!(completed.resource_errors.len(), 1);
    assert!(completed.resource_errors[0].contains("HTTP 403"));
    let after = fixture.all_items();
    assert_eq!(after.len(), 1);
    assert_files_stable_after_refresh(&before[0].files, &after[0].files);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/already-downloaded")
            .count(),
        1,
        "the recovered resource must not be downloaded again"
    );
}

#[tokio::test]
async fn fetched_batch_already_cancelled_writes_no_media_or_catalog_items() {
    let server = MockServer::start().await;
    Mock::given(path("/must-not-download"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/jpeg")
                .set_body_bytes(JPEG),
        )
        .expect(0)
        .mount(&server)
        .await;
    let fixture = Fixture::new();
    let item = post(
        "batch-cancelled",
        "BATCH-CANCELLED",
        vec![resource(&server, "/must-not-download", MediaKind::Photo)],
    );
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(true);

    let result = fixture
        .run_fetched_posts(&[item], true, &NoopProgress, Some(cancel_rx))
        .await;

    assert!(matches!(result, Err(JobFail::Cancelled)));
    assert!(fixture.all_items().is_empty());
    let destination = fixture.root.join("nike/posts");
    assert!(
        !destination.exists() || fs::read_dir(destination).unwrap().next().is_none(),
        "cancelled batch must not write archive files"
    );
}

#[tokio::test]
async fn fetched_batch_retains_every_fatal_post_error_and_continues_to_later_posts() {
    let server = MockServer::start().await;
    Mock::given(path("/first-forbidden"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/second-missing"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    mount_media(&server, "/later-success", "image/jpeg", &JPEG).await;
    let fixture = Fixture::new();
    let failed = post(
        "batch-all-resource-failures",
        "BATCH-ALL-RESOURCE-FAILURES",
        vec![
            resource(&server, "/first-forbidden", MediaKind::Photo),
            resource(&server, "/second-missing", MediaKind::Video),
        ],
    );
    let succeeded = post(
        "batch-later-success",
        "BATCH-LATER-SUCCESS",
        vec![resource(&server, "/later-success", MediaKind::Photo)],
    );

    let completed = fixture
        .run_fetched_posts(&[failed, succeeded], false, &NoopProgress, None)
        .await
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("fetched batch was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("fetched batch failed: {error}"),
        });

    assert_eq!(completed.outcome.files_written, 1);
    assert_eq!(completed.resource_errors.len(), 2);
    assert!(completed
        .resource_errors
        .iter()
        .any(|error| error.contains("HTTP 403")));
    assert!(completed
        .resource_errors
        .iter()
        .any(|error| error.contains("HTTP 404")));
    assert_eq!(fixture.all_items().len(), 1);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/later-success")
            .count(),
        1,
        "a fatal first post must not prevent a later download"
    );
}

#[tokio::test]
async fn fetched_batch_with_no_successful_resources_is_fatal_after_trying_every_post() {
    let server = MockServer::start().await;
    Mock::given(path("/failed-first-post"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/failed-second-post"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let fixture = Fixture::new();
    let first = post(
        "batch-failed-first",
        "BATCH-FAILED-FIRST",
        vec![resource(&server, "/failed-first-post", MediaKind::Photo)],
    );
    let second = post(
        "batch-failed-second",
        "BATCH-FAILED-SECOND",
        vec![resource(&server, "/failed-second-post", MediaKind::Video)],
    );

    let result = fixture
        .run_fetched_posts(&[first, second], false, &NoopProgress, None)
        .await;

    match result {
        Err(JobFail::Fatal(error)) => assert!(error.contains("HTTP 404")),
        Err(JobFail::Cancelled) => panic!("fetched batch was unexpectedly cancelled"),
        Ok(_) => panic!("an all-failed fetched batch must be fatal"),
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "every post must be attempted");
}

#[tokio::test]
async fn fetched_batch_final_recovered_resource_emits_terminal_monotonic_progress() {
    let server = MockServer::start().await;
    mount_media(&server, "/new-first", "image/jpeg", &JPEG).await;
    mount_media(&server, "/recovered-final", "video/mp4", &MP4).await;
    let fixture = Fixture::new();
    let recovered = post(
        "batch-recovered-final",
        "BATCH-RECOVERED-FINAL",
        vec![resource(&server, "/recovered-final", MediaKind::Video)],
    );
    let seeded = fixture
        .run_fetched_posts(std::slice::from_ref(&recovered), false, &NoopProgress, None)
        .await
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("seed batch was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("seed batch failed: {error}"),
        });
    assert_eq!(seeded.outcome.files_written, 1);
    let new = post(
        "batch-new-first",
        "BATCH-NEW-FIRST",
        vec![resource(&server, "/new-first", MediaKind::Photo)],
    );
    let progress = RecordingProgress::default();

    let completed = fixture
        .run_fetched_posts(&[new, recovered], false, &progress, None)
        .await
        .unwrap_or_else(|failure| match failure {
            JobFail::Cancelled => panic!("fetched batch was unexpectedly cancelled"),
            JobFail::Fatal(error) => panic!("fetched batch failed: {error}"),
        });

    assert_eq!(completed.outcome.files_written, 1);
    assert!(completed.resource_errors.is_empty());
    let updates = progress.updates();
    assert!(updates.iter().all(|(_, total, _)| *total == 2));
    assert!(updates
        .windows(2)
        .all(|pair| { pair[0].0 <= pair[1].0 && pair[0].2 <= pair[1].2 }));
    assert_eq!(updates.last().copied(), Some((2, 2, 24)));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/recovered-final")
            .count(),
        1,
        "the terminal resource must be recovered instead of downloaded again"
    );
}

#[tokio::test]
async fn standalone_post_all_failed_after_attempt_refactor_is_concrete_fatal() {
    let server = MockServer::start().await;
    Mock::given(path("/standalone-forbidden"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;
    let fixture = Fixture::new();
    let cfg = Config {
        token: None,
        dest_dir: fixture.root.to_string_lossy().into_owned(),
        sidecar: false,
    };
    let item = post(
        "standalone-failed",
        "STANDALONE-FAILED",
        vec![resource(&server, "/standalone-forbidden", MediaKind::Photo)],
    );

    let result = run_single_post(
        &fixture.http,
        &fixture.catalog,
        &fixture.root,
        &cfg,
        &NoopProgress,
        &fixture.root.join("nested/profile"),
        &item,
        None,
        true,
    )
    .await;

    match result {
        Err(JobFail::Fatal(error)) => assert!(error.contains("HTTP 403")),
        Err(JobFail::Cancelled) => panic!("standalone post was unexpectedly cancelled"),
        Ok(_) => panic!("an all-failed standalone post must be fatal"),
    }
    assert!(fixture.all_items().is_empty());
}

fn absolute(root: &Path, relative: &str) -> PathBuf {
    root.join(relative)
}

fn assert_files_stable_after_refresh(before: &[LibraryFile], after: &[LibraryFile]) {
    assert_eq!(before.len(), after.len());
    for (before, after) in before.iter().zip(after) {
        assert_eq!(before.id, after.id);
        assert_eq!(before.root_id, after.root_id);
        assert_eq!(before.relative_path, after.relative_path);
        assert_eq!(before.ordinal, after.ordinal);
        assert_eq!(before.kind, after.kind);
        assert_eq!(before.byte_size, after.byte_size);
        assert_eq!(before.mtime, after.mtime);
        assert_eq!(before.exists_on_disk, after.exists_on_disk);
        assert!(
            after.last_seen_at >= before.last_seen_at,
            "last_seen_at moved backwards for {}",
            before.relative_path
        );
    }
}

fn collision_server() -> (
    String,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut slow, _) = listener.accept().unwrap();
        let slow_handler = thread::spawn(move || {
            let mut request = [0_u8; 1024];
            let read = slow.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..read]).contains("GET /slow "));
            let mut body = vec![0_u8; 1024];
            body[..JPEG.len()].copy_from_slice(&JPEG);
            write!(
                slow,
                "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            slow.write_all(&body[..512]).unwrap();
            slow.flush().unwrap();
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            slow.write_all(&body[512..]).unwrap();
            slow.flush().unwrap();
        });

        let (mut fast, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let read = fast.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..read]).contains("GET /fast "));
        write!(
            fast,
            "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            JPEG.len()
        )
        .unwrap();
        fast.write_all(&JPEG).unwrap();
        fast.flush().unwrap();
        slow_handler.join().unwrap();
    });
    (format!("http://{address}"), started_rx, release_tx, handle)
}

#[tokio::test]
async fn persists_the_exact_collision_adjusted_downloader_path() {
    let fixture = Fixture::new();
    let (server, slow_started, release_slow, server_thread) = collision_server();
    let item = post(
        "111",
        "COLLIDE",
        vec![MediaResource {
            url: format!("{server}/fast"),
            kind: MediaKind::Photo,
        }],
    );
    let requested_base = fixture.root.join("nested/profile").join(format!(
        "{}_COLLIDE",
        chrono::DateTime::<chrono::Utc>::from_timestamp(1_700_000_000, 0)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d_%H-%M-%S")
    ));
    fs::create_dir_all(requested_base.parent().unwrap()).unwrap();
    let blocker_client = fixture.http.clone();
    let blocker_base = requested_base.clone();
    let blocker_url = format!("{server}/slow");
    let blocker = tokio::spawn(async move {
        crate::cdn::stream_to_file_retried_for_test(
            &blocker_client,
            &blocker_url,
            &blocker_base,
            None,
            |_| {},
            None,
            1,
        )
        .await
        .unwrap()
    });
    let mut slow_response_started = false;
    for _ in 0..500 {
        if slow_started.try_recv().is_ok() {
            slow_response_started = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(slow_response_started, "slow mock response never started");
    let mut reservation_visible = false;
    for _ in 0..200 {
        reservation_visible = fs::read_dir(requested_base.parent().unwrap())
            .unwrap()
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "part"));
        if reservation_visible {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        reservation_visible,
        "slow download never reserved its final path"
    );

    let result = fixture.run(&item, true).await;
    release_slow.send(()).unwrap();
    let blocker_outcome = blocker.await.unwrap();
    server_thread.join().unwrap();

    let retried = fixture.run(&item, true).await;

    assert_eq!(result.outcome.files_written, 1);
    assert_eq!(result.outcome.catalog_warnings, 0);
    assert_eq!(retried.outcome.files_written, 0);
    assert!(retried.resource_errors.is_empty());
    let detail = fixture.only_item();
    assert_eq!(detail.remote_key, "post:111");
    assert_eq!(detail.files.len(), 2);
    assert!(detail.files[0].relative_path.ends_with("_1.jpg"));
    assert!(detail.files[1].relative_path.ends_with("_1.json"));
    assert!(blocker_outcome
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with("_COLLIDE.jpg"));
    assert_eq!(
        fs::read(absolute(&fixture.root, &detail.files[0].relative_path)).unwrap(),
        JPEG
    );
    for file in &detail.files {
        assert!(!file.relative_path.starts_with('/'));
        assert_ne!(
            absolute(&fixture.root, &file.relative_path),
            blocker_outcome.path
        );
    }
    assert!(!blocker_outcome.path.with_extension("json").exists());
}

#[tokio::test]
async fn carousel_is_one_item_with_resource_position_ordinals() {
    let server = MockServer::start().await;
    mount_media(&server, "/one", "image/jpeg", &JPEG).await;
    mount_media(&server, "/two", "video/mp4", &MP4).await;
    mount_media(&server, "/three", "image/jpeg", &JPEG).await;
    let fixture = Fixture::new();
    let item = post(
        "222",
        "CAROUSEL",
        vec![
            resource(&server, "/one", MediaKind::Photo),
            resource(&server, "/two", MediaKind::Video),
            resource(&server, "/three", MediaKind::Photo),
        ],
    );

    let result = fixture.run(&item, false).await;

    assert_eq!(result.outcome.files_written, 3);
    assert!(result.resource_errors.is_empty());
    let detail = fixture.only_item();
    assert_eq!(detail.remote_key, "post:222");
    assert_eq!(detail.kind, MediaItemKind::Post);
    assert_eq!(detail.remote_pk.as_deref(), Some("222"));
    assert_eq!(detail.shortcode.as_deref(), Some("CAROUSEL"));
    assert_eq!(detail.owner_pk.as_deref(), Some("9001"));
    assert_eq!(detail.owner_username.as_deref(), Some("owner"));
    assert_eq!(detail.taken_at, Some(1_700_000_000));
    assert_eq!(detail.caption.as_deref(), Some("catalog integration"));
    assert_eq!(detail.like_count, Some(41));
    assert_eq!(detail.comment_count, Some(7));
    assert_eq!(
        detail
            .files
            .iter()
            .map(|file| (file.ordinal, file.kind))
            .collect::<Vec<_>>(),
        vec![
            (0, MediaFileKind::Photo),
            (1, MediaFileKind::Video),
            (2, MediaFileKind::Photo),
        ]
    );
}

#[tokio::test]
async fn sidecar_is_metadata_and_media_kind_comes_from_verified_bytes() {
    let server = MockServer::start().await;
    mount_media(&server, "/misleading.mp4", "image/jpeg", &JPEG).await;
    let fixture = Fixture::new();
    let item = post(
        "333",
        "SIDECAR",
        vec![resource(&server, "/misleading.mp4", MediaKind::Video)],
    );

    fixture.run(&item, true).await;

    let detail = fixture.only_item();
    assert_eq!(
        detail
            .files
            .iter()
            .map(|file| (file.ordinal, file.kind))
            .collect::<Vec<_>>(),
        vec![(0, MediaFileKind::Photo), (1, MediaFileKind::Metadata)]
    );
    for file in detail.files {
        assert!(absolute(&fixture.root, &file.relative_path).is_file());
    }
}

#[tokio::test]
async fn repeated_logical_download_keeps_the_same_catalog_rows_and_history() {
    let server = MockServer::start().await;
    mount_media(&server, "/same", "image/jpeg", &JPEG).await;
    let fixture = Fixture::new();
    let item = post(
        "444",
        "REPEAT",
        vec![resource(&server, "/same", MediaKind::Photo)],
    );

    let first = fixture.run(&item, true).await;
    let before = fixture.only_item();
    let second = fixture.run(&item, true).await;
    let after = fixture.only_item();

    assert_eq!(first.outcome.files_written, 1);
    assert_eq!(second.outcome.files_written, 0);
    assert_eq!(before.id, after.id);
    assert_eq!(before.imported_at, after.imported_at);
    assert_files_stable_after_refresh(&before.files, &after.files);
    assert_eq!(after.files.len(), 2);
}

#[tokio::test]
async fn catalog_failure_after_disk_success_is_only_one_warning() {
    let server = MockServer::start().await;
    mount_media(&server, "/durable", "image/jpeg", &JPEG).await;
    let fixture = Fixture::new();
    fs::remove_file(&fixture.database_path).unwrap();
    fs::create_dir(&fixture.database_path).unwrap();
    let item = post(
        "555",
        "DURABLE",
        vec![resource(&server, "/durable", MediaKind::Photo)],
    );

    let result = fixture.run(&item, false).await;

    assert_eq!(result.outcome.files_written, 1);
    assert_eq!(result.outcome.catalog_warnings, 1);
    assert!(result.resource_errors.is_empty());
    assert_eq!(result.relative_paths.len(), 1);
    assert_eq!(
        fs::read(absolute(&fixture.root, &result.relative_paths[0])).unwrap(),
        JPEG
    );
}

#[tokio::test]
async fn partial_resource_failure_catalogs_success_and_preserves_concrete_error() {
    let server = MockServer::start().await;
    Mock::given(path("/gone"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    mount_media(&server, "/good", "video/mp4", &MP4).await;
    let fixture = Fixture::new();
    let item = post(
        "666",
        "PARTIAL",
        vec![
            resource(&server, "/gone", MediaKind::Photo),
            resource(&server, "/good", MediaKind::Photo),
        ],
    );

    let result = fixture.run(&item, false).await;

    assert_eq!(result.outcome.files_written, 1);
    assert_eq!(result.outcome.catalog_warnings, 0);
    assert_eq!(result.resource_errors.len(), 1);
    assert!(result.resource_errors[0].contains("HTTP 403"));
    let detail = fixture.only_item();
    assert_eq!(detail.files.len(), 1);
    assert_eq!(detail.files[0].ordinal, 1);
    assert_eq!(detail.files[0].kind, MediaFileKind::Video);
    assert_eq!(
        fs::read(absolute(&fixture.root, &detail.files[0].relative_path)).unwrap(),
        MP4
    );
}

#[tokio::test]
async fn retry_recovers_successful_ordinals_and_only_downloads_missing_resources() {
    let server = MockServer::start().await;
    mount_media(&server, "/stable", "image/jpeg", &JPEG).await;
    Mock::given(path("/gone"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    mount_media(&server, "/recovered", "video/mp4", &MP4).await;
    let fixture = Fixture::new();
    let first_attempt = post(
        "777",
        "RETRY",
        vec![
            resource(&server, "/stable", MediaKind::Photo),
            resource(&server, "/gone", MediaKind::Video),
        ],
    );
    let retry = post(
        "777",
        "RETRY",
        vec![
            resource(&server, "/stable", MediaKind::Photo),
            resource(&server, "/recovered", MediaKind::Video),
        ],
    );

    let partial = fixture.run(&first_attempt, false).await;
    assert_eq!(partial.outcome.files_written, 1);
    assert_eq!(partial.resource_errors.len(), 1);
    assert!(partial.resource_errors[0].contains("HTTP 403"));
    assert_eq!(fixture.only_item().files[0].ordinal, 0);

    let completed = fixture.run(&retry, false).await;
    assert_eq!(completed.outcome.files_written, 1);
    assert!(completed.resource_errors.is_empty());
    let before_noop = fixture.only_item();
    assert_eq!(
        before_noop
            .files
            .iter()
            .map(|file| (file.ordinal, file.kind))
            .collect::<Vec<_>>(),
        vec![(0, MediaFileKind::Photo), (1, MediaFileKind::Video)]
    );

    let noop = fixture.run(&retry, false).await;
    assert_eq!(noop.outcome.files_written, 0);
    assert!(noop.resource_errors.is_empty());
    let after_noop = fixture.only_item();
    assert_files_stable_after_refresh(&before_noop.files, &after_noop.files);

    let requests = server.received_requests().await.unwrap();
    for route in ["/stable", "/gone", "/recovered"] {
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.url.path() == route)
                .count(),
            1,
            "unexpected request count for {route}"
        );
    }
}

#[tokio::test]
async fn retry_with_recovered_success_and_still_missing_resource_remains_partial_success() {
    let server = MockServer::start().await;
    mount_media(&server, "/kept", "image/jpeg", &JPEG).await;
    Mock::given(path("/still-gone"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let fixture = Fixture::new();
    let item = post(
        "778",
        "PARTIAL-RETRY",
        vec![
            resource(&server, "/kept", MediaKind::Photo),
            resource(&server, "/still-gone", MediaKind::Video),
        ],
    );

    let first = fixture.run(&item, false).await;
    let repeated = fixture.run(&item, false).await;

    assert_eq!(first.outcome.files_written, 1);
    assert_eq!(repeated.outcome.files_written, 0);
    assert_eq!(repeated.resource_errors.len(), 1);
    assert!(repeated.resource_errors[0].contains("HTTP 403"));
    let detail = fixture.only_item();
    assert_eq!(detail.files.len(), 1);
    assert_eq!(detail.files[0].ordinal, 0);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/kept")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/still-gone")
            .count(),
        2
    );
}

#[tokio::test]
async fn direct_propic_round_trip_keeps_avatar_identity_and_file_link() {
    let server = MockServer::start().await;
    mount_media(&server, "/avatar", "image/jpeg", &JPEG).await;
    let fixture = Fixture::new();
    let dir = fixture.root.join("owner/propic");
    let item = DirectItem {
        url: format!("{}/avatar", server.uri()),
        taken_at: Some(1_700_000_000),
        pk: "445566".into(),
    };

    let outcome = run_direct_job(
        &fixture.http,
        &fixture.catalog,
        &fixture.root,
        &NoopProgress,
        &dir,
        "owner",
        "propic",
        &[item],
        None,
        true,
    )
    .await
    .unwrap_or_else(|failure| match failure {
        JobFail::Cancelled => panic!("download was unexpectedly cancelled"),
        JobFail::Fatal(error) => panic!("download failed: {error}"),
    });

    assert_eq!(outcome.outcome.files_written, 1);
    assert!(outcome.resource_errors.is_empty());
    let downloaded = fixture.only_item();
    assert_eq!(downloaded.remote_key, "avatar:445566");
    assert_eq!(downloaded.kind, MediaItemKind::Avatar);
    assert_eq!(downloaded.files.len(), 1);
    assert_eq!(
        downloaded.files[0].relative_path,
        "owner/propic/avatar_445566.jpg"
    );

    let root = fixture.catalog.list_roots().unwrap().remove(0);
    let discovery = crate::scanner::discover_archive(root.id, &root.path).unwrap();
    let rescanned = discovery
        .groups
        .iter()
        .find(|group| group.remote_key == "avatar:445566")
        .expect("downloaded avatar should retain scanner identity");
    assert_eq!(rescanned.item.kind, MediaItemKind::Avatar);
    assert_eq!(rescanned.item.files.len(), 1);
    assert_eq!(
        rescanned.item.files[0].relative_path,
        downloaded.files[0].relative_path
    );
    fixture.catalog.upsert_media(&rescanned.item).unwrap();
    let after_scan = fixture.only_item();
    assert_eq!(after_scan.id, downloaded.id);
    assert_files_stable_after_refresh(&downloaded.files, &after_scan.files);
}

#[tokio::test]
async fn direct_partial_success_retains_concrete_failures_internally() {
    let server = MockServer::start().await;
    mount_media(&server, "/good-avatar", "image/jpeg", &JPEG).await;
    Mock::given(path("/gone-avatar"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let fixture = Fixture::new();
    let dir = fixture.root.join("owner/propic");
    let items = [
        DirectItem {
            url: format!("{}/good-avatar", server.uri()),
            taken_at: None,
            pk: "100".into(),
        },
        DirectItem {
            url: format!("{}/gone-avatar", server.uri()),
            taken_at: None,
            pk: "200".into(),
        },
    ];

    let completed = run_direct_job(
        &fixture.http,
        &fixture.catalog,
        &fixture.root,
        &NoopProgress,
        &dir,
        "owner",
        "propic",
        &items,
        None,
        true,
    )
    .await
    .unwrap_or_else(|failure| match failure {
        JobFail::Cancelled => panic!("download was unexpectedly cancelled"),
        JobFail::Fatal(error) => panic!("download failed: {error}"),
    });

    assert_eq!(completed.outcome.files_written, 1);
    assert_eq!(completed.resource_errors.len(), 1);
    assert!(completed.resource_errors[0].contains("HTTP 403"));
    assert!(!completed.resource_errors[0].contains("token="));
}

#[tokio::test]
async fn single_reel_sidecar_round_trip_keeps_post_identity_kind_and_file_links() {
    let server = MockServer::start().await;
    mount_media(&server, "/reel", "video/mp4", &MP4).await;
    let fixture = Fixture::new();
    let item = post(
        "888999",
        "REEL",
        vec![resource(&server, "/reel", MediaKind::Video)],
    );

    let result = fixture.run(&item, true).await;

    assert_eq!(result.outcome.files_written, 1);
    let downloaded = fixture.only_item();
    assert_eq!(downloaded.remote_key, "post:888999");
    assert_eq!(downloaded.kind, MediaItemKind::Reel);
    let root = fixture.catalog.list_roots().unwrap().remove(0);
    let discovery = crate::scanner::discover_archive(root.id, &root.path).unwrap();
    let rescanned = discovery
        .groups
        .iter()
        .find(|group| group.remote_key == "post:888999")
        .expect("downloaded reel should retain the stable post namespace");
    assert_eq!(rescanned.item.kind, MediaItemKind::Reel);
    assert_eq!(
        rescanned
            .item
            .files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect::<Vec<_>>(),
        downloaded
            .files
            .iter()
            .map(|file| PathBuf::from(&file.relative_path))
            .collect::<Vec<_>>()
    );
    fixture.catalog.upsert_media(&rescanned.item).unwrap();
    let after_scan = fixture.only_item();
    assert_eq!(after_scan.id, downloaded.id);
    assert_eq!(after_scan.kind, MediaItemKind::Reel);
    assert_files_stable_after_refresh(&downloaded.files, &after_scan.files);
}

#[tokio::test]
async fn repeat_upgrades_a_legacy_reel_sidecar_without_redownloading_media() {
    let server = MockServer::start().await;
    mount_media(&server, "/legacy-reel", "video/mp4", &MP4).await;
    let fixture = Fixture::new();
    let item = post(
        "990011",
        "LEGACY",
        vec![resource(&server, "/legacy-reel", MediaKind::Video)],
    );
    fixture.run(&item, true).await;
    let before = fixture.only_item();
    let sidecar = before
        .files
        .iter()
        .find(|file| file.kind == MediaFileKind::Metadata)
        .unwrap();
    fs::write(
        absolute(&fixture.root, &sidecar.relative_path),
        br#"{"pk":"990011","code":"LEGACY"}"#,
    )
    .unwrap();

    let repeated = fixture.run(&item, true).await;

    assert_eq!(repeated.outcome.files_written, 0);
    assert!(repeated.resource_errors.is_empty());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "the existing reel media must be reused");
    let root = fixture.catalog.list_roots().unwrap().remove(0);
    let discovery = crate::scanner::discover_archive(root.id, &root.path).unwrap();
    let rescanned = discovery
        .groups
        .iter()
        .find(|group| group.remote_key == "post:990011")
        .expect("upgraded sidecar should preserve the stable post identity");
    assert_eq!(rescanned.item.kind, MediaItemKind::Reel);
    fixture.catalog.upsert_media(&rescanned.item).unwrap();
    let after = fixture.only_item();
    assert_eq!(after.id, before.id);
    assert_eq!(after.kind, MediaItemKind::Reel);
    assert_eq!(
        after
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        before
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>()
    );
}
