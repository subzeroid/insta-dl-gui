use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::catalog::{Catalog, LibraryRoot};
use crate::config::Config;
use crate::jobs::{ScanLease, ScanRegistry};
use crate::scanner::{run_scan, LibraryScanProgress};
use crate::AppState;

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
    register_configured_library_root(&state.catalog, &config)
}

#[tauri::command]
pub async fn start_library_scan(
    root_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let scan_id = Uuid::new_v4().to_string();
    let (root, lease, scan_started_at) =
        prepare_scan(&state.catalog, &state.scans, root_id, &scan_id, unix_now())?;
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
