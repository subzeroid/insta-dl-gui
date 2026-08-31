use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::catalog::{
    Catalog, FileAvailability, LibraryCard, LibraryFile, LibraryItemDetail, LibraryPage,
    LibraryQuery, LibraryRoot, MediaFileKind, MediaItemKind, ResolvedCatalogFile,
};
use crate::config::Config;
use crate::jobs::{ScanLease, ScanRegistry};
use crate::scanner::{run_scan, LibraryScanProgress};
use crate::AppState;

const FILE_UNAVAILABLE: &str = "Library file is unavailable";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryPageResponse {
    pub items: Vec<LibraryCardResponse>,
    pub next_cursor: Option<String>,
}

impl From<LibraryPage> for LibraryPageResponse {
    fn from(page: LibraryPage) -> Self {
        Self {
            items: page.items.into_iter().map(Into::into).collect(),
            next_cursor: page.next_cursor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryCardResponse {
    pub id: i64,
    pub kind: MediaItemKind,
    pub shortcode: Option<String>,
    pub owner_username: Option<String>,
    pub taken_at: Option<i64>,
    pub caption: Option<String>,
    pub imported_at: i64,
    pub updated_at: i64,
    pub preview_file_id: Option<i64>,
    pub preview_file_kind: Option<MediaFileKind>,
    pub resource_count: u32,
    pub availability: FileAvailability,
}

impl From<LibraryCard> for LibraryCardResponse {
    fn from(card: LibraryCard) -> Self {
        Self {
            id: card.id,
            kind: card.kind,
            shortcode: card.shortcode,
            owner_username: card.owner_username,
            taken_at: card.taken_at,
            caption: card.caption,
            imported_at: card.imported_at,
            updated_at: card.updated_at,
            preview_file_id: card.preview.as_ref().map(|preview| preview.file_id),
            preview_file_kind: card.preview.as_ref().map(|preview| preview.kind),
            resource_count: card.resource_count,
            availability: card.availability,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryItemDetailResponse {
    pub id: i64,
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
    pub files: Vec<LibraryFileResponse>,
    pub source_ids: Vec<i64>,
}

impl From<LibraryItemDetail> for LibraryItemDetailResponse {
    fn from(item: LibraryItemDetail) -> Self {
        Self {
            id: item.id,
            kind: item.kind,
            remote_pk: item.remote_pk,
            shortcode: item.shortcode,
            owner_pk: item.owner_pk,
            owner_username: item.owner_username,
            taken_at: item.taken_at,
            caption: item.caption,
            like_count: item.like_count,
            comment_count: item.comment_count,
            imported_at: item.imported_at,
            updated_at: item.updated_at,
            files: item.files.into_iter().map(Into::into).collect(),
            source_ids: item.source_ids,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryFileResponse {
    pub id: i64,
    pub ordinal: i64,
    pub kind: MediaFileKind,
    pub byte_size: i64,
    pub mtime: i64,
    pub exists_on_disk: bool,
    pub last_seen_at: i64,
}

impl From<LibraryFile> for LibraryFileResponse {
    fn from(file: LibraryFile) -> Self {
        Self {
            id: file.id,
            ordinal: file.ordinal,
            kind: file.kind,
            byte_size: file.byte_size,
            mtime: file.mtime,
            exists_on_disk: file.exists_on_disk,
            last_seen_at: file.last_seen_at,
        }
    }
}

pub trait FileAction: Send + Sync {
    fn open(&self, path: &Path) -> Result<(), String>;
    fn reveal(&self, path: &Path) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy, Default)]
struct SystemFileAction;

impl FileAction for SystemFileAction {
    fn open(&self, path: &Path) -> Result<(), String> {
        tauri_plugin_opener::open_path(path, None::<&str>).map_err(|error| error.to_string())
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        tauri_plugin_opener::reveal_item_in_dir(path).map_err(|error| error.to_string())
    }
}

pub struct LibraryFileActions<A: FileAction + ?Sized> {
    catalog: Catalog,
    action: Arc<A>,
}

impl<A: FileAction + ?Sized + 'static> LibraryFileActions<A> {
    pub fn new(catalog: Catalog, action: Arc<A>) -> Self {
        Self { catalog, action }
    }

    pub async fn open(&self, file_id: i64) -> Result<(), String> {
        self.execute(file_id, FileActionOperation::Open).await
    }

    pub async fn reveal(&self, file_id: i64) -> Result<(), String> {
        self.execute(file_id, FileActionOperation::Reveal).await
    }

    async fn execute(&self, file_id: i64, operation: FileActionOperation) -> Result<(), String> {
        let catalog = self.catalog.clone();
        let action = Arc::clone(&self.action);
        tauri::async_runtime::spawn_blocking(move || {
            let file = resolve_validated_catalog_file(&catalog, file_id)
                .map_err(|_| FILE_UNAVAILABLE.to_owned())?;
            match operation {
                FileActionOperation::Open => action
                    .open(&file.canonical_path)
                    .map_err(|_| "Could not open library file".to_owned()),
                FileActionOperation::Reveal => action
                    .reveal(&file.canonical_path)
                    .map_err(|_| "Could not reveal library file".to_owned()),
            }
        })
        .await
        .map_err(|_| operation.failure_message().to_owned())?
    }
}

#[derive(Debug, Clone, Copy)]
enum FileActionOperation {
    Open,
    Reveal,
}

impl FileActionOperation {
    const fn failure_message(self) -> &'static str {
        match self {
            Self::Open => "Could not open library file",
            Self::Reveal => "Could not reveal library file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedCatalogFile {
    pub canonical_path: PathBuf,
    pub relative_path: PathBuf,
    pub kind: MediaFileKind,
}

pub(crate) fn resolve_validated_catalog_file(
    catalog: &Catalog,
    file_id: i64,
) -> Result<ValidatedCatalogFile, ()> {
    if file_id <= 0 {
        return Err(());
    }
    let resolved = catalog.resolve_file(file_id).map_err(|_| ())?;
    validate_resolved_catalog_file(&resolved)
}

pub fn probe_library_preview_access(catalog: &Catalog, file_id: i64) -> bool {
    resolve_validated_catalog_file(catalog, file_id)
        .and_then(|file| std::fs::File::open(file.canonical_path).map_err(|_| ()))
        .is_ok()
}

fn validate_resolved_catalog_file(
    resolved: &ResolvedCatalogFile,
) -> Result<ValidatedCatalogFile, ()> {
    if !resolved.exists_on_disk
        || !resolved.root_path.is_absolute()
        || resolved.relative_path.as_os_str().is_empty()
        || resolved.relative_path.is_absolute()
        || resolved
            .relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(());
    }
    let root = resolved.root_path.canonicalize().map_err(|_| ())?;
    if !root.is_dir() {
        return Err(());
    }
    let file = root
        .join(&resolved.relative_path)
        .canonicalize()
        .map_err(|_| ())?;
    if !file.starts_with(&root) || !file.is_file() {
        return Err(());
    }
    Ok(ValidatedCatalogFile {
        canonical_path: file,
        relative_path: resolved.relative_path.clone(),
        kind: resolved.kind,
    })
}

async fn run_catalog<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, crate::catalog::CatalogError> + Send + 'static,
    public_error: &'static str,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| public_error.to_owned())?
        .map_err(|_| public_error.to_owned())
}

pub(crate) fn register_configured_library_root(
    catalog: &Catalog,
    config: &Config,
) -> Result<LibraryRoot, String> {
    let path = Path::new(&config.dest_dir);
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Downloads");
    catalog
        .register_root(path, label)
        .map_err(|error| error.to_string())
}

pub(crate) fn prepare_scan(
    catalog: &Catalog,
    registry: &Arc<ScanRegistry>,
    root_id: i64,
    scan_id: &str,
    scan_started_at: i64,
) -> Result<(LibraryRoot, ScanLease, i64), String> {
    let root = catalog
        .list_roots()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|root| root.id == root_id)
        .ok_or_else(|| format!("library root {root_id} was not found"))?;
    let scan_started_at = root
        .last_scan_started_at
        .map_or(scan_started_at, |previous| {
            scan_started_at.max(previous.saturating_add(1))
        });
    let lease = registry
        .try_register(root_id, scan_id)
        .map_err(|error| error.to_string())?;
    catalog
        .begin_scan(root_id, scan_started_at)
        .map_err(|error| error.to_string())?;
    Ok((root, lease, scan_started_at))
}

#[tauri::command]
pub async fn ensure_configured_library_root(
    state: State<'_, AppState>,
) -> Result<LibraryRoot, String> {
    let config = state.cfg.read().await.clone();
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        register_configured_library_root(&catalog, &config)
    })
    .await
    .map_err(|_| "Could not register library root".to_owned())?
    .map_err(|_| "Could not register library root".to_owned())
}

#[tauri::command]
pub async fn list_library_roots(state: State<'_, AppState>) -> Result<Vec<LibraryRoot>, String> {
    let catalog = state.catalog.clone();
    run_catalog(move || catalog.list_roots(), "Could not list library roots").await
}

#[tauri::command]
pub async fn query_library(
    query: LibraryQuery,
    state: State<'_, AppState>,
) -> Result<LibraryPageResponse, String> {
    let catalog = state.catalog.clone();
    run_catalog(
        move || catalog.query_library(&query),
        "Could not query library",
    )
    .await
    .map(Into::into)
}

#[tauri::command]
pub async fn get_library_item(
    id: i64,
    state: State<'_, AppState>,
) -> Result<LibraryItemDetailResponse, String> {
    let catalog = state.catalog.clone();
    let item = run_catalog(
        move || catalog.get_library_item(id),
        "Could not load library item",
    )
    .await?;
    item.map(Into::into)
        .ok_or_else(|| "Library item was not found".to_owned())
}

#[tauri::command]
pub async fn request_library_preview_access(
    file_id: i64,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || probe_library_preview_access(&catalog, file_id))
        .await
        .map_err(|_| "Could not check library preview access".to_owned())
}

#[tauri::command]
pub async fn open_library_file(file_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    LibraryFileActions::new(state.catalog.clone(), Arc::new(SystemFileAction))
        .open(file_id)
        .await
}

#[tauri::command]
pub async fn reveal_library_file(file_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    LibraryFileActions::new(state.catalog.clone(), Arc::new(SystemFileAction))
        .reveal(file_id)
        .await
}

#[tauri::command]
pub async fn start_library_scan(
    root_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let scan_id = Uuid::new_v4().to_string();
    let catalog = state.catalog.clone();
    let scans = Arc::clone(&state.scans);
    let prepare_scan_id = scan_id.clone();
    let (root, lease, scan_started_at) = tauri::async_runtime::spawn_blocking(move || {
        prepare_scan(&catalog, &scans, root_id, &prepare_scan_id, unix_now())
    })
    .await
    .map_err(|_| "Could not start library scan".to_owned())?
    .map_err(|_| "Could not start library scan".to_owned())?;
    let catalog = state.catalog.clone();
    let worker_scan_id = scan_id.clone();
    let cancellation = lease.cancellation().clone();
    let worker_app = app.clone();
    let worker = tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        run_scan(
            &catalog,
            &root,
            &worker_scan_id,
            scan_started_at,
            &cancellation,
            |event: &LibraryScanProgress| {
                if worker_app.emit("library-scan-progress", event).is_err() {
                    eprintln!("failed to deliver library scan progress event");
                }
                Ok(())
            },
        );
    });
    let monitor_scan_id = scan_id.clone();
    tauri::async_runtime::spawn(async move {
        monitor_scan_worker(worker, &monitor_scan_id, root_id, |event| {
            if app.emit("library-scan-progress", event).is_err() {
                eprintln!("failed to deliver library scan worker failure event");
            }
        })
        .await;
    });
    Ok(scan_id)
}

