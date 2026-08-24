pub mod parser;
pub mod walk;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use thiserror::Error;

pub use parser::DiscoveredGroup;

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
    let walked = walk::walk_archive(root)?;
    let complete = walked.complete;
    let grouped = group_files(walked.files);
    let mut warnings = walked.warnings;
    let mut groups = Vec::with_capacity(grouped.len());

    for files in grouped {
        let group = parser::parse_group(root_id, &walked.canonical_root, &files);
        warnings.extend(group.warnings.iter().cloned());
        groups.push(group);
    }

    groups.sort_by(|left, right| left.remote_key.cmp(&right.remote_key));
    Ok(ArchiveDiscovery {
        groups,
        warnings,
        complete,
    })
}

fn group_files(files: Vec<DiscoveredFile>) -> Vec<Vec<DiscoveredFile>> {
    let mut media_by_directory: BTreeMap<PathBuf, Vec<DiscoveredFile>> = BTreeMap::new();
    let mut metadata_by_directory: BTreeMap<PathBuf, Vec<DiscoveredFile>> = BTreeMap::new();

    for file in files {
        let parent = file
            .relative_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        if extension_is(&file.relative_path, "json") {
            metadata_by_directory.entry(parent).or_default().push(file);
        } else {
            media_by_directory.entry(parent).or_default().push(file);
        }
    }

    let mut groups = Vec::new();
    for (directory, mut media) in media_by_directory {
        media.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

        let sibling_counts = media.iter().fold(HashMap::new(), |mut counts, file| {
            let stem = file_stem_lossy(&file.relative_path);
            let (base, _) = parser::carousel_base(&stem);
            *counts.entry(base).or_insert(0_usize) += 1;
            counts
        });

        let mut directory_groups: BTreeMap<String, Vec<(u32, DiscoveredFile)>> = BTreeMap::new();
        for file in media {
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
                .push((suffix, file));
        }

        let metadata = metadata_by_directory.remove(&directory).unwrap_or_default();
        for (base, mut resources) in directory_groups {
            resources.sort_by(|(left_suffix, left), (right_suffix, right)| {
                left_suffix
                    .cmp(right_suffix)
                    .then_with(|| left.relative_path.cmp(&right.relative_path))
            });

            let mut files = resources
                .into_iter()
                .enumerate()
                .map(|(ordinal, (_, mut file))| {
                    file.ordinal = i64::try_from(ordinal).unwrap_or(i64::MAX);
                    file
                })
                .collect::<Vec<_>>();

            let mut sidecars = metadata
                .iter()
                .filter(|file| sidecar_matches_group(&file.relative_path, &base, &files))
                .cloned()
                .collect::<Vec<_>>();
            sidecars.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
            files.extend(sidecars);
            groups.push(files);
        }
    }
    groups
}

fn sidecar_matches_group(path: &Path, base: &str, media: &[DiscoveredFile]) -> bool {
    let sidecar_stem = file_stem_lossy(path);
    if sidecar_stem == base {
        return true;
    }
    media
        .iter()
        .any(|file| file_stem_lossy(&file.relative_path) == sidecar_stem)
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
