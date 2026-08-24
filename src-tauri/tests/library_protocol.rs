use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

use insta_dl_gui_lib::catalog::{
    Catalog, CatalogFileInput, CatalogMediaInput, MediaFileKind, MediaItemKind,
};
use insta_dl_gui_lib::library_protocol::{
    handle_library_protocol, read_bounded, MAX_PROTOCOL_BODY_BYTES,
};
use tauri::http::{header, HeaderValue, Method, Request, StatusCode};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    database_path: PathBuf,
    root_path: PathBuf,
    catalog: Catalog,
    photo_id: i64,
    video_id: i64,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let database_path = temp.path().join("catalog.sqlite3");
        let root_path = temp.path().join("archive");
        std::fs::create_dir(&root_path).unwrap();
        let catalog = Catalog::open(&database_path).unwrap();
        let root = catalog.register_root(&root_path, "Archive").unwrap();
        let photo_id = add_file(
            &catalog,
            root.id,
            &root_path,
            "photo.jpg",
            MediaFileKind::Photo,
            b"photo-body",
        );
        let video_id = add_file(
            &catalog,
            root.id,
            &root_path,
            "video.mp4",
            MediaFileKind::Video,
            b"0123456789",
        );
        Self {
            _temp: temp,
            database_path,
            root_path,
            catalog,
            photo_id,
            video_id,
        }
    }

    fn request(&self, method: Method, file_id: i64) -> Request<Vec<u8>> {
        Request::builder()
            .method(method)
            .uri(format!("library://localhost/media/{file_id}"))
            .body(Vec::new())
            .unwrap()
    }
}

fn add_file(
    catalog: &Catalog,
    root_id: i64,
    root_path: &std::path::Path,
    relative_path: &str,
    kind: MediaFileKind,
    bytes: &[u8],
) -> i64 {
    std::fs::write(root_path.join(relative_path), bytes).unwrap();
    catalog_file(catalog, root_id, relative_path, kind, bytes.len() as i64)
}

fn catalog_file(
    catalog: &Catalog,
    root_id: i64,
    relative_path: &str,
    kind: MediaFileKind,
    byte_size: i64,
) -> i64 {
    catalog
        .upsert_media(&CatalogMediaInput {
            remote_key: format!("post:{relative_path}"),
            kind: MediaItemKind::Post,
            remote_pk: None,
            shortcode: None,
            owner_pk: None,
            owner_username: None,
            taken_at: Some(10),
            caption: None,
            like_count: None,
            comment_count: None,
            imported_at: 10,
            updated_at: 10,
            files: vec![CatalogFileInput {
                root_id,
                relative_path: relative_path.into(),
                ordinal: 0,
                kind,
                byte_size,
                mtime: 10,
                last_seen_at: 10,
            }],
            source_id: None,
        })
        .unwrap()
        .file_ids[0]
}

#[tokio::test]
async fn photo_get_returns_bytes_mime_and_non_cacheable_range_headers() {
    let fixture = Fixture::new();

    let response = handle_library_protocol(
        fixture.catalog.clone(),
        "main",
        fixture.request(Method::GET, fixture.photo_id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), b"photo-body");
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "10");
    assert_eq!(response.headers()[header::ACCEPT_RANGES], "bytes");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert_eq!(
        response.headers()[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; sandbox"
    );
}

#[tokio::test]
async fn photo_head_returns_no_body_and_the_get_content_length() {
    let fixture = Fixture::new();

    let response = handle_library_protocol(
        fixture.catalog.clone(),
        "main",
        fixture.request(Method::HEAD, fixture.photo_id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.body().is_empty());
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "10");
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert_eq!(
        response.headers()[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; sandbox"
    );
}

#[tokio::test]
async fn bounded_video_range_reads_only_requested_bytes() {
    let fixture = Fixture::new();
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("library://localhost/media/{}", fixture.video_id))
        .header(header::RANGE, "bytes=2-5")
        .body(Vec::new())
        .unwrap();

    let response = handle_library_protocol(fixture.catalog.clone(), "main", request).await;

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.body(), b"2345");
    assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "4");
    assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert_eq!(
        response.headers()[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; sandbox"
    );
}

