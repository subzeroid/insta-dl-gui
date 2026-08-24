use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::{DirEntry, WalkDir};

use super::{DiscoveredFile, ScanError, ScanWarning};

#[derive(Debug)]
pub struct WalkedArchive {
    pub canonical_root: PathBuf,
    pub files: Vec<DiscoveredFile>,
    pub warnings: Vec<ScanWarning>,
    pub complete: bool,
}

pub fn walk_archive(root: &Path) -> Result<WalkedArchive, ScanError> {
    let metadata = fs::metadata(root).map_err(|source| ScanError::Io {
        operation: "inspect library root",
        path: root.to_path_buf(),
        source,
    })?;
    if !metadata.is_dir() {
        return Err(ScanError::InvalidRoot {
            path: root.to_path_buf(),
        });
    }
    let canonical_root = root.canonicalize().map_err(|source| ScanError::Io {
        operation: "canonicalize library root",
        path: root.to_path_buf(),
        source,
    })?;

    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut complete = true;
    for entry in WalkDir::new(&canonical_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_hidden_below_root(entry, &canonical_root))
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let fallback = std::io::Error::other("unknown traversal error");
                record_incomplete_io(
                    &mut complete,
                    &mut warnings,
                    &canonical_root,
                    error.path(),
                    "could not inspect archive entry",
                    error.io_error().unwrap_or(&fallback),
                );
                continue;
            }
        };
        if entry.depth() == 0 || entry.file_type().is_dir() {
            continue;
        }

        let path = entry.path();
        let relative_path = match path.strip_prefix(&canonical_root) {
            Ok(relative_path) => relative_path.to_path_buf(),
            Err(_) => {
                complete = false;
                warnings.push(ScanWarning {
                    relative_path: None,
                    message: "archive entry escaped the canonical library root".into(),
                });
                continue;
            }
        };
        if let Err(warning) = validate_catalog_relative_path(&relative_path) {
            warnings.push(warning);
            continue;
        }
        if !is_supported_path(&relative_path) {
            continue;
        }

        if entry.file_type().is_symlink() {
            let target = match path.canonicalize() {
                Ok(target) => target,
                Err(error) => {
                    record_incomplete_io(
                        &mut complete,
                        &mut warnings,
                        &canonical_root,
                        Some(path),
                        "could not resolve symlink target",
                        &error,
                    );
                    continue;
                }
            };
            if !target.starts_with(&canonical_root) {
                warnings.push(ScanWarning {
                    relative_path: Some(relative_path),
                    message: "symlink target is outside the library root; skipped".into(),
                });
                continue;
            }
            if !target.is_file() {
                continue;
            }
        } else if !entry.file_type().is_file() {
            continue;
        }

        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                record_incomplete_io(
                    &mut complete,
                    &mut warnings,
                    &canonical_root,
                    Some(path),
                    "could not read file metadata",
                    &error,
                );
                continue;
            }
        };
        let byte_size = match i64::try_from(metadata.len()) {
            Ok(byte_size) => byte_size,
            Err(_) => {
                complete = false;
                warnings.push(ScanWarning {
                    relative_path: Some(relative_path),
                    message: "file is too large for the catalog".into(),
                });
                continue;
            }
        };

        files.push(DiscoveredFile {
            relative_path,
            byte_size,
            mtime: metadata
                .modified()
                .map(system_time_seconds)
                .unwrap_or_default(),
            ordinal: 0,
        });
    }

    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(WalkedArchive {
        canonical_root,
        files,
        warnings,
        complete,
    })
}

fn record_incomplete_io(
    complete: &mut bool,
    warnings: &mut Vec<ScanWarning>,
    canonical_root: &Path,
    entry_path: Option<&Path>,
    operation: &'static str,
    error: &std::io::Error,
) {
    *complete = false;
    let relative_path = entry_path.and_then(|path| {
        path.strip_prefix(canonical_root)
            .ok()
            .map(Path::to_path_buf)
            .or_else(|| (!path.is_absolute()).then(|| path.to_path_buf()))
    });
    warnings.push(ScanWarning {
        relative_path,
        message: format!("{operation}: {:?}", error.kind()),
    });
}

pub fn validate_catalog_relative_path(relative_path: &Path) -> Result<(), ScanWarning> {
    if relative_path.to_str().is_none() {
        return Err(ScanWarning {
            relative_path: Some(relative_path.to_path_buf()),
            message: "archive entry path is not valid UTF-8; skipped".into(),
        });
    }
    Ok(())
}

fn is_hidden_below_root(entry: &DirEntry, canonical_root: &Path) -> bool {
    entry.path() != canonical_root
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
}

fn is_supported_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["jpg", "jpeg", "png", "webp", "mp4", "mov", "json"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

fn system_time_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(error) => -i64::try_from(error.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::Path;

    use super::{record_incomplete_io, ScanWarning};

    #[test]
    fn scanner_walk_gap_marks_discovery_incomplete_without_absolute_warning_path() {
        let root = Path::new("/private/library-root");
        let entry = root.join("posts/unreadable.jpg");
        let error = io::Error::from(io::ErrorKind::PermissionDenied);
        let mut complete = true;
        let mut warnings = Vec::<ScanWarning>::new();

        record_incomplete_io(
            &mut complete,
            &mut warnings,
            root,
            Some(&entry),
            "could not inspect archive entry",
            &error,
        );

        assert!(!complete);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].relative_path.as_deref(),
            Some(Path::new("posts/unreadable.jpg"))
        );
        assert!(!warnings[0].message.contains("/private/library-root"));
        assert!(!warnings[0].message.contains("unreadable.jpg"));
        assert!(warnings[0].message.contains("PermissionDenied"));
    }
}
