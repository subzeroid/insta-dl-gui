use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use chrono::NaiveDateTime;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::Value;

use crate::catalog::{CatalogFileInput, CatalogMediaInput, MediaFileKind, MediaItemKind};

use super::{extension_is, DiscoveredFile, ScanError, ScanWarning};

const MAX_SIDECAR_BYTES: i64 = 4 * 1024 * 1024;

#[derive(Debug)]
pub struct DiscoveredGroup {
    pub remote_key: String,
    pub item: CatalogMediaInput,
    pub warnings: Vec<ScanWarning>,
}

pub fn parse_group(root_id: i64, root: &Path, files: &[DiscoveredFile]) -> DiscoveredGroup {
    parse_group_cancellable(root_id, root, files, &mut || false)
        .expect("the public parser uses a non-cancelling predicate")
}

pub(crate) fn parse_group_cancellable(
    root_id: i64,
    root: &Path,
    files: &[DiscoveredFile],
    should_cancel: &mut impl FnMut() -> bool,
) -> Result<DiscoveredGroup, ScanError> {
    let mut warnings = Vec::new();
    let mut sidecar_file = None;
    for file in files {
        if should_cancel() {
            return Err(ScanError::Cancelled);
        }
        if extension_is(&file.relative_path, "json") {
            sidecar_file = Some(file);
            break;
        }
    }
    let sidecar = sidecar_file.and_then(|file| read_sidecar(root, file, &mut warnings));
    let mut first_media = None;
    for file in files {
        if should_cancel() {
            return Err(ScanError::Cancelled);
        }
        if !extension_is(&file.relative_path, "json") {
            first_media = Some(file);
            break;
        }
    }
    let relative_path = first_media
        .map(|file| file.relative_path.as_path())
        .unwrap_or_else(|| Path::new("unknown"));
    let inferred_kind = infer_kind(relative_path);
    let sidecar_identity = sidecar
        .as_ref()
        .and_then(|sidecar| validated_sidecar_identity(sidecar, inferred_kind));
    let path_identity = path_identity(relative_path).map(|(kind, remote_pk)| {
        let remote_key = format!("{}:{remote_pk}", remote_prefix(kind));
        (kind, remote_pk, remote_key)
    });
    let (kind, remote_pk, remote_key) = match sidecar_identity.or(path_identity) {
        Some((kind, remote_pk, remote_key)) => (kind, Some(remote_pk), remote_key),
        None => {
            let local_path = carousel_identity_path_cancellable(files, should_cancel)?
                .unwrap_or_else(|| relative_path.into());
            let encoded = percent_encode_relative(&local_path);
            (inferred_kind, None, format!("local:{root_id}:{encoded}"))
        }
    };
    let now = unix_now();

    let mut item_files = Vec::with_capacity(files.len());
    for file in files {
        if should_cancel() {
            return Err(ScanError::Cancelled);
        }
        item_files.push(CatalogFileInput {
            root_id,
            relative_path: file.relative_path.clone(),
            ordinal: file.ordinal,
            kind: file_kind(&file.relative_path),
            byte_size: file.byte_size,
            mtime: file.mtime,
            last_seen_at: now,
        });
    }
    let owner = sidecar.as_ref().and_then(|sidecar| sidecar.get("owner"));
    let item = CatalogMediaInput {
        remote_key: remote_key.clone(),
        kind,
        remote_pk,
        shortcode: sidecar
            .as_ref()
            .and_then(|sidecar| sidecar_string(sidecar, "code")),
        owner_pk: owner.and_then(|owner| value_string(owner.get("pk"))),
        owner_username: owner.and_then(|owner| value_string(owner.get("username"))),
        taken_at: sidecar
            .as_ref()
            .and_then(|sidecar| sidecar.get("taken_at"))
            .and_then(value_i64),
        caption: sidecar
            .as_ref()
            .and_then(|sidecar| value_string(sidecar.get("caption"))),
        like_count: nonnegative_count_cancellable(
            &sidecar,
            "like_count",
            files,
            &mut warnings,
            should_cancel,
        )?,
        comment_count: nonnegative_count_cancellable(
            &sidecar,
            "comment_count",
            files,
            &mut warnings,
            should_cancel,
        )?,
        imported_at: now,
        updated_at: now,
        files: item_files,
        source_id: None,
    };

    Ok(DiscoveredGroup {
        remote_key,
        item,
        warnings,
    })
}

