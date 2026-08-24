use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use insta_dl_gui_lib::catalog::{Catalog, MediaFileKind, MediaItemKind};
use insta_dl_gui_lib::scanner::parser::{carousel_base, parse_group};
#[cfg(target_os = "macos")]
use insta_dl_gui_lib::scanner::walk::validate_catalog_relative_path;
use insta_dl_gui_lib::scanner::{discover_archive, DiscoveredFile, ScanError};
use tempfile::TempDir;

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    bytes: Option<Vec<u8>>,
    len: u64,
    modified: SystemTime,
    kind: &'static str,
    symlink_target: Option<PathBuf>,
}

fn fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    for directory in ["posts", "stories", "avatars", "misc", ".catalog"] {
        fs::create_dir_all(root.path().join(directory)).unwrap();
    }
    root
}

fn write(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, bytes).unwrap();
    path
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Snapshot> {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.depth() > 0)
        .map(|entry| {
            let path = entry.path();
            let metadata = fs::symlink_metadata(path).unwrap();
            let file_type = metadata.file_type();
            (
                path.strip_prefix(root).unwrap().to_path_buf(),
                Snapshot {
                    bytes: file_type.is_file().then(|| fs::read(path).unwrap()),
                    len: metadata.len(),
                    modified: metadata.modified().unwrap(),
                    kind: if file_type.is_file() {
                        "file"
                    } else if file_type.is_dir() {
                        "directory"
                    } else if file_type.is_symlink() {
                        "symlink"
                    } else {
                        "other"
                    },
                    symlink_target: file_type.is_symlink().then(|| fs::read_link(path).unwrap()),
                },
            )
        })
        .collect()
}

fn discovered_file(root: &Path, relative: &str, ordinal: i64) -> DiscoveredFile {
    let metadata = fs::metadata(root.join(relative)).unwrap();
    DiscoveredFile {
        relative_path: PathBuf::from(relative),
        byte_size: i64::try_from(metadata.len()).unwrap(),
        mtime: 1,
        ordinal,
    }
}

#[test]
fn scanner_parses_current_sidecar_shape_and_preserves_the_archive() {
    let root = fixture();
    let media = "posts/2026-04-21_19-04-15_DXZlTiKEpxw.mp4";
    let sidecar = "posts/2026-04-21_19-04-15_DXZlTiKEpxw.json";
    write(root.path(), media, b"video bytes");
    write(
        root.path(),
        sidecar,
        br#"{
          "pk": "987654321",
          "code": "DXZlTiKEpxw",
          "taken_at": 1776787455,
          "caption": "archive caption",
          "like_count": 42,
          "comment_count": 3,
          "owner": {"pk": "1234", "username": "archive_owner"}
        }"#,
    );
    let before = snapshot(root.path());

    let discovery = discover_archive(7, root.path()).unwrap();

    assert!(discovery.complete);
    assert!(discovery.warnings.is_empty());
    assert_eq!(discovery.groups.len(), 1);
    let group = &discovery.groups[0];
    assert_eq!(group.remote_key, "post:987654321");
    assert_eq!(group.item.remote_key, group.remote_key);
    assert_eq!(group.item.kind, MediaItemKind::Post);
    assert_eq!(group.item.remote_pk.as_deref(), Some("987654321"));
    assert_eq!(group.item.shortcode.as_deref(), Some("DXZlTiKEpxw"));
    assert_eq!(group.item.taken_at, Some(1776787455));
    assert_eq!(group.item.caption.as_deref(), Some("archive caption"));
    assert_eq!(group.item.like_count, Some(42));
    assert_eq!(group.item.comment_count, Some(3));
    assert_eq!(group.item.owner_pk.as_deref(), Some("1234"));
    assert_eq!(group.item.owner_username.as_deref(), Some("archive_owner"));
    assert_eq!(group.item.files.len(), 2);
    assert_eq!(group.item.files[0].relative_path, PathBuf::from(media));
    assert_eq!(group.item.files[0].kind, MediaFileKind::Video);
    assert_eq!(group.item.files[0].ordinal, 0);
    assert_eq!(group.item.files[1].relative_path, PathBuf::from(sidecar));
    assert_eq!(group.item.files[1].kind, MediaFileKind::Metadata);
    assert_eq!(snapshot(root.path()), before);
}

