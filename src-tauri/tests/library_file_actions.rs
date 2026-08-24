use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use insta_dl_gui_lib::catalog::{
    Catalog, CatalogFileInput, CatalogMediaInput, LibraryQuery, LibrarySort, MediaFileKind,
    MediaItemKind,
};
use insta_dl_gui_lib::library_commands::{
    FileAction, LibraryFileActions, LibraryItemDetailResponse, LibraryPageResponse,
};
use tempfile::TempDir;

const UNAVAILABLE: &str = "Library file is unavailable";

#[derive(Clone, Default)]
struct RecordingFileAction {
    opened: Arc<Mutex<Vec<PathBuf>>>,
    revealed: Arc<Mutex<Vec<PathBuf>>>,
}

impl FileAction for RecordingFileAction {
    fn open(&self, path: &Path) -> Result<(), String> {
        self.opened.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        self.revealed.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}

struct Fixture {
    _temp: TempDir,
    database_path: PathBuf,
    root_path: PathBuf,
    catalog: Catalog,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let database_path = temp.path().join("catalog.sqlite3");
        let root_path = temp.path().join("archive");
        std::fs::create_dir(&root_path).unwrap();
        let catalog = Catalog::open(&database_path).unwrap();
        catalog.register_root(&root_path, "Archive").unwrap();
        Self {
            _temp: temp,
            database_path,
            root_path,
            catalog,
        }
    }

    fn add_file(&self, relative_path: &str, contents: Option<&[u8]>) -> i64 {
        let root = self.catalog.list_roots().unwrap().remove(0);
        if let Some(contents) = contents {
            let path = self.root_path.join(relative_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }
        self.catalog
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
                    root_id: root.id,
                    relative_path: relative_path.into(),
                    ordinal: 0,
                    kind: MediaFileKind::Photo,
                    byte_size: contents.map_or(0, |bytes| bytes.len() as i64),
                    mtime: 10,
                    last_seen_at: 10,
                }],
                source_id: None,
            })
            .unwrap()
            .file_ids[0]
    }

    fn service(&self, action: RecordingFileAction) -> LibraryFileActions<RecordingFileAction> {
        LibraryFileActions::new(self.catalog.clone(), Arc::new(action))
    }
}

#[tokio::test]
async fn valid_catalog_file_passes_only_its_canonical_path_to_open_and_reveal() {
    let fixture = Fixture::new();
    let file_id = fixture.add_file("posts/photo.jpg", Some(b"photo"));
    let action = RecordingFileAction::default();
    let service = fixture.service(action.clone());

    service.open(file_id).await.unwrap();
    service.reveal(file_id).await.unwrap();

    let expected = fixture
        .root_path
        .join("posts/photo.jpg")
        .canonicalize()
        .unwrap();
    assert_eq!(*action.opened.lock().unwrap(), vec![expected.clone()]);
    assert_eq!(*action.revealed.lock().unwrap(), vec![expected]);
}

#[tokio::test]
async fn unavailable_catalog_row_is_rejected_even_when_file_still_exists() {
    let fixture = Fixture::new();
    let file_id = fixture.add_file("missing.jpg", Some(b"still here"));
    rusqlite::Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE media_files SET exists_on_disk = 0 WHERE id = ?1",
            [file_id],
        )
        .unwrap();
    let action = RecordingFileAction::default();

    assert_eq!(
        fixture.service(action.clone()).open(file_id).await,
        Err(UNAVAILABLE.into())
    );
    assert!(action.opened.lock().unwrap().is_empty());
}

#[tokio::test]
async fn missing_file_is_rejected_without_launching_an_application() {
    let fixture = Fixture::new();
    let file_id = fixture.add_file("missing.jpg", None);
    let action = RecordingFileAction::default();

    assert_eq!(
        fixture.service(action.clone()).open(file_id).await,
        Err(UNAVAILABLE.into())
    );
    assert!(action.opened.lock().unwrap().is_empty());
}