fn validated_sidecar_identity(
    sidecar: &Value,
    inferred_kind: MediaItemKind,
) -> Option<(MediaItemKind, String, String)> {
    let remote_pk = sidecar_string(sidecar, "pk").filter(|pk| valid_remote_pk(pk))?;
    if let Some(catalog) = sidecar.get("catalog") {
        let version = catalog.get("version").and_then(Value::as_u64);
        let remote_key = catalog.get("remote_key").and_then(Value::as_str);
        let item_kind = catalog.get("item_kind").and_then(Value::as_str);
        let expected_key = format!("post:{remote_pk}");
        let kind = match item_kind {
            Some("post") => Some(MediaItemKind::Post),
            Some("reel") => Some(MediaItemKind::Reel),
            _ => None,
        };
        if version == Some(1) && remote_key == Some(expected_key.as_str()) {
            if let Some(kind) = kind {
                return Some((kind, remote_pk, expected_key));
            }
        }
    }
    let remote_key = format!("{}:{remote_pk}", remote_prefix(inferred_kind));
    Some((inferred_kind, remote_pk, remote_key))
}

fn valid_remote_pk(pk: &str) -> bool {
    !pk.is_empty() && pk.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn carousel_base(stem: &str) -> (String, u32) {
    let Some((base, suffix)) = stem.rsplit_once('_') else {
        return (stem.to_owned(), 0);
    };
    match suffix.parse::<u32>() {
        Ok(ordinal) if ordinal > 0 && !base.is_empty() => (base.to_owned(), ordinal),
        _ => (stem.to_owned(), 0),
    }
}

fn read_sidecar(
    root: &Path,
    file: &DiscoveredFile,
    warnings: &mut Vec<ScanWarning>,
) -> Option<Value> {
    let directory = match Dir::open_ambient_dir(root, ambient_authority()) {
        Ok(directory) => directory,
        Err(error) => {
            warnings.push(sidecar_warning(
                file,
                format!(
                    "could not open library root for metadata: {:?}",
                    error.kind()
                ),
            ));
            return None;
        }
    };
    let opened = match directory.open(&file.relative_path) {
        Ok(opened) => opened,
        Err(error) => {
            warnings.push(sidecar_warning(
                file,
                format!("could not open metadata sidecar: {:?}", error.kind()),
            ));
            return None;
        }
    };
    let mut bytes = Vec::with_capacity(
        usize::try_from(file.byte_size.max(0))
            .unwrap_or_default()
            .min(MAX_SIDECAR_BYTES as usize),
    );
    if let Err(error) = opened
        .take((MAX_SIDECAR_BYTES as u64) + 1)
        .read_to_end(&mut bytes)
    {
        warnings.push(sidecar_warning(
            file,
            format!("could not read metadata sidecar: {:?}", error.kind()),
        ));
        return None;
    }
    if bytes.len() > MAX_SIDECAR_BYTES as usize {
        warnings.push(sidecar_warning(
            file,
            "metadata sidecar exceeds the 4 MiB read limit".into(),
        ));
        return None;
    }
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(Value::Object(object)) => Some(Value::Object(object)),
        Ok(_) => {
            warnings.push(sidecar_warning(
                file,
                "metadata sidecar is not a JSON object".into(),
            ));
            None
        }
        Err(error) => {
            warnings.push(sidecar_warning(
                file,
                format!("could not parse metadata sidecar: {error}"),
            ));
            None
        }
    }
}

fn sidecar_warning(file: &DiscoveredFile, message: String) -> ScanWarning {
    ScanWarning {
        relative_path: Some(file.relative_path.clone()),
        message,
    }
}

fn infer_kind(path: &Path) -> MediaItemKind {
    if path
        .components()
        .any(|component| component_eq(component, "stories"))
    {
        MediaItemKind::Story
    } else if path
        .components()
        .any(|component| component_eq(component, "reels"))
    {
        MediaItemKind::Reel
    } else if path
        .components()
        .any(|component| component_eq(component, "avatars") || component_eq(component, "avatar"))
        || path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
            .is_some_and(|stem| stem.starts_with("avatar_"))
    {
        MediaItemKind::Avatar
    } else {
        MediaItemKind::Post
    }
}

fn component_eq(component: Component<'_>, expected: &str) -> bool {
    matches!(component, Component::Normal(value) if value.to_string_lossy().eq_ignore_ascii_case(expected))
}