#[test]
fn scanner_groups_carousel_siblings_but_not_a_lone_numbered_file() {
    let root = fixture();
    for suffix in [3, 1, 2] {
        write(
            root.path(),
            &format!("posts/carousel_{suffix}.JpG"),
            format!("image {suffix}").as_bytes(),
        );
    }
    write(root.path(), "misc/kept_as_one_1.png", b"lone image");

    let discovery = discover_archive(11, root.path()).unwrap();

    let carousel = discovery
        .groups
        .iter()
        .find(|group| group.item.files.len() == 3)
        .unwrap();
    assert_eq!(
        carousel
            .item
            .files
            .iter()
            .map(|file| (file.relative_path.clone(), file.ordinal))
            .collect::<Vec<_>>(),
        vec![
            (PathBuf::from("posts/carousel_1.JpG"), 0),
            (PathBuf::from("posts/carousel_2.JpG"), 1),
            (PathBuf::from("posts/carousel_3.JpG"), 2),
        ]
    );

    let lone = discovery
        .groups
        .iter()
        .find(|group| group.item.files[0].relative_path == Path::new("misc/kept_as_one_1.png"))
        .unwrap();
    assert_eq!(lone.item.files[0].ordinal, 0);
    assert_eq!(carousel_base("kept_as_one_1"), ("kept_as_one".into(), 1));
    assert_eq!(
        carousel_base("not_numbered_0"),
        ("not_numbered_0".into(), 0)
    );
}

#[test]
fn scanner_infers_story_avatar_and_percent_encoded_local_keys() {
    let root = fixture();
    write(root.path(), "stories/1776787455_998877.jpg", b"story");
    write(root.path(), "avatars/avatar_445566.jpg", b"avatar");
    write(root.path(), "misc/space name.webp", b"unknown");

    let discovery = discover_archive(23, root.path()).unwrap();
    let mut keys = discovery
        .groups
        .iter()
        .map(|group| (group.remote_key.as_str(), group.item.kind))
        .collect::<Vec<_>>();
    keys.sort_by_key(|(key, _)| *key);

    assert!(keys.contains(&("story:998877", MediaItemKind::Story)));
    assert!(keys.contains(&("avatar:445566", MediaItemKind::Avatar)));
    assert!(keys.contains(&("local:23:misc%2Fspace%20name%2Ewebp", MediaItemKind::Post,)));
    for group in &discovery.groups {
        assert!(!group
            .remote_key
            .contains(root.path().to_string_lossy().as_ref()));
        assert!(!group
            .item
            .caption
            .as_deref()
            .unwrap_or_default()
            .contains(root.path().to_string_lossy().as_ref()));
    }
}

#[test]
fn scanner_keeps_story_and_avatar_identities_separate_from_carousel_suffixes() {
    let root = fixture();
    for relative in [
        "stories/1776787455_998877.jpg",
        "stories/1776787455_112233.jpg",
        "avatars/avatar_445566.jpg",
        "avatars/avatar_778899.jpg",
    ] {
        write(root.path(), relative, relative.as_bytes());
    }

    let discovery = discover_archive(31, root.path()).unwrap();
    let keys = discovery
        .groups
        .iter()
        .map(|group| group.remote_key.as_str())
        .collect::<Vec<_>>();

    assert_eq!(discovery.groups.len(), 4);
    for expected in [
        "story:998877",
        "story:112233",
        "avatar:445566",
        "avatar:778899",
    ] {
        assert!(keys.contains(&expected), "missing identity {expected}");
    }
    assert!(discovery
        .groups
        .iter()
        .all(|group| group.item.files.len() == 1 && group.item.files[0].ordinal == 0));
}