#[tokio::test]
async fn allowlisted_photo_and_video_extensions_use_static_mime_types() {
    let fixture = Fixture::new();
    let root_id = fixture.catalog.list_roots().unwrap()[0].id;
    for (name, kind, expected_mime) in [
        ("photo.jpeg", MediaFileKind::Photo, "image/jpeg"),
        ("photo.png", MediaFileKind::Photo, "image/png"),
        ("photo.webp", MediaFileKind::Photo, "image/webp"),
        ("video.mov", MediaFileKind::Video, "video/quicktime"),
    ] {
        let file_id = add_file(
            &fixture.catalog,
            root_id,
            &fixture.root_path,
            name,
            kind,
            b"safe preview",
        );
        let response = handle_library_protocol(
            fixture.catalog.clone(),
            "main",
            fixture.request(Method::GET, file_id),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "{name}");
        assert_eq!(response.headers()[header::CONTENT_TYPE], expected_mime);
        assert_eq!(response.body(), b"safe preview");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn internal_photo_symlink_to_html_is_not_served_as_active_content() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let root_id = fixture.catalog.list_roots().unwrap()[0].id;
    let html = b"<html><script>steal()</script></html>";
    std::fs::write(fixture.root_path.join("payload.html"), html).unwrap();
    symlink("payload.html", fixture.root_path.join("linked-photo.jpg")).unwrap();
    let file_id = catalog_file(
        &fixture.catalog,
        root_id,
        "linked-photo.jpg",
        MediaFileKind::Photo,
        html.len() as i64,
    );

    let response = handle_library_protocol(
        fixture.catalog.clone(),
        "main",
        fixture.request(Method::GET, file_id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.body(), b"not found");
    assert!(!String::from_utf8_lossy(response.body()).contains("<html"));
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert_eq!(
        response.headers()[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; sandbox"
    );
}

#[tokio::test]
async fn metadata_unknown_and_catalog_kind_extension_mismatches_are_not_previewable() {
    let fixture = Fixture::new();
    let root_id = fixture.catalog.list_roots().unwrap()[0].id;
    let ids = [
        add_file(
            &fixture.catalog,
            root_id,
            &fixture.root_path,
            "metadata.json",
            MediaFileKind::Metadata,
            br#"{"secret":true}"#,
        ),
        add_file(
            &fixture.catalog,
            root_id,
            &fixture.root_path,
            "unknown.bin",
            MediaFileKind::Unknown,
            b"unknown",
        ),
        add_file(
            &fixture.catalog,
            root_id,
            &fixture.root_path,
            "declared-photo.mp4",
            MediaFileKind::Photo,
            b"not a photo",
        ),
    ];

    for file_id in ids {
        let response = handle_library_protocol(
            fixture.catalog.clone(),
            "main",
            fixture.request(Method::GET, file_id),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.body(), b"not found");
        assert!(!String::from_utf8_lossy(response.body()).contains("secret"));
    }
}

#[tokio::test]
async fn open_ended_and_suffix_ranges_follow_rfc_7233() {
    let fixture = Fixture::new();
    for (range, expected, content_range) in [
        ("bytes=6-", b"6789".as_slice(), "bytes 6-9/10"),
        ("bytes=-3", b"789".as_slice(), "bytes 7-9/10"),
        ("bytes=-99", b"0123456789".as_slice(), "bytes 0-9/10"),
    ] {
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("library://localhost/media/{}", fixture.video_id))
            .header(header::RANGE, range)
            .body(Vec::new())
            .unwrap();
        let response = handle_library_protocol(fixture.catalog.clone(), "main", request).await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT, "{range}");
        assert_eq!(response.body(), expected, "{range}");
        assert_eq!(response.headers()[header::CONTENT_RANGE], content_range);
    }
}

#[tokio::test]
async fn invalid_multiple_and_unsatisfiable_ranges_return_sanitized_416() {
    let fixture = Fixture::new();
    for range in [
        "items=0-1",
        "bytes=",
        "bytes=x-2",
        "bytes=5-2",
        "bytes=10-",
        "bytes=-0",
        "bytes=0-1,4-5",
        "bytes=18446744073709551616-",
    ] {
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("library://localhost/media/{}", fixture.video_id))
            .header(header::RANGE, range)
            .body(Vec::new())
            .unwrap();
        let response = handle_library_protocol(fixture.catalog.clone(), "main", request).await;
        assert_eq!(
            response.status(),
            StatusCode::RANGE_NOT_SATISFIABLE,
            "{range}"
        );
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */10");
        assert_eq!(response.body(), b"range not satisfiable");
    }

    let mut multiple = fixture.request(Method::GET, fixture.video_id);
    multiple
        .headers_mut()
        .append(header::RANGE, HeaderValue::from_static("bytes=0-1"));
    multiple
        .headers_mut()
        .append(header::RANGE, HeaderValue::from_static("bytes=4-5"));
    let response = handle_library_protocol(fixture.catalog.clone(), "main", multiple).await;
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */10");
}

