use std::path::PathBuf;

use insta_dl_gui_lib::catalog::{
    Catalog, CatalogError, CatalogFileInput, CatalogMediaInput, MediaFileKind, MediaItemKind,
};
use insta_dl_gui_lib::cdn::{self, CdnError};

#[test]
fn catalog_public_boundary_rejects_absolute_download_paths() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("archive");
    let catalog = Catalog::open(temp.path().join("catalog.sqlite3")).unwrap();
    let root = catalog.register_root(&archive, "Archive").unwrap();
    let absolute_path = archive.join("must-not-leak.jpg");
    let input = CatalogMediaInput {
        remote_key: "post:123".into(),
        kind: MediaItemKind::Post,
        remote_pk: Some("123".into()),
        shortcode: Some("SAFE".into()),
        owner_pk: None,
        owner_username: None,
        taken_at: None,
        caption: None,
        like_count: None,
        comment_count: None,
        imported_at: 1,
        updated_at: 1,
        files: vec![CatalogFileInput {
            root_id: root.id,
            relative_path: absolute_path.clone(),
            ordinal: 0,
            kind: MediaFileKind::Photo,
            byte_size: 8,
            mtime: 1,
            last_seen_at: 1,
        }],
        source_id: None,
    };

    assert!(matches!(
        catalog.upsert_media(&input),
        Err(CatalogError::InvalidRelativePath { path }) if path == absolute_path
    ));
}

#[tokio::test]
async fn release_downloader_keeps_loopback_outside_the_cdn_allowlist() {
    let temp = tempfile::tempdir().unwrap();
    let http = reqwest::Client::new();
    let result = cdn::stream_to_file_retried(
        &http,
        "https://127.0.0.1:9/private",
        &PathBuf::from(temp.path()).join("blocked"),
        None,
        |_| {},
        None,
        1,
    )
    .await;

    assert!(matches!(
        result,
        Err(CdnError::HostNotAllowed(host)) if host == "127.0.0.1"
    ));
    assert!(std::fs::read_dir(temp.path()).unwrap().next().is_none());
}