#[test]
fn scanner_requires_strict_numeric_story_identity() {
    let root = fixture();
    for relative in [
        "stories/cat_1.jpg",
        "stories/dog_1.jpg",
        "stories/1776787455_998877.jpg",
        "stories/2026-04-21_19-04-15_998877.jpg",
        "stories/2026-99-99_99-99-99_112233.jpg",
    ] {
        write(root.path(), relative, relative.as_bytes());
    }

    let discovery = discover_archive(41, root.path()).unwrap();
    let keys = discovery
        .groups
        .iter()
        .map(|group| group.remote_key.as_str())
        .collect::<Vec<_>>();

    assert_eq!(discovery.groups.len(), 5);
    assert!(keys.contains(&"story:998877"));
    assert!(discovery.groups.iter().any(|group| {
        group.remote_key == "story:998877"
            && group.item.files[0].relative_path
                == Path::new("stories/2026-04-21_19-04-15_998877.jpg")
    }));
    assert!(keys.contains(&"local:41:stories%2Fcat%5F1%2Ejpg"));
    assert!(keys.contains(&"local:41:stories%2Fdog%5F1%2Ejpg"));
    assert_eq!(keys.iter().filter(|key| **key == "story:1").count(), 0);
    assert_eq!(keys.iter().filter(|key| **key == "story:112233").count(), 0);
    assert!(discovery.groups.iter().any(|group| {
        group.remote_key.starts_with("local:41:")
            && group.item.files[0].relative_path
                == Path::new("stories/2026-99-99_99-99-99_112233.jpg")
    }));
}

