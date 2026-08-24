pub mod parser;
pub mod walk;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use thiserror::Error;

use crate::catalog::{Catalog, CatalogError, LibraryRoot, UpsertDisposition};

pub use crate::jobs::ScanCancellation;
pub use parser::DiscoveredGroup;

const SCAN_BATCH_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ScanSummary {
    pub imported: u64,
    pub updated: u64,
    pub missing: u64,
    pub warnings: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum LibraryScanProgress {
    Scanning {
        scan_id: String,
        root_id: i64,
        discovered: u64,
        processed: u64,
        warnings: u64,
    },
    Done {
        scan_id: String,
        root_id: i64,
        summary: ScanSummary,
    },
    Failed {
        scan_id: String,
        root_id: i64,
        error: String,
    },
    Cancelled {
        scan_id: String,
        root_id: i64,
        summary: ScanSummary,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub relative_path: PathBuf,
    pub byte_size: i64,
    pub mtime: i64,
    pub ordinal: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanWarning {
    pub relative_path: Option<PathBuf>,
    pub message: String,
}

#[derive(Debug)]
pub struct ArchiveDiscovery {
    pub groups: Vec<DiscoveredGroup>,
    pub warnings: Vec<ScanWarning>,
    pub complete: bool,
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("archive discovery was cancelled")]
    Cancelled,
    #[error("library root is not a directory: {path}")]
    InvalidRoot { path: PathBuf },
    #[error("could not {operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn discover_archive(root_id: i64, root: &Path) -> Result<ArchiveDiscovery, ScanError> {
    discover_archive_with_cancel(root_id, root, || false)
}

fn discover_archive_with_cancel(
    root_id: i64,
    root: &Path,
    mut should_cancel: impl FnMut() -> bool,
) -> Result<ArchiveDiscovery, ScanError> {
    let walked = walk::walk_archive_with_cancel(root, &mut should_cancel)?;
    let complete = walked.complete;
    let grouped = group_files_with_cancel(walked.files, &mut should_cancel)?;
    let mut warnings = walked.warnings;
    let mut groups_by_key = BTreeMap::<String, Vec<DiscoveredGroup>>::new();

    for files in grouped {
        if should_cancel() {
            return Err(ScanError::Cancelled);
        }
        let group = parser::parse_group_cancellable(
            root_id,
            &walked.canonical_root,
            &files,
            &mut should_cancel,
        )?;
        for warning in &group.warnings {
            if should_cancel() {
                return Err(ScanError::Cancelled);
            }
            warnings.push(warning.clone());
        }
        groups_by_key
            .entry(group.remote_key.clone())
            .or_default()
            .push(group);
    }

    let mut groups = Vec::new();
    for (_, keyed_groups) in groups_by_key {
        for group in keyed_groups {
            if should_cancel() {
                return Err(ScanError::Cancelled);
            }
            groups.push(group);
        }
    }
    Ok(ArchiveDiscovery {
        groups,
        warnings,
        complete,
    })
}

pub fn run_scan(
    catalog: &Catalog,
    root: &LibraryRoot,
    scan_id: &str,
    scan_started_at: i64,
    cancellation: &ScanCancellation,
    emit: impl FnMut(&LibraryScanProgress) -> Result<(), String>,
) -> LibraryScanProgress {
    if cancellation.is_cancelled() {
        return emit_terminal(
            emit,
            LibraryScanProgress::Cancelled {
                scan_id: scan_id.to_owned(),
                root_id: root.id,
                summary: ScanSummary::default(),
            },
        );
    }
    match discover_archive_with_cancel(root.id, &root.path, || cancellation.is_cancelled()) {
        Ok(discovery) => process_discovery(
            catalog,
            root.id,
            scan_id,
            scan_started_at,
            discovery,
            cancellation,
            emit,
        ),
        Err(ScanError::Cancelled) => emit_terminal(
            emit,
            LibraryScanProgress::Cancelled {
                scan_id: scan_id.to_owned(),
                root_id: root.id,
                summary: ScanSummary::default(),
            },
        ),
        Err(error) => emit_terminal(
            emit,
            LibraryScanProgress::Failed {
                scan_id: scan_id.to_owned(),
                root_id: root.id,
                error: private_scan_error(&error),
            },
        ),
    }
}

fn process_discovery(
    catalog: &Catalog,
    root_id: i64,
    scan_id: &str,
    scan_started_at: i64,
    discovery: ArchiveDiscovery,
    cancellation: &ScanCancellation,
    emit: impl FnMut(&LibraryScanProgress) -> Result<(), String>,
) -> LibraryScanProgress {
    process_discovery_with_clock(
        catalog,
        root_id,
        scan_id,
        ScanTiming {
            started_at: scan_started_at,
            completion_clock: unix_now,
        },
        discovery,
        cancellation,
        emit,
    )
}

struct ScanTiming<Clock> {
    started_at: i64,
    completion_clock: Clock,
}

fn process_discovery_with_clock(
    catalog: &Catalog,
    root_id: i64,
    scan_id: &str,
    timing: ScanTiming<impl FnOnce() -> i64>,
    discovery: ArchiveDiscovery,
    cancellation: &ScanCancellation,
    mut emit: impl FnMut(&LibraryScanProgress) -> Result<(), String>,
) -> LibraryScanProgress {
    let scan_started_at = timing.started_at;
    let warnings = u64::try_from(discovery.warnings.len()).unwrap_or(u64::MAX);
    let mut summary = ScanSummary {
        warnings,
        ..ScanSummary::default()
    };
    if !discovery.complete {
        return emit_terminal(
            emit,
            LibraryScanProgress::Failed {
                scan_id: scan_id.to_owned(),
                root_id,
                error: "archive traversal was incomplete; reconciliation was skipped".into(),
            },
        );
    }

    let discovered = u64::try_from(discovery.groups.len()).unwrap_or(u64::MAX);
    let mut groups = discovery.groups.into_iter();
    let mut processed = 0_u64;
    loop {
        if cancellation.is_cancelled() {
            return emit_terminal(
                emit,
                LibraryScanProgress::Cancelled {
                    scan_id: scan_id.to_owned(),
                    root_id,
                    summary,
                },
            );
        }

        let mut batch = Vec::with_capacity(SCAN_BATCH_SIZE);
        for _ in 0..SCAN_BATCH_SIZE {
            if cancellation.is_cancelled() {
                return emit_terminal(
                    emit,
                    LibraryScanProgress::Cancelled {
                        scan_id: scan_id.to_owned(),
                        root_id,
                        summary,
                    },
                );
            }
            let Some(mut group) = groups.next() else {
                break;
            };
            group.item.imported_at = scan_started_at;
            group.item.updated_at = scan_started_at;
            for file in &mut group.item.files {
                file.last_seen_at = scan_started_at;
            }
            batch.push(group.item);
        }
        if batch.is_empty() {
            break;
        }
        if cancellation.is_cancelled() {
            return emit_terminal(
                emit,
                LibraryScanProgress::Cancelled {
                    scan_id: scan_id.to_owned(),
                    root_id,
                    summary,
                },
            );
        }

        let results =
            match catalog.upsert_media_batch_cancellable(&batch, || cancellation.is_cancelled()) {
                Ok(results) => results,
                Err(error) => {
                    let terminal = catalog_error_terminal(
                        scan_id,
                        root_id,
                        summary,
                        error,
                        "catalog update failed during library scan",
                    );
                    return emit_terminal(emit, terminal);
                }
            };
        for result in results {
            match result.disposition {
                UpsertDisposition::Inserted => summary.imported += 1,
                UpsertDisposition::Updated => summary.updated += 1,
                UpsertDisposition::Unchanged => {}
            }
        }
        processed = processed.saturating_add(u64::try_from(batch.len()).unwrap_or(u64::MAX));
        let progress = LibraryScanProgress::Scanning {
            scan_id: scan_id.to_owned(),
            root_id,
            discovered,
            processed,
            warnings,
        };
        let _ = emit(&progress);
    }

    if cancellation.is_cancelled() {
        return emit_terminal(
            emit,
            LibraryScanProgress::Cancelled {
                scan_id: scan_id.to_owned(),
                root_id,
                summary,
            },
        );
    }
    let scan_completed_at = (timing.completion_clock)().max(scan_started_at);
    summary.missing =
        match catalog.finalize_scan_cancellable(root_id, scan_started_at, scan_completed_at, || {
            cancellation.is_cancelled()
        }) {
            Ok(missing) => missing,
            Err(error) => {
                let terminal = catalog_error_terminal(
                    scan_id,
                    root_id,
                    summary,
                    error,
                    "catalog finalization failed during library scan",
                );
                return emit_terminal(emit, terminal);
            }
        };
    emit_terminal(
        emit,
        LibraryScanProgress::Done {
            scan_id: scan_id.to_owned(),
            root_id,
            summary,
        },
    )
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or_default()
}

fn catalog_error_terminal(
    scan_id: &str,
    root_id: i64,
    summary: ScanSummary,
    error: CatalogError,
    failed_message: &'static str,
) -> LibraryScanProgress {
    match error {
        CatalogError::Cancelled { .. } => LibraryScanProgress::Cancelled {
            scan_id: scan_id.to_owned(),
            root_id,
            summary,
        },
        _ => LibraryScanProgress::Failed {
            scan_id: scan_id.to_owned(),
            root_id,
            error: failed_message.into(),
        },
    }
}

fn emit_terminal(
    mut emit: impl FnMut(&LibraryScanProgress) -> Result<(), String>,
    terminal: LibraryScanProgress,
) -> LibraryScanProgress {
    let _ = emit(&terminal);
    terminal
}

fn private_scan_error(error: &ScanError) -> String {
    match error {
        ScanError::Cancelled => "library scan was cancelled".into(),
        ScanError::InvalidRoot { .. } => "library root is not a directory".into(),
        ScanError::Io {
            operation, source, ..
        } => {
            format!("could not {operation}: {:?}", source.kind())
        }
    }
}

fn group_files_with_cancel(
    files: Vec<DiscoveredFile>,
    mut should_cancel: impl FnMut() -> bool,
) -> Result<Vec<Vec<DiscoveredFile>>, ScanError> {
    let mut media_by_directory: BTreeMap<PathBuf, BTreeMap<PathBuf, DiscoveredFile>> =
        BTreeMap::new();
    let mut metadata_by_directory: BTreeMap<PathBuf, BTreeMap<PathBuf, DiscoveredFile>> =
        BTreeMap::new();

    for file in files {
        if should_cancel() {
            return Err(ScanError::Cancelled);
        }
        let parent = file
            .relative_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        let relative_path = file.relative_path.clone();
        if extension_is(&file.relative_path, "json") {
            metadata_by_directory
                .entry(parent)
                .or_default()
                .insert(relative_path, file);
        } else {
            media_by_directory
                .entry(parent)
                .or_default()
                .insert(relative_path, file);
        }
    }

    let mut groups = Vec::new();
    for (directory, media) in media_by_directory {
        if should_cancel() {
            return Err(ScanError::Cancelled);
        }

        let mut sibling_counts = BTreeMap::new();
        for file in media.values() {
            if should_cancel() {
                return Err(ScanError::Cancelled);
            }
            let stem = file_stem_lossy(&file.relative_path);
            let (base, _) = parser::carousel_base(&stem);
            *sibling_counts.entry(base).or_insert(0_usize) += 1;
        }

        let mut directory_groups: BTreeMap<String, BTreeMap<(u32, PathBuf), DiscoveredFile>> =
            BTreeMap::new();
        for file in media.into_values() {
            if should_cancel() {
                return Err(ScanError::Cancelled);
            }
            let stem = file_stem_lossy(&file.relative_path);
            let (base, suffix) = parser::carousel_base(&stem);
            let has_siblings = sibling_counts.get(&base).copied().unwrap_or_default() > 1;
            let key = if parser::path_identity(&file.relative_path).is_some() {
                stem
            } else if suffix > 0 && has_siblings {
                base
            } else {
                stem
            };
            directory_groups
                .entry(key)
                .or_default()
                .insert((suffix, file.relative_path.clone()), file);
        }

        let metadata = metadata_by_directory.remove(&directory).unwrap_or_default();
        for (base, resources) in directory_groups {
            if should_cancel() {
                return Err(ScanError::Cancelled);
            }
            let mut files = Vec::with_capacity(resources.len());
            for (ordinal, (_, mut file)) in resources.into_iter().enumerate() {
                if should_cancel() {
                    return Err(ScanError::Cancelled);
                }
                file.ordinal = i64::try_from(ordinal).unwrap_or(i64::MAX);
                files.push(file);
            }

            for sidecar in metadata.values() {
                if should_cancel() {
                    return Err(ScanError::Cancelled);
                }
                if sidecar_matches_group_with_cancel(
                    &sidecar.relative_path,
                    &base,
                    &files,
                    &mut should_cancel,
                )? {
                    files.push(sidecar.clone());
                }
            }
            groups.push(files);
        }
    }
    Ok(groups)
}

fn sidecar_matches_group_with_cancel(
    path: &Path,
    base: &str,
    media: &[DiscoveredFile],
    mut should_cancel: impl FnMut() -> bool,
) -> Result<bool, ScanError> {
    let sidecar_stem = file_stem_lossy(path);
    if sidecar_stem == base {
        return Ok(true);
    }
    for file in media {
        if should_cancel() {
            return Err(ScanError::Cancelled);
        }
        if file_stem_lossy(&file.relative_path) == sidecar_stem {
            return Ok(true);
        }
    }
    Ok(false)
}

fn file_stem_lossy(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn extension_is(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    mod scan_service {
        use std::fs;
        use std::path::Path;
        use std::sync::{Arc, Barrier};
        use std::thread;

        use serde_json::json;
        use tempfile::TempDir;

        use crate::catalog::{
            Catalog, CatalogError, FileAvailability, LibraryQuery, LibraryRoot, LibrarySort,
            UpsertDisposition,
        };
        use crate::jobs::ScanRegistry;

        use super::super::{
            catalog_error_terminal, group_files_with_cancel, process_discovery,
            process_discovery_with_clock, run_scan, ArchiveDiscovery, LibraryScanProgress,
            ScanCancellation, ScanError, ScanSummary, ScanTiming,
        };

        struct Fixture {
            _directory: TempDir,
            archive: TempDir,
            catalog: Catalog,
            root: LibraryRoot,
        }

        impl Fixture {
            fn new() -> Self {
                let directory = tempfile::tempdir().unwrap();
                let archive = tempfile::tempdir().unwrap();
                let catalog = Catalog::open(directory.path().join("catalog.sqlite3")).unwrap();
                let root = catalog.register_root(archive.path(), "Archive").unwrap();
                Self {
                    _directory: directory,
                    archive,
                    catalog,
                    root,
                }
            }

            fn write_media(&self, count: usize) {
                let posts = self.archive.path().join("posts");
                fs::create_dir_all(&posts).unwrap();
                for index in 0..count {
                    fs::write(posts.join(format!("item-{index:04}.jpg")), b"photo").unwrap();
                }
            }

            fn scan(
                &self,
                scan_id: &str,
                started_at: i64,
                cancellation: &ScanCancellation,
                events: &mut Vec<LibraryScanProgress>,
            ) -> LibraryScanProgress {
                self.catalog.begin_scan(self.root.id, started_at).unwrap();
                run_scan(
                    &self.catalog,
                    &self.root,
                    scan_id,
                    started_at,
                    cancellation,
                    |event| {
                        events.push(event.clone());
                        Ok(())
                    },
                )
            }

            fn library_items(&self) -> Vec<crate::catalog::LibraryCard> {
                self.catalog
                    .query_library(&LibraryQuery {
                        search: None,
                        kinds: Vec::new(),
                        source_id: None,
                        availability: None,
                        taken_after: None,
                        taken_before: None,
                        sort: LibrarySort::ImportedAtDesc,
                        cursor: None,
                        limit: 100,
                    })
                    .unwrap()
                    .items
            }

            fn library_item_count(&self) -> usize {
                let mut cursor = None;
                let mut count = 0;
                loop {
                    let page = self
                        .catalog
                        .query_library(&LibraryQuery {
                            search: None,
                            kinds: Vec::new(),
                            source_id: None,
                            availability: None,
                            taken_after: None,
                            taken_before: None,
                            sort: LibrarySort::ImportedAtDesc,
                            cursor,
                            limit: 100,
                        })
                        .unwrap();
                    count += page.items.len();
                    let Some(next_cursor) = page.next_cursor else {
                        return count;
                    };
                    cursor = Some(next_cursor);
                }
            }
        }

        #[test]
        fn commits_205_groups_as_three_bounded_batches_with_monotonic_progress() {
            let fixture = Fixture::new();
            fixture.write_media(205);
            let cancellation = ScanCancellation::new();
            let mut events = Vec::new();

            let terminal = fixture.scan("scan-205", 100, &cancellation, &mut events);

            assert_eq!(
                terminal,
                LibraryScanProgress::Done {
                    scan_id: "scan-205".into(),
                    root_id: fixture.root.id,
                    summary: ScanSummary {
                        imported: 205,
                        updated: 0,
                        missing: 0,
                        warnings: 0,
                    },
                }
            );
            let progress = events
                .iter()
                .filter_map(|event| match event {
                    LibraryScanProgress::Scanning {
                        discovered,
                        processed,
                        warnings,
                        ..
                    } => Some((*discovered, *processed, *warnings)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(progress, vec![(205, 100, 0), (205, 200, 0), (205, 205, 0)]);
            assert_eq!(fixture.library_item_count(), 205);
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, LibraryScanProgress::Done { .. }))
                    .count(),
                1
            );
        }

        #[test]
        fn cancellation_between_batches_keeps_only_completed_batch() {
            let fixture = Fixture::new();
            fixture.write_media(205);
            fixture.catalog.begin_scan(fixture.root.id, 200).unwrap();
            let cancellation = ScanCancellation::new();
            let cancel_after_first = cancellation.clone();
            let mut events = Vec::new();

            let terminal = run_scan(
                &fixture.catalog,
                &fixture.root,
                "cancelled-scan",
                200,
                &cancellation,
                |event| {
                    events.push(event.clone());
                    if matches!(event, LibraryScanProgress::Scanning { processed: 100, .. }) {
                        cancel_after_first.cancel();
                    }
                    Ok(())
                },
            );

            assert_eq!(
                terminal,
                LibraryScanProgress::Cancelled {
                    scan_id: "cancelled-scan".into(),
                    root_id: fixture.root.id,
                    summary: ScanSummary {
                        imported: 100,
                        updated: 0,
                        missing: 0,
                        warnings: 0,
                    },
                }
            );
            assert_eq!(fixture.library_item_count(), 100);
            let root = fixture.catalog.list_roots().unwrap().remove(0);
            assert_eq!(root.last_scan_started_at, Some(200));
            assert_eq!(root.last_scan_completed_at, None);
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, LibraryScanProgress::Scanning { .. }))
                    .count(),
                1
            );
        }

        #[test]
        fn typed_catalog_cancellation_maps_to_cancelled_with_completed_batch_summary() {
            let summary = ScanSummary {
                imported: 100,
                updated: 3,
                missing: 0,
                warnings: 2,
            };

            let cancelled = catalog_error_terminal(
                "typed-cancel",
                7,
                summary,
                CatalogError::Cancelled {
                    operation: "committing media upsert",
                },
                "catalog update failed during library scan",
            );
            let failed = catalog_error_terminal(
                "real-failure",
                7,
                summary,
                CatalogError::InvalidInput {
                    message: "broken input".into(),
                },
                "catalog update failed during library scan",
            );

            assert_eq!(
                cancelled,
                LibraryScanProgress::Cancelled {
                    scan_id: "typed-cancel".into(),
                    root_id: 7,
                    summary,
                }
            );
            assert!(matches!(failed, LibraryScanProgress::Failed { .. }));
        }

        #[test]
        fn completed_rescan_marks_disappeared_file_missing_and_reappearance_available() {
            let fixture = Fixture::new();
            fixture.write_media(2);
            let cancellation = ScanCancellation::new();
            let mut events = Vec::new();
            let first = fixture.scan("first", 300, &cancellation, &mut events);
            assert!(matches!(first, LibraryScanProgress::Done { .. }));

            fs::remove_file(fixture.archive.path().join("posts/item-0001.jpg")).unwrap();
            events.clear();
            let second = fixture.scan("second", 400, &cancellation, &mut events);
            assert!(matches!(
                second,
                LibraryScanProgress::Done {
                    summary: ScanSummary {
                        imported: 0,
                        updated: 1,
                        missing: 1,
                        warnings: 0,
                    },
                    ..
                }
            ));
            let missing = fixture
                .library_items()
                .into_iter()
                .find(|item| item.remote_key.contains("item%2D0001"))
                .unwrap();
            assert_eq!(missing.availability, FileAvailability::Missing);

            fs::write(fixture.archive.path().join("posts/item-0001.jpg"), b"photo").unwrap();
            events.clear();
            let third = fixture.scan("third", 500, &cancellation, &mut events);
            assert!(matches!(
                third,
                LibraryScanProgress::Done {
                    summary: ScanSummary {
                        imported: 0,
                        updated: 2,
                        missing: 0,
                        warnings: 0,
                    },
                    ..
                }
            ));
            assert!(fixture
                .library_items()
                .iter()
                .all(|item| item.availability == FileAvailability::Available));
        }

        #[test]
        fn malformed_sidecar_warns_but_media_is_imported() {
            let fixture = Fixture::new();
            let posts = fixture.archive.path().join("posts");
            fs::create_dir_all(&posts).unwrap();
            fs::write(posts.join("broken.jpg"), b"photo").unwrap();
            fs::write(posts.join("broken.json"), b"{not-json").unwrap();
            let cancellation = ScanCancellation::new();
            let mut events = Vec::new();

            let terminal = fixture.scan("warnings", 600, &cancellation, &mut events);

            assert!(matches!(
                terminal,
                LibraryScanProgress::Done {
                    summary: ScanSummary {
                        imported: 1,
                        updated: 0,
                        missing: 0,
                        warnings: 1,
                    },
                    ..
                }
            ));
            assert_eq!(fixture.library_items().len(), 1);
        }

        #[test]
        fn scan_registry_atomically_rejects_same_root_allows_others_and_cleans_up() {
            let registry = Arc::new(ScanRegistry::new());
            let barrier = Arc::new(Barrier::new(17));
            let handles = (0..16)
                .map(|index| {
                    let registry = registry.clone();
                    let barrier = barrier.clone();
                    thread::spawn(move || {
                        barrier.wait();
                        registry.try_register(7, format!("scan-{index}"))
                    })
                })
                .collect::<Vec<_>>();
            barrier.wait();
            let leases = handles
                .into_iter()
                .filter_map(|handle| handle.join().unwrap().ok())
                .collect::<Vec<_>>();

            assert_eq!(leases.len(), 1);
            assert!(registry.try_register(7, "duplicate").is_err());
            let other = registry.try_register(8, "other-root").unwrap();
            assert_eq!(registry.active_len(), 2);
            let winner_id = leases[0].scan_id().to_owned();
            assert!(registry.cancel(&winner_id));
            assert!(leases[0].cancellation().is_cancelled());
            drop(leases);
            drop(other);
            assert_eq!(registry.active_len(), 0);
            assert!(registry.try_register(7, "after-terminal").is_ok());
        }

        #[test]
        fn incomplete_discovery_fails_without_writes_reconciliation_or_finish() {
            let fixture = Fixture::new();
            fixture.write_media(1);
            let cancellation = ScanCancellation::new();
            let mut events = Vec::new();
            let first = fixture.scan("seed", 700, &cancellation, &mut events);
            assert!(matches!(first, LibraryScanProgress::Done { .. }));
            let complete_before = fixture
                .catalog
                .list_roots()
                .unwrap()
                .remove(0)
                .last_scan_completed_at;
            fixture.catalog.begin_scan(fixture.root.id, 800).unwrap();
            let discovery = ArchiveDiscovery {
                groups: Vec::new(),
                warnings: vec![super::super::ScanWarning {
                    relative_path: Some("posts/unreadable.jpg".into()),
                    message: "could not inspect archive entry: PermissionDenied".into(),
                }],
                complete: false,
            };
            events.clear();

            let terminal = process_discovery(
                &fixture.catalog,
                fixture.root.id,
                "incomplete",
                800,
                discovery,
                &cancellation,
                |event| {
                    events.push(event.clone());
                    Ok(())
                },
            );

            assert!(matches!(terminal, LibraryScanProgress::Failed { .. }));
            assert_eq!(
                fixture.library_items()[0].availability,
                FileAvailability::Available
            );
            let root = fixture.catalog.list_roots().unwrap().remove(0);
            assert_eq!(root.last_scan_started_at, Some(800));
            assert_eq!(root.last_scan_completed_at, complete_before);
            assert!(events
                .iter()
                .all(|event| !matches!(event, LibraryScanProgress::Done { .. })));
        }

        #[test]
        fn empty_completed_rescan_uses_completion_clock_and_marks_prior_file_missing() {
            let fixture = Fixture::new();
            fixture.write_media(1);
            let cancellation = ScanCancellation::new();
            let mut seed_events = Vec::new();
            assert!(matches!(
                fixture.scan("seed-empty", 1_200, &cancellation, &mut seed_events),
                LibraryScanProgress::Done { .. }
            ));
            fs::remove_file(fixture.archive.path().join("posts/item-0000.jpg")).unwrap();
            fixture.catalog.begin_scan(fixture.root.id, 1_300).unwrap();
            let discovery = ArchiveDiscovery {
                groups: Vec::new(),
                warnings: Vec::new(),
                complete: true,
            };
            let mut events = Vec::new();

            let terminal = process_discovery_with_clock(
                &fixture.catalog,
                fixture.root.id,
                "empty",
                ScanTiming {
                    started_at: 1_300,
                    completion_clock: || 1_375,
                },
                discovery,
                &cancellation,
                |event| {
                    events.push(event.clone());
                    Ok(())
                },
            );

            assert_eq!(
                terminal,
                LibraryScanProgress::Done {
                    scan_id: "empty".into(),
                    root_id: fixture.root.id,
                    summary: ScanSummary {
                        imported: 0,
                        updated: 0,
                        missing: 1,
                        warnings: 0,
                    },
                }
            );
            assert!(events
                .iter()
                .all(|event| !matches!(event, LibraryScanProgress::Scanning { .. })));
            assert_eq!(
                fixture.library_items()[0].availability,
                FileAvailability::Missing
            );
            let root = fixture.catalog.list_roots().unwrap().remove(0);
            assert_eq!(root.last_scan_started_at, Some(1_300));
            assert_eq!(root.last_scan_completed_at, Some(1_375));
        }

        #[test]
        fn cancellation_after_walk_stops_inside_grouping_before_parse() {
            let fixture = Fixture::new();
            fixture.write_media(50);
            let walked = super::super::walk::walk_archive(&fixture.root.path).unwrap();
            let partition_size = walked.files.len();
            let mut checks = 0;

            let result = group_files_with_cancel(walked.files, || {
                checks += 1;
                checks > partition_size
            });

            assert!(matches!(result, Err(ScanError::Cancelled)));
            assert_eq!(checks, partition_size + 1);
        }

        #[test]
        fn cancellation_inside_huge_group_stops_parser_file_loops() {
            let fixture = Fixture::new();
            let files = (0..1_000)
                .map(|index| super::super::DiscoveredFile {
                    relative_path: format!("posts/carousel_{index}.jpg").into(),
                    byte_size: 5,
                    mtime: 10,
                    ordinal: index,
                })
                .collect::<Vec<_>>();
            let mut checks = 0;

            let result = super::super::parser::parse_group_cancellable(
                fixture.root.id,
                &fixture.root.path,
                &files,
                &mut || {
                    checks += 1;
                    checks > 10
                },
            );

            assert!(matches!(result, Err(ScanError::Cancelled)));
            assert_eq!(checks, 11);
        }

        #[test]
        fn event_delivery_failure_does_not_change_scan_outcome_or_catalog_state() {
            let fixture = Fixture::new();
            fixture.write_media(101);
            fixture.catalog.begin_scan(fixture.root.id, 900).unwrap();
            let cancellation = ScanCancellation::new();
            let registry = Arc::new(ScanRegistry::new());
            let lease = registry
                .try_register(fixture.root.id, "event-failure")
                .unwrap();
            let mut delivery_attempts = 0;

            let terminal = run_scan(
                &fixture.catalog,
                &fixture.root,
                "event-failure",
                900,
                &cancellation,
                |_| {
                    delivery_attempts += 1;
                    Err("event channel closed".into())
                },
            );

            assert!(delivery_attempts >= 2);
            assert!(matches!(
                terminal,
                LibraryScanProgress::Done {
                    summary: ScanSummary {
                        imported: 101,
                        updated: 0,
                        missing: 0,
                        warnings: 0,
                    },
                    ..
                }
            ));
            assert_eq!(fixture.library_item_count(), 101);
            assert!(fixture
                .catalog
                .list_roots()
                .unwrap()
                .remove(0)
                .last_scan_completed_at
                .is_some_and(|completed| completed > 900));
            drop(lease);
            assert_eq!(registry.active_len(), 0);
        }

        #[test]
        fn progress_serde_uses_exact_tag_names_and_failures_hide_absolute_roots() {
            let scanning = LibraryScanProgress::Scanning {
                scan_id: "serde-scan".into(),
                root_id: 9,
                discovered: 12,
                processed: 10,
                warnings: 2,
            };
            assert_eq!(
                serde_json::to_value(scanning).unwrap(),
                json!({
                    "state": "scanning",
                    "scan_id": "serde-scan",
                    "root_id": 9,
                    "discovered": 12,
                    "processed": 10,
                    "warnings": 2
                })
            );
            assert_eq!(
                serde_json::to_value(LibraryScanProgress::Cancelled {
                    scan_id: "serde-scan".into(),
                    root_id: 9,
                    summary: ScanSummary {
                        imported: 1,
                        updated: 2,
                        missing: 0,
                        warnings: 3,
                    },
                })
                .unwrap(),
                json!({
                    "state": "cancelled",
                    "scan_id": "serde-scan",
                    "root_id": 9,
                    "summary": {"imported": 1, "updated": 2, "missing": 0, "warnings": 3}
                })
            );

            let fixture = Fixture::new();
            let absolute_root = fixture.root.path.to_string_lossy().into_owned();
            fs::remove_dir_all(&fixture.root.path).unwrap();
            fixture.catalog.begin_scan(fixture.root.id, 1_000).unwrap();
            let terminal = run_scan(
                &fixture.catalog,
                &fixture.root,
                "private-failure",
                1_000,
                &ScanCancellation::new(),
                |_| Ok(()),
            );
            let LibraryScanProgress::Failed { error, .. } = terminal else {
                panic!("expected failed scan");
            };
            assert!(!error.contains(&absolute_root));
            assert!(!error.contains(Path::new(&absolute_root).to_string_lossy().as_ref()));
        }

        #[test]
        fn summary_distinguishes_imported_updated_and_unchanged() {
            let fixture = Fixture::new();
            fixture.write_media(1);
            let cancellation = ScanCancellation::new();
            let mut events = Vec::new();
            let first = fixture.scan("import", 1_100, &cancellation, &mut events);
            let LibraryScanProgress::Done { summary, .. } = first else {
                panic!("expected completed scan");
            };
            assert_eq!(summary.imported, 1);

            let discovery =
                super::super::discover_archive(fixture.root.id, &fixture.root.path).unwrap();
            let mut identical = discovery;
            for group in &mut identical.groups {
                group.item.imported_at = 1_100;
                group.item.updated_at = 1_100;
                for file in &mut group.item.files {
                    file.last_seen_at = 1_100;
                }
            }
            let terminal = process_discovery(
                &fixture.catalog,
                fixture.root.id,
                "unchanged",
                1_100,
                identical,
                &cancellation,
                |_| Ok(()),
            );
            assert!(matches!(
                terminal,
                LibraryScanProgress::Done {
                    summary: ScanSummary {
                        imported: 0,
                        updated: 0,
                        ..
                    },
                    ..
                }
            ));

            let detail = fixture
                .catalog
                .get_library_item(fixture.library_items()[0].id)
                .unwrap()
                .unwrap();
            assert_eq!(detail.imported_at, 1_100);
            assert_eq!(detail.updated_at, 1_100);
            assert_eq!(
                fixture
                    .catalog
                    .upsert_media(&detail_to_input(&detail, fixture.root.id, 1_100))
                    .unwrap()
                    .disposition,
                UpsertDisposition::Unchanged
            );
        }

        fn detail_to_input(
            detail: &crate::catalog::LibraryItemDetail,
            root_id: i64,
            seen_at: i64,
        ) -> crate::catalog::CatalogMediaInput {
            crate::catalog::CatalogMediaInput {
                remote_key: detail.remote_key.clone(),
                kind: detail.kind,
                remote_pk: detail.remote_pk.clone(),
                shortcode: detail.shortcode.clone(),
                owner_pk: detail.owner_pk.clone(),
                owner_username: detail.owner_username.clone(),
                taken_at: detail.taken_at,
                caption: detail.caption.clone(),
                like_count: detail.like_count,
                comment_count: detail.comment_count,
                imported_at: detail.imported_at,
                updated_at: detail.updated_at,
                files: detail
                    .files
                    .iter()
                    .map(|file| crate::catalog::CatalogFileInput {
                        root_id,
                        relative_path: file.relative_path.as_str().into(),
                        ordinal: file.ordinal,
                        kind: file.kind,
                        byte_size: file.byte_size,
                        mtime: file.mtime,
                        last_seen_at: seen_at,
                    })
                    .collect(),
                source_id: None,
            }
        }
    }
}