#[tokio::test]
async fn head_ignores_valid_range_and_returns_full_representation_headers() {
    let fixture = Fixture::new();
    let request = Request::builder()
        .method(Method::HEAD)
        .uri(format!("library://localhost/media/{}", fixture.video_id))
        .header(header::RANGE, "bytes=2-5")
        .body(Vec::new())
        .unwrap();

    let response = handle_library_protocol(fixture.catalog.clone(), "main", request).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.body().is_empty());
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "10");
    assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert_eq!(
        response.headers()[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; sandbox"
    );
    assert!(!response.headers().contains_key(header::CONTENT_RANGE));
}

#[tokio::test]
async fn head_ignores_invalid_and_multiple_ranges_but_keeps_error_bodies_empty() {
    let fixture = Fixture::new();
    let missing = handle_library_protocol(
        fixture.catalog.clone(),
        "main",
        fixture.request(Method::HEAD, 99_999),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert!(missing.body().is_empty());
    assert_eq!(missing.headers()[header::CONTENT_LENGTH], "9");

    let invalid_range = Request::builder()
        .method(Method::HEAD)
        .uri(format!("library://localhost/media/{}", fixture.video_id))
        .header(header::RANGE, "bytes=99-")
        .body(Vec::new())
        .unwrap();
    let invalid_range =
        handle_library_protocol(fixture.catalog.clone(), "main", invalid_range).await;
    assert_eq!(invalid_range.status(), StatusCode::OK);
    assert!(invalid_range.body().is_empty());
    assert_eq!(invalid_range.headers()[header::CONTENT_LENGTH], "10");
    assert!(!invalid_range.headers().contains_key(header::CONTENT_RANGE));

    let mut multiple = fixture.request(Method::HEAD, fixture.video_id);
    multiple
        .headers_mut()
        .append(header::RANGE, HeaderValue::from_static("bytes=0-1"));
    multiple
        .headers_mut()
        .append(header::RANGE, HeaderValue::from_static("bytes=4-5"));
    let multiple = handle_library_protocol(fixture.catalog.clone(), "main", multiple).await;
    assert_eq!(multiple.status(), StatusCode::OK);
    assert!(multiple.body().is_empty());
    assert_eq!(multiple.headers()[header::CONTENT_LENGTH], "10");
    assert!(!multiple.headers().contains_key(header::CONTENT_RANGE));

    let non_main = handle_library_protocol(
        fixture.catalog.clone(),
        "secondary",
        fixture.request(Method::HEAD, fixture.photo_id),
    )
    .await;
    assert_eq!(non_main.status(), StatusCode::NOT_FOUND);
    assert!(non_main.body().is_empty());
}

#[tokio::test]
async fn oversized_full_and_range_gets_are_rejected_before_body_materialization() {
    let fixture = Fixture::new();
    let root_id = fixture.catalog.list_roots().unwrap()[0].id;
    let oversized_id = add_file(
        &fixture.catalog,
        root_id,
        &fixture.root_path,
        "oversized.mp4",
        MediaFileKind::Video,
        b"",
    );
    let oversized_length = MAX_PROTOCOL_BODY_BYTES + 1;
    std::fs::OpenOptions::new()
        .write(true)
        .open(fixture.root_path.join("oversized.mp4"))
        .unwrap()
        .set_len(oversized_length)
        .unwrap();

    let full = handle_library_protocol(
        fixture.catalog.clone(),
        "main",
        fixture.request(Method::GET, oversized_id),
    )
    .await;
    assert_eq!(full.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        full.headers()[header::CONTENT_RANGE],
        format!("bytes */{oversized_length}")
    );
    assert_eq!(full.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(full.body(), b"range not satisfiable");

    let oversized_range = Request::builder()
        .method(Method::GET)
        .uri(format!("library://localhost/media/{oversized_id}"))
        .header(
            header::RANGE,
            format!("bytes=0-{}", MAX_PROTOCOL_BODY_BYTES),
        )
        .body(Vec::new())
        .unwrap();
    let oversized_range =
        handle_library_protocol(fixture.catalog.clone(), "main", oversized_range).await;
    assert_eq!(oversized_range.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        oversized_range.headers()[header::CONTENT_RANGE],
        format!("bytes */{oversized_length}")
    );

    let head = handle_library_protocol(
        fixture.catalog.clone(),
        "main",
        fixture.request(Method::HEAD, oversized_id),
    )
    .await;
    assert_eq!(head.status(), StatusCode::OK);
    assert!(head.body().is_empty());
    assert_eq!(
        head.headers()[header::CONTENT_LENGTH],
        oversized_length.to_string()
    );
}

#[tokio::test]
async fn unknown_missing_and_tampered_catalog_files_are_404_without_path_disclosure() {
    let fixture = Fixture::new();
    let missing_id = add_file(
        &fixture.catalog,
        fixture.catalog.list_roots().unwrap()[0].id,
        &fixture.root_path,
        "missing.jpg",
        MediaFileKind::Photo,
        b"temporary",
    );
    std::fs::remove_file(fixture.root_path.join("missing.jpg")).unwrap();
    let tampered_id = add_file(
        &fixture.catalog,
        fixture.catalog.list_roots().unwrap()[0].id,
        &fixture.root_path,
        "tampered.jpg",
        MediaFileKind::Photo,
        b"temporary",
    );
    rusqlite::Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE media_files SET relative_path = '../secret.jpg' WHERE id = ?1",
            [tampered_id],
        )
        .unwrap();

    for file_id in [99_999, missing_id, tampered_id] {
        let response = handle_library_protocol(
            fixture.catalog.clone(),
            "main",
            fixture.request(Method::GET, file_id),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.body(), b"not found");
        let body = String::from_utf8_lossy(response.body());
        assert!(!body.contains("SQLite"));
        assert!(!body.contains("secret.jpg"));
        assert!(!body.contains(&fixture.root_path.to_string_lossy().to_string()));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn escaping_symlink_is_404() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let outside = fixture.root_path.parent().unwrap().join("outside.jpg");
    std::fs::write(&outside, b"outside").unwrap();
    symlink(&outside, fixture.root_path.join("link.jpg")).unwrap();
    let root_id = fixture.catalog.list_roots().unwrap()[0].id;
    let file_id = fixture
        .catalog
        .upsert_media(&CatalogMediaInput {
            remote_key: "post:link".into(),
            kind: MediaItemKind::Post,
            remote_pk: None,
            shortcode: None,
            owner_pk: None,
            owner_username: None,
            taken_at: None,
            caption: None,
            like_count: None,
            comment_count: None,
            imported_at: 10,
            updated_at: 10,
            files: vec![CatalogFileInput {
                root_id,
                relative_path: "link.jpg".into(),
                ordinal: 0,
                kind: MediaFileKind::Photo,
                byte_size: 7,
                mtime: 10,
                last_seen_at: 10,
            }],
            source_id: None,
        })
        .unwrap()
        .file_ids[0];

    let response = handle_library_protocol(
        fixture.catalog.clone(),
        "main",
        fixture.request(Method::GET, file_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_main_webview_strict_media_path_and_get_or_head_are_accepted() {
    let fixture = Fixture::new();

    let non_main = handle_library_protocol(
        fixture.catalog.clone(),
        "settings",
        fixture.request(Method::GET, fixture.photo_id),
    )
    .await;
    assert_eq!(non_main.status(), StatusCode::NOT_FOUND);

    for uri in [
        format!(
            "library://localhost/media/{}?path=/etc/passwd",
            fixture.photo_id
        ),
        format!("library://localhost/media/{}/extra", fixture.photo_id),
        "library://localhost/media/0".into(),
        "library://localhost/media/-1".into(),
        format!("library://localhost/media/+{}", fixture.photo_id),
        format!("library://localhost/media/0{}", fixture.photo_id),
        "library://localhost/media/%31".into(),
        "library://localhost/private/1".into(),
    ] {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Vec::new())
            .unwrap();
        let response = handle_library_protocol(fixture.catalog.clone(), "main", request).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    let request = fixture.request(Method::POST, fixture.photo_id);
    let response = handle_library_protocol(fixture.catalog.clone(), "main", request).await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(response.headers()[header::ALLOW], "GET, HEAD");
}

#[tokio::test]
async fn protocol_rejects_every_noncanonical_scheme_or_authority() {
    let fixture = Fixture::new();
    for uri in [
        format!("library://evil.example/media/{}", fixture.photo_id),
        format!("library://localhost:443/media/{}", fixture.photo_id),
        format!("library://user@localhost/media/{}", fixture.photo_id),
        format!("http://localhost/media/{}", fixture.photo_id),
        format!("http://library.localhost/media/{}", fixture.photo_id),
        format!("/media/{}", fixture.photo_id),
    ] {
        let request = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .body(Vec::new())
            .unwrap();
        let response = handle_library_protocol(fixture.catalog.clone(), "main", request).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(response.body(), b"not found", "{uri}");
    }
}

struct CountingReader {
    inner: Cursor<Vec<u8>>,
    bytes_read: usize,
    seeks: usize,
}

impl Read for CountingReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.bytes_read += read;
        Ok(read)
    }
}

impl Seek for CountingReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.seeks += 1;
        self.inner.seek(position)
    }
}

#[test]
fn bounded_reader_never_reads_past_the_requested_range() {
    let mut reader = CountingReader {
        inner: Cursor::new((0_u8..100).collect()),
        bytes_read: 0,
        seeks: 0,
    };

    let body = read_bounded(&mut reader, 20, 4).unwrap();

    assert_eq!(body, vec![20, 21, 22, 23]);
    assert_eq!(reader.bytes_read, 4);
    assert_eq!(reader.seeks, 1);
}

#[test]
fn bounded_reader_rejects_oversized_requests_before_seek_or_read() {
    let mut reader = CountingReader {
        inner: Cursor::new(vec![0; 16]),
        bytes_read: 0,
        seeks: 0,
    };

    assert!(read_bounded(&mut reader, 0, MAX_PROTOCOL_BODY_BYTES + 1).is_err());
    assert_eq!(reader.bytes_read, 0);
    assert_eq!(reader.seeks, 0);
}