#[tokio::test]
async fn tampered_parent_path_is_rejected_without_launching_an_application() {
    let fixture = Fixture::new();
    let file_id = fixture.add_file("safe.jpg", Some(b"safe"));
    let outside = fixture.root_path.parent().unwrap().join("outside.jpg");
    std::fs::write(&outside, b"outside").unwrap();
    rusqlite::Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE media_files SET relative_path = '../outside.jpg' WHERE id = ?1",
            [file_id],
        )
        .unwrap();
    let action = RecordingFileAction::default();

    assert_eq!(
        fixture.service(action.clone()).reveal(file_id).await,
        Err(UNAVAILABLE.into())
    );
    assert!(action.revealed.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_escaping_the_registered_root_is_rejected() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let outside = fixture.root_path.parent().unwrap().join("private.jpg");
    std::fs::write(&outside, b"private").unwrap();
    symlink(&outside, fixture.root_path.join("link.jpg")).unwrap();
    let file_id = fixture.add_file("link.jpg", None);
    let action = RecordingFileAction::default();

    assert_eq!(
        fixture.service(action.clone()).open(file_id).await,
        Err(UNAVAILABLE.into())
    );
    assert!(action.opened.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_file_id_is_rejected_without_exposing_catalog_internals() {
    let fixture = Fixture::new();
    let action = RecordingFileAction::default();

    let error = fixture
        .service(action.clone())
        .open(9_999)
        .await
        .unwrap_err();

    assert_eq!(error, UNAVAILABLE);
    assert!(!error.contains("SQLite"));
    assert!(!error.contains(&fixture.database_path.to_string_lossy().to_string()));
    assert!(action.opened.lock().unwrap().is_empty());
}

#[derive(Clone, Default)]
struct FailingFileAction;

impl FileAction for FailingFileAction {
    fn open(&self, _path: &Path) -> Result<(), String> {
        Err("opener failed for /private/archive/secret.jpg".into())
    }

    fn reveal(&self, _path: &Path) -> Result<(), String> {
        Err("explorer failed for /private/archive/secret.jpg".into())
    }
}

#[tokio::test]
async fn application_launch_errors_are_sanitized() {
    let fixture = Fixture::new();
    let file_id = fixture.add_file("photo.jpg", Some(b"photo"));
    let service = LibraryFileActions::new(fixture.catalog.clone(), Arc::new(FailingFileAction));

    assert_eq!(
        service.open(file_id).await,
        Err("Could not open library file".into())
    );
    assert_eq!(
        service.reveal(file_id).await,
        Err("Could not reveal library file".into())
    );
}

#[test]
fn ipc_library_dtos_never_serialize_catalog_paths_or_root_ids() {
    let fixture = Fixture::new();
    let file_id = fixture.add_file("private/archive/photo.jpg", Some(b"photo"));
    let page = fixture
        .catalog
        .query_library(&LibraryQuery {
            search: None,
            kinds: Vec::new(),
            source_id: None,
            availability: None,
            taken_after: None,
            taken_before: None,
            sort: LibrarySort::TakenAtDesc,
            cursor: None,
            limit: 60,
        })
        .unwrap();
    let item_id = page.items[0].id;

    let safe_page = LibraryPageResponse::from(page);
    let page_json = serde_json::to_value(safe_page).unwrap();
    assert_eq!(page_json["items"][0]["preview_file_id"], file_id);
    let page_text = page_json.to_string();
    assert!(!page_text.contains("remote_key"));
    assert!(!page_text.contains("relative_path"));
    assert!(!page_text.contains("root_path"));
    assert!(!page_text.contains("root_id"));
    assert!(!page_text.contains("private/archive"));
    assert!(!page_text.contains(&fixture.root_path.to_string_lossy().to_string()));

    let detail = fixture.catalog.get_library_item(item_id).unwrap().unwrap();
    let detail_json = serde_json::to_value(LibraryItemDetailResponse::from(detail)).unwrap();
    assert_eq!(detail_json["files"][0]["id"], file_id);
    let detail_text = detail_json.to_string();
    assert!(!detail_text.contains("remote_key"));
    assert!(!detail_text.contains("relative_path"));
    assert!(!detail_text.contains("root_path"));
    assert!(!detail_text.contains("root_id"));
    assert!(!detail_text.contains("private/archive"));
    assert!(!detail_text.contains(&fixture.root_path.to_string_lossy().to_string()));
}