pub(crate) fn path_identity(path: &Path) -> Option<(MediaItemKind, String)> {
    let stem = path.file_stem()?.to_string_lossy();
    let kind = infer_kind(path);
    let pk = match kind {
        MediaItemKind::Story => {
            let (timestamp, pk) = stem.rsplit_once('_')?;
            let valid_timestamp = valid_story_timestamp(timestamp);
            let valid_pk = !pk.is_empty() && pk.bytes().all(|byte| byte.is_ascii_digit());
            (valid_timestamp && valid_pk).then_some(pk)
        }
        MediaItemKind::Avatar => stem
            .strip_prefix("avatar_")
            .or_else(|| stem.strip_prefix("Avatar_")),
        MediaItemKind::Post | MediaItemKind::Reel => None,
    }?;
    valid_remote_pk(pk).then(|| (kind, pk.to_owned()))
}

fn valid_story_timestamp(timestamp: &str) -> bool {
    if timestamp.bytes().all(|byte| byte.is_ascii_digit())
        && timestamp
            .parse::<i64>()
            .is_ok_and(|timestamp| timestamp > 0)
    {
        return true;
    }

    let bytes = timestamp.as_bytes();
    let exact_writer_shape = bytes.len() == 19
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'_'
        && bytes[13] == b'-'
        && bytes[16] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7 | 10 | 13 | 16) || byte.is_ascii_digit());
    exact_writer_shape && NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d_%H-%M-%S").is_ok()
}

fn carousel_identity_path_cancellable(
    files: &[DiscoveredFile],
    should_cancel: &mut impl FnMut() -> bool,
) -> Result<Option<PathBuf>, ScanError> {
    let mut media_count = 0_usize;
    let mut parent = None::<PathBuf>;
    let mut base = None::<String>;
    let mut has_numeric_suffix = false;
    for file in files {
        if should_cancel() {
            return Err(ScanError::Cancelled);
        }
        if extension_is(&file.relative_path, "json") {
            continue;
        }
        let candidate_parent = file.relative_path.parent().unwrap_or_else(|| Path::new(""));
        let Some(stem) = file.relative_path.file_stem() else {
            return Ok(None);
        };
        let (candidate_base, suffix) = carousel_base(&stem.to_string_lossy());
        if media_count == 0 {
            parent = Some(candidate_parent.to_path_buf());
            base = Some(candidate_base);
        } else if parent.as_deref() != Some(candidate_parent)
            || base.as_deref() != Some(candidate_base.as_str())
        {
            return Ok(None);
        }
        media_count += 1;
        has_numeric_suffix |= suffix > 0;
    }
    if media_count < 2 || !has_numeric_suffix {
        return Ok(None);
    }
    Ok(Some(
        parent.unwrap_or_default().join(base.unwrap_or_default()),
    ))
}

fn nonnegative_count_cancellable(
    sidecar: &Option<Value>,
    field: &str,
    files: &[DiscoveredFile],
    warnings: &mut Vec<ScanWarning>,
    should_cancel: &mut impl FnMut() -> bool,
) -> Result<Option<i64>, ScanError> {
    let Some(value) = sidecar
        .as_ref()
        .and_then(|sidecar| sidecar.get(field))
        .and_then(value_i64)
    else {
        return Ok(None);
    };
    if value < 0 {
        for file in files {
            if should_cancel() {
                return Err(ScanError::Cancelled);
            }
            if extension_is(&file.relative_path, "json") {
                warnings.push(sidecar_warning(
                    file,
                    format!("metadata field {field} must not be negative; ignored"),
                ));
                break;
            }
        }
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn remote_prefix(kind: MediaItemKind) -> &'static str {
    match kind {
        MediaItemKind::Post => "post",
        MediaItemKind::Reel => "reel",
        MediaItemKind::Story => "story",
        MediaItemKind::Avatar => "avatar",
    }
}

fn file_kind(path: &Path) -> MediaFileKind {
    if extension_is(path, "json") {
        MediaFileKind::Metadata
    } else if extension_is(path, "mp4") || extension_is(path, "mov") {
        MediaFileKind::Video
    } else if ["jpg", "jpeg", "png", "webp"]
        .iter()
        .any(|extension| extension_is(path, extension))
    {
        MediaFileKind::Photo
    } else {
        MediaFileKind::Unknown
    }
}

fn sidecar_string(sidecar: &Value, field: &str) -> Option<String> {
    value_string(sidecar.get(field))
}

fn value_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn percent_encode_relative(path: &Path) -> String {
    let normalized = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    utf8_percent_encode(&normalized, NON_ALPHANUMERIC).to_string()
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or_default()
}