#[test]
fn scanner_keeps_sidecarless_carousel_identity_when_the_first_file_disappears() {
    let root = fixture();
    for suffix in [1, 2, 3] {
        write(
            root.path(),
            &format!("posts/archive_carousel_{suffix}.jpg"),
            format!("resource {suffix}").as_bytes(),
        );
    }

    let initial = discover_archive(43, root.path()).unwrap();
    let initial_group = initial
        .groups
        .iter()
        .find(|group| group.item.files.len() == 3)
        .unwrap();
    let initial_key = initial_group.remote_key.clone();
    assert_eq!(
        initial_group
            .item
            .files
            .iter()
            .map(|file| file.ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );

    fs::remove_file(root.path().join("posts/archive_carousel_1.jpg")).unwrap();
    let rescanned = discover_archive(43, root.path()).unwrap();
    let remaining = rescanned
        .groups
        .iter()
        .find(|group| group.item.files.len() == 2)
        .unwrap();

    assert_eq!(remaining.remote_key, initial_key);
    assert_eq!(
        remaining
            .item
            .files
            .iter()
            .map(|file| file.ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn scanner_bounds_actual_sidecar_reads_even_when_walk_metadata_is_stale() {
    const MAX_SIDECAR_BYTES: usize = 4 * 1024 * 1024;

    let root = fixture();
    write(root.path(), "posts/oversized.jpg", b"photo");
    let mut sidecar = br#"{"pk":"must-not-parse","caption":""#.to_vec();
    sidecar.extend(std::iter::repeat_n(b'x', MAX_SIDECAR_BYTES));
    sidecar.extend_from_slice(br#""}"#);
    write(root.path(), "posts/oversized.json", &sidecar);
    let media = discovered_file(root.path(), "posts/oversized.jpg", 0);
    let mut metadata = discovered_file(root.path(), "posts/oversized.json", 0);
    metadata.byte_size = 1;

    let group = parse_group(47, root.path(), &[media, metadata]);

    assert_eq!(group.warnings.len(), 1);
    assert!(group.warnings[0].message.contains("4 MiB"));
    assert_eq!(group.item.remote_pk, None);
    assert_eq!(group.item.caption, None);
    assert!(!group.remote_key.contains("must-not-parse"));
}

#[cfg(unix)]
#[test]
fn scanner_sidecar_reader_rejects_external_and_retargeted_symlinks() {
    use std::os::unix::fs::symlink;

    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    write(root.path(), "posts/static.jpg", b"photo");
    write(root.path(), "posts/retargeted.jpg", b"photo");
    write(
        outside.path(),
        "secret.json",
        br#"{"pk":"outside-secret","caption":"never expose me"}"#,
    );
    write(
        root.path(),
        "posts/inside.json",
        br#"{"pk":"inside-before-retarget"}"#,
    );
    symlink(
        outside.path().join("secret.json"),
        root.path().join("posts/static.json"),
    )
    .unwrap();
    symlink(
        root.path().join("posts/inside.json"),
        root.path().join("posts/retargeted.json"),
    )
    .unwrap();

    let static_files = [
        discovered_file(root.path(), "posts/static.jpg", 0),
        discovered_file(root.path(), "posts/static.json", 0),
    ];
    let retargeted_media = discovered_file(root.path(), "posts/retargeted.jpg", 0);
    let retargeted_metadata = discovered_file(root.path(), "posts/retargeted.json", 0);
    fs::remove_file(root.path().join("posts/retargeted.json")).unwrap();
    symlink(
        outside.path().join("secret.json"),
        root.path().join("posts/retargeted.json"),
    )
    .unwrap();

    for group in [
        parse_group(53, root.path(), &static_files),
        parse_group(53, root.path(), &[retargeted_media, retargeted_metadata]),
    ] {
        assert_eq!(group.warnings.len(), 1);
        assert_eq!(group.item.remote_pk, None);
        assert_eq!(group.item.caption, None);
        assert!(!group.remote_key.contains("outside-secret"));
        assert!(!group.warnings[0].message.contains("outside-secret"));
        assert!(!group.warnings[0].message.contains("never expose me"));
        assert!(!group.warnings[0]
            .message
            .contains(root.path().to_string_lossy().as_ref()));
        assert!(!group.warnings[0]
            .message
            .contains(outside.path().to_string_lossy().as_ref()));
    }
}

#[test]
fn scanner_sanitizes_negative_sidecar_counts_before_catalog_upsert() {
    let outer = tempfile::tempdir().unwrap();
    let library = outer.path().join("library");
    fs::create_dir(&library).unwrap();
    write(&library, "posts/negative.jpg", b"photo");
    write(
        &library,
        "posts/negative.json",
        br#"{
          "pk":"negative-counts",
          "code":"kept-code",
          "caption":"kept caption",
          "like_count":-1,
          "comment_count":-2,
          "owner":{"pk":"7","username":"kept-owner"}
        }"#,
    );
    let catalog = Catalog::open(outer.path().join("catalog.sqlite3")).unwrap();
    let root = catalog.register_root(&library, "Archive").unwrap();

    let discovery = discover_archive(root.id, &library).unwrap();
    let group = &discovery.groups[0];

    assert_eq!(discovery.warnings.len(), 2);
    assert_eq!(group.item.like_count, None);
    assert_eq!(group.item.comment_count, None);
    assert_eq!(group.item.shortcode.as_deref(), Some("kept-code"));
    assert_eq!(group.item.caption.as_deref(), Some("kept caption"));
    assert_eq!(group.item.owner_username.as_deref(), Some("kept-owner"));
    let results = catalog
        .upsert_media_batch(std::slice::from_ref(&group.item))
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn scanner_skips_non_utf8_relative_paths_before_building_catalog_inputs() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let root = fixture();
    let mut filename = b"invalid-".to_vec();
    filename.push(0xff);
    filename.extend_from_slice(b".jpg");
    let relative = PathBuf::from("posts").join(OsString::from_vec(filename));
    fs::write(root.path().join(&relative), b"image bytes").unwrap();

    let discovery = discover_archive(37, root.path()).unwrap();

    assert!(discovery.groups.is_empty());
    assert_eq!(discovery.warnings.len(), 1);
    assert_eq!(
        discovery.warnings[0].relative_path.as_deref(),
        Some(relative.as_path())
    );
    assert!(discovery.warnings[0].message.contains("valid UTF-8"));
    assert!(!discovery.warnings[0]
        .message
        .contains(root.path().to_string_lossy().as_ref()));
}

// APFS rejects invalid UTF-8 at file creation with EILSEQ. Exercise the exact
// walker validation hook here; the discovery regression above runs on Unix
// filesystems that can actually contain such an entry.
#[cfg(target_os = "macos")]
#[test]
fn scanner_rejects_non_utf8_relative_paths_before_walking_on_macos() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let mut filename = b"invalid-".to_vec();
    filename.push(0xff);
    filename.extend_from_slice(b".jpg");
    let relative = PathBuf::from("posts").join(OsString::from_vec(filename));

    let warning = validate_catalog_relative_path(&relative).unwrap_err();

    assert_eq!(warning.relative_path.as_deref(), Some(relative.as_path()));
    assert!(warning.message.contains("valid UTF-8"));
    assert!(!warning.message.contains('/'));
}

#[test]
fn scanner_warns_once_for_malformed_sidecar_and_still_discovers_media() {
    let root = fixture();
    write(root.path(), "posts/broken.mp4", b"video");
    write(root.path(), "posts/broken.json", br#"{"pk": "oops""#);

    let discovery = discover_archive(3, root.path()).unwrap();

    assert_eq!(discovery.groups.len(), 1);
    assert_eq!(discovery.warnings.len(), 1);
    assert_eq!(discovery.groups[0].warnings.len(), 1);
    assert_eq!(
        discovery.groups[0].item.files[0].relative_path,
        PathBuf::from("posts/broken.mp4")
    );
    assert!(discovery.groups[0].remote_key.starts_with("local:3:"));
}

#[test]
fn scanner_ignores_partial_unsupported_and_hidden_catalog_files() {
    let root = fixture();
    write(root.path(), "posts/complete.MOV", b"movie");
    write(root.path(), "posts/incomplete.mp4.part", b"partial");
    write(root.path(), "posts/readme.txt", b"not media");
    write(root.path(), ".catalog/preview.jpg", b"catalog-owned");
    write(root.path(), ".hidden.jpg", b"hidden");

    let discovery = discover_archive(5, root.path()).unwrap();

    assert_eq!(discovery.groups.len(), 1);
    assert_eq!(
        discovery.groups[0].item.files[0].relative_path,
        PathBuf::from("posts/complete.MOV")
    );
}

#[cfg(unix)]
#[test]
fn scanner_skips_external_symlink_with_one_warning_and_keeps_internal_symlink() {
    use std::os::unix::fs::symlink;

    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    let outside_file = write(outside.path(), "outside.jpg", b"private outside bytes");
    let inside_file = write(root.path(), "posts/original.jpg", b"inside bytes");
    symlink(&outside_file, root.path().join("posts/external.jpg")).unwrap();
    symlink(&inside_file, root.path().join("posts/internal.jpg")).unwrap();
    let before = snapshot(root.path());

    let discovery = discover_archive(29, root.path()).unwrap();

    assert_eq!(discovery.warnings.len(), 1);
    assert!(discovery.warnings[0]
        .message
        .contains("outside the library root"));
    assert!(discovery.groups.iter().any(|group| {
        group
            .item
            .files
            .iter()
            .any(|file| file.relative_path == Path::new("posts/internal.jpg"))
    }));
    assert!(!discovery.groups.iter().any(|group| {
        group
            .item
            .files
            .iter()
            .any(|file| file.relative_path == Path::new("posts/external.jpg"))
    }));
    assert_eq!(snapshot(root.path()), before);
}

#[test]
fn scanner_rejects_a_non_directory_root_without_creating_or_mutating_it() {
    let root = tempfile::tempdir().unwrap();
    let file = write(root.path(), "archive-file", b"unchanged");
    let before = fs::read(&file).unwrap();

    let error = discover_archive(1, &file).unwrap_err();

    assert!(matches!(error, ScanError::InvalidRoot { .. }));
    assert_eq!(fs::read(&file).unwrap(), before);
}