async fn monitor_scan_worker<Worker, WorkerError>(
    worker: Worker,
    scan_id: &str,
    root_id: i64,
    mut emit: impl FnMut(&LibraryScanProgress),
) where
    Worker: Future<Output = Result<(), WorkerError>>,
{
    if worker.await.is_err() {
        emit(&LibraryScanProgress::Failed {
            scan_id: scan_id.to_owned(),
            root_id,
            error: "library scan worker stopped unexpectedly".into(),
        });
    }
}

#[tauri::command]
pub fn cancel_library_scan(scan_id: String, state: State<'_, AppState>) -> bool {
    state.scans.cancel(&scan_id)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::tempdir;

    use super::{monitor_scan_worker, prepare_scan, register_configured_library_root};
    use crate::catalog::Catalog;
    use crate::config::Config;
    use crate::jobs::ScanRegistry;

    #[test]
    fn prepare_scan_validates_root_then_registers_and_records_start() {
        let directory = tempdir().unwrap();
        let archive = tempdir().unwrap();
        let catalog = Catalog::open(directory.path().join("catalog.sqlite3")).unwrap();
        let root = catalog.register_root(archive.path(), "Archive").unwrap();
        let registry = Arc::new(ScanRegistry::new());

        let unknown = prepare_scan(&catalog, &registry, root.id + 99, "unknown", 10);
        assert!(unknown.is_err());
        assert_eq!(registry.active_len(), 0);
        assert_eq!(catalog.list_roots().unwrap()[0].last_scan_started_at, None);

        let (prepared_root, lease, effective_started_at) =
            prepare_scan(&catalog, &registry, root.id, "valid", 20).unwrap();
        assert_eq!(prepared_root.id, root.id);
        assert_eq!(lease.scan_id(), "valid");
        assert_eq!(effective_started_at, 20);
        assert_eq!(registry.active_len(), 1);
        assert_eq!(
            catalog.list_roots().unwrap()[0].last_scan_started_at,
            Some(20)
        );
        assert!(prepare_scan(&catalog, &registry, root.id, "duplicate", 30).is_err());
        assert_eq!(
            catalog.list_roots().unwrap()[0].last_scan_started_at,
            Some(20),
            "a rejected duplicate must not overwrite the active scan timestamp"
        );
        drop(lease);
        assert_eq!(registry.active_len(), 0);

        let (_, second_lease, second_started_at) =
            prepare_scan(&catalog, &registry, root.id, "same-second", 20).unwrap();
        assert_eq!(second_started_at, 21);
        assert_eq!(
            catalog.list_roots().unwrap()[0].last_scan_started_at,
            Some(21),
            "scan markers must advance even when two scans start in one wall-clock second"
        );
        drop(second_lease);
    }

    #[test]
    fn configured_destination_registers_new_root_without_rewriting_old_one() {
        let directory = tempdir().unwrap();
        let first_archive = tempdir().unwrap();
        let second_archive = tempdir().unwrap();
        let catalog = Catalog::open(directory.path().join("catalog.sqlite3")).unwrap();
        let mut config = Config {
            token: None,
            dest_dir: first_archive.path().to_string_lossy().into_owned(),
            sidecar: true,
            proxy_url: None,
        };

        let first = register_configured_library_root(&catalog, &config).unwrap();
        config.dest_dir = second_archive.path().to_string_lossy().into_owned();
        let second = register_configured_library_root(&catalog, &config).unwrap();

        assert_ne!(first.id, second.id);
        assert_eq!(catalog.list_roots().unwrap().len(), 2);
        assert_eq!(first.path, first_archive.path().canonicalize().unwrap());
        assert_eq!(second.path, second_archive.path().canonicalize().unwrap());
    }

    #[tokio::test]
    async fn panic_worker_cleans_registry_and_monitor_emits_one_sanitized_failure() {
        let registry = Arc::new(ScanRegistry::new());
        let lease = registry.try_register(77, "panic-scan").unwrap();
        let worker = tokio::task::spawn_blocking(move || {
            let _lease = lease;
            panic!("secret panic payload at /private/archive/file.jpg");
        });
        let mut events = Vec::new();

        monitor_scan_worker(worker, "panic-scan", 77, |event| {
            events.push(event.clone());
        })
        .await;

        assert_eq!(registry.active_len(), 0);
        assert_eq!(events.len(), 1);
        let crate::scanner::LibraryScanProgress::Failed {
            scan_id,
            root_id,
            error,
        } = &events[0]
        else {
            panic!("panic monitor must deliver Failed");
        };
        assert_eq!(scan_id, "panic-scan");
        assert_eq!(*root_id, 77);
        assert_eq!(error, "library scan worker stopped unexpectedly");
        assert!(!error.contains("/private/archive"));
        assert!(!error.contains("secret panic payload"));

        events.clear();
        monitor_scan_worker(
            std::future::ready(Ok::<(), ()>(())),
            "normal-scan",
            78,
            |event| events.push(event.clone()),
        )
        .await;
        assert!(
            events.is_empty(),
            "normal completion must not be duplicated"
        );
    }
}
