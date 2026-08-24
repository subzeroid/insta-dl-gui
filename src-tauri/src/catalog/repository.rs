use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use rusqlite::types::{Type, Value};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, TransactionBehavior};

use super::models::{
    CatalogMediaInput, CatalogRecoveryFile, FileAvailability, LibraryCard, LibraryCursor,
    LibraryCursorScope, LibraryFile, LibraryItemDetail, LibraryPage, LibraryPreview, LibraryQuery,
    LibraryRoot, LibrarySort, MediaFileKind, MediaItemKind, ResolvedCatalogFile, UpsertDisposition,
    UpsertResult,
};
use super::{Catalog, CatalogError};

const MAX_BATCH_SIZE: usize = 100;
const DEFAULT_PAGE_SIZE: u32 = 60;
const MAX_PAGE_SIZE: u32 = 100;
const LIBRARY_MEDIA_FROM: &str = "media_items mi";

pub fn local_remote_key(root_id: i64, relative_path: &Path) -> Result<String, CatalogError> {
    Ok(format!(
        "local:{root_id}:{}",
        normalize_relative_path(relative_path)?
    ))
}

fn normalize_relative_path(path: &Path) -> Result<String, CatalogError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(CatalogError::InvalidRelativePath {
            path: path.to_path_buf(),
        });
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_str().ok_or_else(|| CatalogError::NonUtf8Path {
                    path: path.to_path_buf(),
                })?;
                parts.push(part.to_owned());
            }
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => {
                return Err(CatalogError::InvalidRelativePath {
                    path: path.to_path_buf(),
                });
            }
        }
    }
    if parts.is_empty() || parts.iter().any(String::is_empty) {
        return Err(CatalogError::InvalidRelativePath {
            path: path.to_path_buf(),
        });
    }
    Ok(parts.join("/"))
}

fn sql_error(operation: &'static str, source: rusqlite::Error) -> CatalogError {
    CatalogError::Sql { operation, source }
}

fn invalid_db_value(index: usize, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid catalog enum value {value:?}"),
        )),
    )
}

fn media_kind_at(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<MediaItemKind> {
    let value: String = row.get(index)?;
    MediaItemKind::from_db(&value).ok_or_else(|| invalid_db_value(index, &value))
}

fn file_kind_at(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<MediaFileKind> {
    let value: String = row.get(index)?;
    MediaFileKind::from_db(&value).ok_or_else(|| invalid_db_value(index, &value))
}

fn root_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryRoot> {
    Ok(LibraryRoot {
        id: row.get(0)?,
        path: PathBuf::from(row.get::<_, String>(1)?),
        label: row.get(2)?,
        created_at: row.get(3)?,
        last_scan_started_at: row.get(4)?,
        last_scan_completed_at: row.get(5)?,
    })
}

impl Catalog {
    pub fn register_root(&self, path: &Path, label: &str) -> Result<LibraryRoot, CatalogError> {
        let label = label.trim();
        if label.is_empty() {
            return Err(CatalogError::InvalidInput {
                message: "root label must not be empty".into(),
            });
        }
        if path.to_str().is_none() {
            return Err(CatalogError::NonUtf8Path {
                path: path.to_path_buf(),
            });
        }
        std::fs::create_dir_all(path).map_err(|source| CatalogError::Io {
            operation: "creating root directory",
            path: path.to_path_buf(),
            source,
        })?;
        let canonical = path.canonicalize().map_err(|source| CatalogError::Io {
            operation: "canonicalizing root directory",
            path: path.to_path_buf(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(CatalogError::InvalidInput {
                message: format!("root is not a directory: {}", canonical.display()),
            });
        }
        let stored_path = canonical
            .to_str()
            .ok_or_else(|| CatalogError::NonUtf8Path {
                path: canonical.clone(),
            })?
            .to_owned();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let mut conn = self.connect()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| sql_error("starting root registration", source))?;
        transaction
            .execute(
                "INSERT INTO library_roots(path, label, created_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(path) DO NOTHING",
                params![stored_path, label, created_at],
            )
            .map_err(|source| sql_error("registering library root", source))?;
        let root = transaction
            .query_row(
                "SELECT id, path, label, created_at, last_scan_started_at, last_scan_completed_at
                 FROM library_roots WHERE path = ?1",
                [&stored_path],
                root_from_row,
            )
            .map_err(|source| sql_error("looking up registered library root", source))?;
        transaction
            .commit()
            .map_err(|source| sql_error("committing root registration", source))?;
        Ok(root)
    }

    pub fn list_roots(&self) -> Result<Vec<LibraryRoot>, CatalogError> {
        let conn = self.connect()?;
        let mut statement = conn
            .prepare(
                "SELECT id, path, label, created_at, last_scan_started_at, last_scan_completed_at
                 FROM library_roots ORDER BY id",
            )
            .map_err(|source| sql_error("preparing root list", source))?;
        let roots = statement
            .query_map([], root_from_row)
            .map_err(|source| sql_error("listing library roots", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sql_error("reading library roots", source))?;
        Ok(roots)
    }

    pub fn begin_scan(&self, root_id: i64, now: i64) -> Result<(), CatalogError> {
        self.update_scan_time(root_id, now, "last_scan_started_at")
    }

    pub fn finish_scan(&self, root_id: i64, now: i64) -> Result<(), CatalogError> {
        self.update_scan_time(root_id, now, "last_scan_completed_at")
    }

    fn update_scan_time(
        &self,
        root_id: i64,
        now: i64,
        column: &'static str,
    ) -> Result<(), CatalogError> {
        let conn = self.connect()?;
        let sql = format!("UPDATE library_roots SET {column} = ?1 WHERE id = ?2");
        let changed = conn
            .execute(&sql, params![now, root_id])
            .map_err(|source| sql_error("updating root scan timestamp", source))?;
        if changed == 0 {
            return Err(CatalogError::NotFound {
                entity: "library root",
                id: root_id,
            });
        }
        Ok(())
    }

    pub fn upsert_media(&self, input: &CatalogMediaInput) -> Result<UpsertResult, CatalogError> {
        let mut results = self.upsert_media_batch(std::slice::from_ref(input))?;
        Ok(results.remove(0))
    }

    pub fn upsert_media_batch(
        &self,
        inputs: &[CatalogMediaInput],
    ) -> Result<Vec<UpsertResult>, CatalogError> {
        self.upsert_media_batch_cancellable(inputs, || false)
    }

    pub fn upsert_media_batch_cancellable(
        &self,
        inputs: &[CatalogMediaInput],
        should_cancel: impl FnOnce() -> bool,
    ) -> Result<Vec<UpsertResult>, CatalogError> {
        if inputs.len() > MAX_BATCH_SIZE {
            return Err(CatalogError::BatchTooLarge {
                size: inputs.len(),
                max: MAX_BATCH_SIZE,
            });
        }
        let validated: Vec<Vec<String>> = inputs
            .iter()
            .map(validate_media_input)
            .collect::<Result<_, _>>()?;
        let mut claimed_paths = BTreeSet::new();
        for (input, relative_paths) in inputs.iter().zip(&validated) {
            for (file, relative_path) in input.files.iter().zip(relative_paths) {
                if !claimed_paths.insert((file.root_id, relative_path.clone())) {
                    return Err(CatalogError::InvalidInput {
                        message: format!(
                            "duplicate file claim for root {} path {relative_path}",
                            file.root_id
                        ),
                    });
                }
            }
        }
        let mut conn = self.connect()?;
        let root_ids: BTreeSet<i64> = inputs
            .iter()
            .flat_map(|input| input.files.iter().map(|file| file.root_id))
            .collect();
        for root_id in root_ids {
            ensure_root_exists(&conn, root_id)?;
        }
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| sql_error("starting media upsert", source))?;
        let mut results = Vec::with_capacity(inputs.len());
        for (input, relative_paths) in inputs.iter().zip(&validated) {
            results.push(upsert_one(&transaction, input, relative_paths)?);
        }
        if should_cancel() {
            return Err(CatalogError::Cancelled {
                operation: "committing media upsert",
            });
        }
        transaction
            .commit()
            .map_err(|source| sql_error("committing media upsert", source))?;
        Ok(results)
    }

    pub fn mark_unseen_missing(
        &self,
        root_id: i64,
        scan_started_at: i64,
    ) -> Result<u64, CatalogError> {
        let conn = self.connect()?;
        ensure_root_exists(&conn, root_id)?;
        let changed = conn
            .execute(
                "UPDATE media_files SET exists_on_disk = 0
                 WHERE library_root_id = ?1 AND last_seen_at < ?2 AND exists_on_disk = 1",
                params![root_id, scan_started_at],
            )
            .map_err(|source| sql_error("marking unseen files missing", source))?;
        Ok(changed as u64)
    }

    pub fn finalize_scan_cancellable(
        &self,
        root_id: i64,
        scan_started_at: i64,
        scan_completed_at: i64,
        should_cancel: impl FnOnce() -> bool,
    ) -> Result<u64, CatalogError> {
        let mut conn = self.connect()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| sql_error("starting scan finalization", source))?;
        ensure_root_exists(&transaction, root_id)?;
        let missing = transaction
            .execute(
                "UPDATE media_files SET exists_on_disk = 0
                 WHERE library_root_id = ?1 AND last_seen_at < ?2 AND exists_on_disk = 1",
                params![root_id, scan_started_at],
            )
            .map_err(|source| sql_error("reconciling unseen files", source))?;
        let changed = transaction
            .execute(
                "UPDATE library_roots SET last_scan_completed_at = ?1 WHERE id = ?2",
                params![scan_completed_at, root_id],
            )
            .map_err(|source| sql_error("recording scan completion", source))?;
        if changed == 0 {
            return Err(CatalogError::NotFound {
                entity: "library root",
                id: root_id,
            });
        }
        if should_cancel() {
            return Err(CatalogError::Cancelled {
                operation: "committing scan finalization",
            });
        }
        transaction
            .commit()
            .map_err(|source| sql_error("committing scan finalization", source))?;
        Ok(missing as u64)
    }

    pub fn query_library(&self, query: &LibraryQuery) -> Result<LibraryPage, CatalogError> {
        if query.source_id.is_some_and(|source_id| source_id <= 0) {
            return Err(CatalogError::InvalidInput {
                message: "query source_id must be positive".into(),
            });
        }
        let scope = cursor_scope(query);
        let cursor = query
            .cursor
            .as_deref()
            .map(|cursor| decode_cursor(cursor, &scope))
            .transpose()?;
        let effective_limit = if query.limit == 0 {
            DEFAULT_PAGE_SIZE
        } else {
            query.limit.min(MAX_PAGE_SIZE)
        } as usize;
        let sort_expression = match query.sort {
            LibrarySort::TakenAtDesc => "COALESCE(mi.taken_at, mi.imported_at)",
            LibrarySort::ImportedAtDesc => "mi.imported_at",
        };
        let mut sql = format!(
            "SELECT mi.id, mi.remote_key, mi.kind, mi.shortcode, mi.owner_username,
                    mi.taken_at, mi.caption, mi.imported_at, mi.updated_at,
                    (SELECT COUNT(*) FROM media_files mf
                     WHERE mf.media_item_id = mi.id AND mf.kind IN ('photo', 'video')),
                    EXISTS(SELECT 1 FROM media_files mf WHERE mf.media_item_id = mi.id AND mf.exists_on_disk = 1),
                    {sort_expression} AS sort_value
             FROM {LIBRARY_MEDIA_FROM}
             WHERE EXISTS(SELECT 1 FROM media_files mf_any WHERE mf_any.media_item_id = mi.id)"
        );
        let mut values = Vec::<Value>::new();
        if let Some(search) = scope.search.as_deref() {
            sql.push_str(" AND mi.id IN (SELECT rowid FROM media_fts WHERE media_fts MATCH ?)");
            values.push(Value::Text(search.to_owned()));
        }
        if !scope.kinds.is_empty() {
            sql.push_str(" AND mi.kind IN (");
            sql.push_str(&vec!["?"; scope.kinds.len()].join(","));
            sql.push(')');
            values.extend(
                scope
                    .kinds
                    .iter()
                    .map(|kind| Value::Text(kind.as_str().into())),
            );
        }
        if let Some(source_id) = query.source_id {
            sql.push_str(" AND EXISTS(SELECT 1 FROM source_media sm WHERE sm.media_item_id = mi.id AND sm.source_id = ?)");
            values.push(source_id.into());
        }
        if let Some(taken_after) = query.taken_after {
            sql.push_str(" AND mi.taken_at >= ?");
            values.push(taken_after.into());
        }
        if let Some(taken_before) = query.taken_before {
            sql.push_str(" AND mi.taken_at <= ?");
            values.push(taken_before.into());
        }
        match query.availability {
            Some(FileAvailability::Available) => sql.push_str(
                " AND EXISTS(SELECT 1 FROM media_files mf WHERE mf.media_item_id = mi.id AND mf.exists_on_disk = 1)",
            ),
            Some(FileAvailability::Missing) => sql.push_str(
                " AND EXISTS(SELECT 1 FROM media_files mf WHERE mf.media_item_id = mi.id)
                  AND NOT EXISTS(SELECT 1 FROM media_files mf WHERE mf.media_item_id = mi.id AND mf.exists_on_disk = 1)",
            ),
            None => {}
        }
        if let Some(cursor) = cursor {
            sql.push_str(&format!(
                " AND ({sort_expression} < ? OR ({sort_expression} = ? AND mi.id < ?))"
            ));
            values.push(cursor.sort_value.into());
            values.push(cursor.sort_value.into());
            values.push(cursor.id.into());
        }
        sql.push_str(&format!(
            " ORDER BY {sort_expression} DESC, mi.id DESC LIMIT ?"
        ));
        values.push(((effective_limit + 1) as i64).into());

        let conn = self.connect()?;
        let rows = {
            let mut statement = conn
                .prepare(&sql)
                .map_err(|source| sql_error("preparing library query", source))?;
            let rows = statement
                .query_map(params_from_iter(values.iter()), |row| {
                    let available: bool = row.get(10)?;
                    Ok((
                        LibraryCard {
                            id: row.get(0)?,
                            remote_key: row.get(1)?,
                            kind: media_kind_at(row, 2)?,
                            shortcode: row.get(3)?,
                            owner_username: row.get(4)?,
                            taken_at: row.get(5)?,
                            caption: row.get(6)?,
                            imported_at: row.get(7)?,
                            updated_at: row.get(8)?,
                            preview: None,
                            resource_count: row.get::<_, i64>(9)? as u32,
                            availability: if available {
                                FileAvailability::Available
                            } else {
                                FileAvailability::Missing
                            },
                        },
                        row.get::<_, i64>(11)?,
                    ))
                })
                .map_err(|source| sql_error("querying library", source))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|source| sql_error("reading library query", source))?;
            rows
        };
        let has_more = rows.len() > effective_limit;
        let mut rows = rows;
        rows.truncate(effective_limit);
        for (card, _) in &mut rows {
            card.preview = preferred_preview(&conn, card.id)?;
        }
        let next_cursor = if has_more {
            rows.last()
                .map(|(card, sort_value)| encode_cursor(*sort_value, card.id, scope.clone()))
                .transpose()?
        } else {
            None
        };
        Ok(LibraryPage {
            items: rows.into_iter().map(|(card, _)| card).collect(),
            next_cursor,
        })
    }

    pub fn get_library_item(&self, id: i64) -> Result<Option<LibraryItemDetail>, CatalogError> {
        let conn = self.connect()?;
        let item = conn
            .query_row(
                "SELECT id, remote_key, kind, remote_pk, shortcode, owner_pk, owner_username,
                        taken_at, caption, like_count, comment_count, imported_at, updated_at
                 FROM media_items WHERE id = ?1",
                [id],
                |row| {
                    Ok(LibraryItemDetail {
                        id: row.get(0)?,
                        remote_key: row.get(1)?,
                        kind: media_kind_at(row, 2)?,
                        remote_pk: row.get(3)?,
                        shortcode: row.get(4)?,
                        owner_pk: row.get(5)?,
                        owner_username: row.get(6)?,
                        taken_at: row.get(7)?,
                        caption: row.get(8)?,
                        like_count: row.get(9)?,
                        comment_count: row.get(10)?,
                        imported_at: row.get(11)?,
                        updated_at: row.get(12)?,
                        files: Vec::new(),
                        source_ids: Vec::new(),
                    })
                },
            )
            .optional()
            .map_err(|source| sql_error("reading library item", source))?;
        let Some(mut item) = item else {
            return Ok(None);
        };
        let mut files = conn
            .prepare(
                "SELECT id, library_root_id, relative_path, ordinal, kind, byte_size, mtime,
                        exists_on_disk, last_seen_at
                 FROM media_files WHERE media_item_id = ?1 ORDER BY ordinal, id",
            )
            .map_err(|source| sql_error("preparing library item files", source))?;
        item.files = files
            .query_map([id], |row| {
                Ok(LibraryFile {
                    id: row.get(0)?,
                    root_id: row.get(1)?,
                    relative_path: row.get(2)?,
                    ordinal: row.get(3)?,
                    kind: file_kind_at(row, 4)?,
                    byte_size: row.get(5)?,
                    mtime: row.get(6)?,
                    exists_on_disk: row.get(7)?,
                    last_seen_at: row.get(8)?,
                })
            })
            .map_err(|source| sql_error("querying library item files", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sql_error("reading library item files", source))?;
        let mut sources = conn
            .prepare(
                "SELECT source_id FROM source_media WHERE media_item_id = ?1 ORDER BY source_id",
            )
            .map_err(|source| sql_error("preparing library item sources", source))?;
        item.source_ids = sources
            .query_map([id], |row| row.get(0))
            .map_err(|source| sql_error("querying library item sources", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sql_error("reading library item sources", source))?;
        Ok(Some(item))
    }

    pub fn resolve_file(&self, file_id: i64) -> Result<ResolvedCatalogFile, CatalogError> {
        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT mf.id, mf.media_item_id, mf.library_root_id, lr.path, mf.relative_path,
                        mf.kind, mf.exists_on_disk
                 FROM media_files mf
                 JOIN library_roots lr ON lr.id = mf.library_root_id
                 WHERE mf.id = ?1",
                [file_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        file_kind_at(row, 5)?,
                        row.get::<_, bool>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|source| sql_error("resolving catalog file", source))?
            .ok_or(CatalogError::NotFound {
                entity: "catalog file",
                id: file_id,
            })?;
        let normalized = normalize_relative_path(Path::new(&row.4))?;
        if normalized != row.4 {
            return Err(CatalogError::InvalidRelativePath {
                path: PathBuf::from(row.4),
            });
        }
        Ok(ResolvedCatalogFile {
            file_id: row.0,
            media_item_id: row.1,
            root_id: row.2,
            root_path: PathBuf::from(row.3),
            relative_path: PathBuf::from(normalized),
            kind: row.5,
            exists_on_disk: row.6,
        })
    }

    pub(crate) fn recovery_file(
        &self,
        remote_key: &str,
        ordinal: i64,
    ) -> Result<Option<CatalogRecoveryFile>, CatalogError> {
        if remote_key.trim().is_empty() || ordinal < 0 {
            return Ok(None);
        }
        let conn = self.connect()?;
        let mut statement = conn
            .prepare(
                "SELECT lr.path, mf.relative_path, mf.ordinal, mf.kind, mf.byte_size
                 FROM media_items mi
                 JOIN media_files mf ON mf.media_item_id = mi.id
                 JOIN library_roots lr ON lr.id = mf.library_root_id
                 WHERE mi.remote_key = ?1 AND mf.ordinal = ?2
                   AND mf.kind IN ('photo', 'video')
                 ORDER BY mf.id
                 LIMIT 2",
            )
            .map_err(|source| sql_error("preparing catalog recovery lookup", source))?;
        let mut candidates = statement
            .query_map(params![remote_key, ordinal], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    file_kind_at(row, 3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(|source| sql_error("querying catalog recovery file", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sql_error("reading catalog recovery file", source))?;
        if candidates.len() != 1 {
            return Ok(None);
        }
        let candidate = candidates.remove(0);
        let normalized = normalize_relative_path(Path::new(&candidate.1))?;
        if normalized != candidate.1 {
            return Err(CatalogError::InvalidRelativePath {
                path: PathBuf::from(candidate.1),
            });
        }
        Ok(Some(CatalogRecoveryFile {
            root_path: PathBuf::from(candidate.0),
            relative_path: PathBuf::from(normalized),
            ordinal: candidate.2,
            kind: candidate.3,
            byte_size: candidate.4,
        }))
    }
}

fn validate_media_input(input: &CatalogMediaInput) -> Result<Vec<String>, CatalogError> {
    if input.remote_key.trim().is_empty() {
        return Err(CatalogError::InvalidInput {
            message: "remote_key must not be empty".into(),
        });
    }
    if input.files.is_empty() {
        return Err(CatalogError::InvalidInput {
            message: "media input must contain at least one file".into(),
        });
    }
    if input.source_id.is_some_and(|source_id| source_id <= 0) {
        return Err(CatalogError::InvalidInput {
            message: "source_id must be positive".into(),
        });
    }
    if input.like_count.is_some_and(|count| count < 0) {
        return Err(CatalogError::InvalidInput {
            message: "like_count must not be negative".into(),
        });
    }
    if input.comment_count.is_some_and(|count| count < 0) {
        return Err(CatalogError::InvalidInput {
            message: "comment_count must not be negative".into(),
        });
    }
    for file in &input.files {
        if file.root_id <= 0 {
            return Err(CatalogError::InvalidInput {
                message: "file root_id must be positive".into(),
            });
        }
        if file.ordinal < 0 {
            return Err(CatalogError::InvalidInput {
                message: "file ordinal must not be negative".into(),
            });
        }
        if file.byte_size < 0 {
            return Err(CatalogError::InvalidInput {
                message: "file byte_size must not be negative".into(),
            });
        }
    }
    input
        .files
        .iter()
        .map(|file| normalize_relative_path(&file.relative_path))
        .collect()
}

fn ensure_root_exists(conn: &Connection, root_id: i64) -> Result<(), CatalogError> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM library_roots WHERE id = ?1)",
            [root_id],
            |row| row.get(0),
        )
        .map_err(|source| sql_error("checking library root", source))?;
    if !exists {
        return Err(CatalogError::NotFound {
            entity: "library root",
            id: root_id,
        });
    }
    Ok(())
}

#[derive(PartialEq)]
struct StoredMedia {
    id: i64,
    kind: String,
    remote_pk: Option<String>,
    shortcode: Option<String>,
    owner_pk: Option<String>,
    owner_username: Option<String>,
    taken_at: Option<i64>,
    caption: Option<String>,
    like_count: Option<i64>,
    comment_count: Option<i64>,
    updated_at: i64,
}

fn upsert_one(
    conn: &Connection,
    input: &CatalogMediaInput,
    relative_paths: &[String],
) -> Result<UpsertResult, CatalogError> {
    let existing = conn
        .query_row(
            "SELECT id, kind, remote_pk, shortcode, owner_pk, owner_username, taken_at, caption,
                    like_count, comment_count, updated_at
             FROM media_items WHERE remote_key = ?1",
            [&input.remote_key],
            |row| {
                Ok(StoredMedia {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    remote_pk: row.get(2)?,
                    shortcode: row.get(3)?,
                    owner_pk: row.get(4)?,
                    owner_username: row.get(5)?,
                    taken_at: row.get(6)?,
                    caption: row.get(7)?,
                    like_count: row.get(8)?,
                    comment_count: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(|source| sql_error("looking up media item", source))?;
    let desired = StoredMedia {
        id: existing.as_ref().map_or(0, |item| item.id),
        kind: input.kind.as_str().into(),
        remote_pk: input.remote_pk.clone(),
        shortcode: input.shortcode.clone(),
        owner_pk: input.owner_pk.clone(),
        owner_username: input.owner_username.clone(),
        taken_at: input.taken_at,
        caption: input.caption.clone(),
        like_count: input.like_count,
        comment_count: input.comment_count,
        updated_at: input.updated_at,
    };
    let (media_item_id, mut disposition) = match existing {
        None => {
            conn.execute(
                "INSERT INTO media_items(remote_key, kind, remote_pk, shortcode, owner_pk,
                  owner_username, taken_at, caption, like_count, comment_count, imported_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    input.remote_key,
                    input.kind.as_str(),
                    input.remote_pk,
                    input.shortcode,
                    input.owner_pk,
                    input.owner_username,
                    input.taken_at,
                    input.caption,
                    input.like_count,
                    input.comment_count,
                    input.imported_at,
                    input.updated_at,
                ],
            )
            .map_err(|source| sql_error("inserting media item", source))?;
            (conn.last_insert_rowid(), UpsertDisposition::Inserted)
        }
        Some(stored) if stored == desired => (stored.id, UpsertDisposition::Unchanged),
        Some(stored) => {
            conn.execute(
                "UPDATE media_items SET kind = ?1, remote_pk = ?2, shortcode = ?3, owner_pk = ?4,
                  owner_username = ?5, taken_at = ?6, caption = ?7, like_count = ?8,
                  comment_count = ?9, updated_at = ?10 WHERE id = ?11",
                params![
                    input.kind.as_str(),
                    input.remote_pk,
                    input.shortcode,
                    input.owner_pk,
                    input.owner_username,
                    input.taken_at,
                    input.caption,
                    input.like_count,
                    input.comment_count,
                    input.updated_at,
                    stored.id,
                ],
            )
            .map_err(|source| sql_error("updating media item", source))?;
            (stored.id, UpsertDisposition::Updated)
        }
    };

    let mut file_ids = Vec::with_capacity(input.files.len());
    for (file, relative_path) in input.files.iter().zip(relative_paths) {
        let existing_file = conn
            .query_row(
                "SELECT id, media_item_id, ordinal, kind, byte_size, mtime, exists_on_disk, last_seen_at
                 FROM media_files WHERE library_root_id = ?1 AND relative_path = ?2",
                params![file.root_id, relative_path],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, bool>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(|source| sql_error("looking up media file", source))?;
        match existing_file {
            None => {
                conn.execute(
                    "INSERT INTO media_files(media_item_id, library_root_id, relative_path, ordinal,
                      kind, byte_size, mtime, exists_on_disk, last_seen_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
                    params![
                        media_item_id,
                        file.root_id,
                        relative_path,
                        file.ordinal,
                        file.kind.as_str(),
                        file.byte_size,
                        file.mtime,
                        file.last_seen_at,
                    ],
                )
                .map_err(|source| sql_error("inserting media file", source))?;
                file_ids.push(conn.last_insert_rowid());
                if disposition == UpsertDisposition::Unchanged {
                    disposition = UpsertDisposition::Updated;
                }
            }
            Some(stored) => {
                file_ids.push(stored.0);
                let matches = stored.1 == media_item_id
                    && stored.2 == file.ordinal
                    && stored.3 == file.kind.as_str()
                    && stored.4 == file.byte_size
                    && stored.5 == file.mtime
                    && stored.6
                    && stored.7 == file.last_seen_at;
                if !matches {
                    conn.execute(
                        "UPDATE media_files SET media_item_id = ?1, ordinal = ?2, kind = ?3,
                          byte_size = ?4, mtime = ?5, exists_on_disk = 1, last_seen_at = ?6
                         WHERE id = ?7",
                        params![
                            media_item_id,
                            file.ordinal,
                            file.kind.as_str(),
                            file.byte_size,
                            file.mtime,
                            file.last_seen_at,
                            stored.0,
                        ],
                    )
                    .map_err(|source| sql_error("updating media file", source))?;
                    if disposition == UpsertDisposition::Unchanged {
                        disposition = UpsertDisposition::Updated;
                    }
                }
            }
        }
    }
    if let Some(source_id) = input.source_id {
        let discovery = conn
            .query_row(
                "SELECT first_discovered_at, last_discovered_at FROM source_media
                 WHERE source_id = ?1 AND media_item_id = ?2",
                params![source_id, media_item_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|source| sql_error("looking up source membership", source))?;
        match discovery {
            None => {
                conn.execute(
                    "INSERT INTO source_media(source_id, media_item_id, first_discovered_at, last_discovered_at)
                     VALUES (?1, ?2, ?3, ?3)",
                    params![source_id, media_item_id, input.updated_at],
                )
                .map_err(|source| sql_error("inserting source membership", source))?;
                if disposition == UpsertDisposition::Unchanged {
                    disposition = UpsertDisposition::Updated;
                }
            }
            Some((_, last)) if last != input.updated_at => {
                conn.execute(
                    "UPDATE source_media SET last_discovered_at = ?1
                     WHERE source_id = ?2 AND media_item_id = ?3",
                    params![input.updated_at, source_id, media_item_id],
                )
                .map_err(|source| sql_error("updating source membership", source))?;
                if disposition == UpsertDisposition::Unchanged {
                    disposition = UpsertDisposition::Updated;
                }
            }
            Some(_) => {}
        }
    }
    Ok(UpsertResult {
        media_item_id,
        file_ids,
        disposition,
    })
}

fn fts_query(search: &str) -> Option<String> {
    let tokens: Vec<String> = search
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect();
    (!tokens.is_empty()).then(|| tokens.join(" AND "))
}

fn preferred_preview(
    conn: &Connection,
    media_item_id: i64,
) -> Result<Option<LibraryPreview>, CatalogError> {
    conn.query_row(
        "SELECT id, kind, relative_path, exists_on_disk FROM media_files
         WHERE media_item_id = ?1
         ORDER BY CASE
           WHEN exists_on_disk = 1 AND kind = 'photo' THEN 0
           WHEN exists_on_disk = 1 AND kind = 'video' THEN 1
           ELSE 2 END,
           ordinal, id
         LIMIT 1",
        [media_item_id],
        |row| {
            Ok(LibraryPreview {
                file_id: row.get(0)?,
                kind: file_kind_at(row, 1)?,
                relative_path: row.get(2)?,
                exists_on_disk: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|source| sql_error("selecting library preview", source))
}

fn cursor_scope(query: &LibraryQuery) -> LibraryCursorScope {
    let mut kinds = query.kinds.clone();
    kinds.sort_by_key(|kind| kind.as_str());
    kinds.dedup();
    LibraryCursorScope {
        sort: query.sort,
        search: query.search.as_deref().and_then(fts_query),
        kinds,
        source_id: query.source_id,
        availability: query.availability,
        taken_after: query.taken_after,
        taken_before: query.taken_before,
    }
}

fn decode_cursor(
    cursor: &str,
    expected_scope: &LibraryCursorScope,
) -> Result<LibraryCursor, CatalogError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|source| CatalogError::InvalidCursor {
            message: source.to_string(),
        })?;
    let cursor: LibraryCursor =
        serde_json::from_slice(&bytes).map_err(|source| CatalogError::InvalidCursor {
            message: source.to_string(),
        })?;
    if cursor.version != 1 {
        return Err(CatalogError::InvalidCursor {
            message: format!("unsupported cursor version {}", cursor.version),
        });
    }
    if cursor.scope != *expected_scope {
        return Err(CatalogError::InvalidCursor {
            message: "cursor does not match the current library query".into(),
        });
    }
    Ok(cursor)
}

fn encode_cursor(
    sort_value: i64,
    id: i64,
    scope: LibraryCursorScope,
) -> Result<String, CatalogError> {
    let json = serde_json::to_vec(&LibraryCursor {
        version: 1,
        sort_value,
        id,
        scope,
    })
    .map_err(|source| CatalogError::InvalidCursor {
        message: source.to_string(),
    })?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::{mpsc, Arc, Barrier};
    use std::time::Duration;

    use base64::Engine;
    use tempfile::TempDir;

    use super::super::{
        local_remote_key, Catalog, CatalogError, CatalogFileInput, CatalogMediaInput,
        FileAvailability, LibraryQuery, LibrarySort, MediaFileKind, MediaItemKind,
        UpsertDisposition,
    };
    use super::LIBRARY_MEDIA_FROM;

    struct Fixture {
        _temp: TempDir,
        catalog: Catalog,
        first_root: super::super::LibraryRoot,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let catalog = Catalog::open(temp.path().join("catalog.sqlite3")).unwrap();
            let first_root = catalog
                .register_root(&temp.path().join("media"), "Media")
                .unwrap();
            Self {
                _temp: temp,
                catalog,
                first_root,
            }
        }

        fn input(
            &self,
            remote_key: &str,
            relative_path: &str,
            timestamp: i64,
        ) -> CatalogMediaInput {
            CatalogMediaInput {
                remote_key: remote_key.into(),
                kind: MediaItemKind::Post,
                remote_pk: Some(remote_key.into()),
                shortcode: Some(format!("short-{remote_key}")),
                owner_pk: Some("owner-1".into()),
                owner_username: Some("alice".into()),
                taken_at: Some(timestamp),
                caption: Some("a caption".into()),
                like_count: Some(3),
                comment_count: Some(2),
                imported_at: timestamp,
                updated_at: timestamp,
                files: vec![CatalogFileInput {
                    root_id: self.first_root.id,
                    relative_path: PathBuf::from(relative_path),
                    ordinal: 0,
                    kind: MediaFileKind::Photo,
                    byte_size: 123,
                    mtime: timestamp,
                    last_seen_at: timestamp,
                }],
                source_id: None,
            }
        }
    }

    fn default_query() -> LibraryQuery {
        LibraryQuery {
            search: None,
            kinds: Vec::new(),
            source_id: None,
            availability: None,
            taken_after: None,
            taken_before: None,
            sort: LibrarySort::TakenAtDesc,
            cursor: None,
            limit: 60,
        }
    }

    #[test]
    fn root_registration_is_canonical_and_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(temp.path().join("catalog.sqlite3")).unwrap();
        let path = temp.path().join("photos");

        let first = catalog.register_root(&path, " Photos ").unwrap();
        let second = catalog.register_root(&path.join("."), "Changed").unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(first.created_at, second.created_at);
        assert_eq!(first.path, path.canonicalize().unwrap());
        assert_eq!(first.label, "Photos");
        assert_eq!(catalog.list_roots().unwrap(), vec![first]);
    }

    #[test]
    fn recovery_lookup_requires_exact_unique_remote_key_and_ordinal() {
        let fixture = Fixture::new();
        let mut input = fixture.input("post:collision", "posts/item_1.jpg", 10);
        fixture.catalog.upsert_media(&input).unwrap();

        let recovered = fixture
            .catalog
            .recovery_file("post:collision", 0)
            .unwrap()
            .expect("one exact catalog file should be recoverable");
        assert_eq!(recovered.root_path, fixture.first_root.path);
        assert_eq!(recovered.relative_path, Path::new("posts/item_1.jpg"));
        assert_eq!(recovered.ordinal, 0);
        assert_eq!(recovered.kind, MediaFileKind::Photo);
        assert_eq!(recovered.byte_size, 123);
        assert!(fixture
            .catalog
            .recovery_file("post:other", 0)
            .unwrap()
            .is_none());
        assert!(fixture
            .catalog
            .recovery_file("post:collision", 1)
            .unwrap()
            .is_none());

        input.files.push(CatalogFileInput {
            root_id: fixture.first_root.id,
            relative_path: PathBuf::from("posts/item_1_duplicate.mp4"),
            ordinal: 0,
            kind: MediaFileKind::Video,
            byte_size: 456,
            mtime: 10,
            last_seen_at: 10,
        });
        fixture.catalog.upsert_media(&input).unwrap();

        assert!(fixture
            .catalog
            .recovery_file("post:collision", 0)
            .unwrap()
            .is_none());
    }

    #[test]
    fn concurrent_root_registration_returns_one_persisted_root() {
        const CALLERS: usize = 16;
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("shared-root");
        std::fs::create_dir(&root_path).unwrap();
        let catalog = Arc::new(Catalog::open(temp.path().join("catalog.sqlite3")).unwrap());
        let barrier = Arc::new(Barrier::new(CALLERS));
        let (sender, receiver) = mpsc::channel();

        let threads: Vec<_> = (0..CALLERS)
            .map(|index| {
                let catalog = Arc::clone(&catalog);
                let barrier = Arc::clone(&barrier);
                let sender = sender.clone();
                let root_path = root_path.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    sender
                        .send(catalog.register_root(&root_path, &format!("label-{index}")))
                        .unwrap();
                })
            })
            .collect();
        drop(sender);

        let results: Vec<_> = (0..CALLERS)
            .map(|_| receiver.recv_timeout(Duration::from_secs(10)).unwrap())
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let roots: Vec<_> = results.into_iter().collect::<Result<_, _>>().unwrap();
        assert!(roots.iter().all(|root| root == &roots[0]));
        assert_eq!(catalog.list_roots().unwrap(), vec![roots[0].clone()]);
    }

    #[test]
    fn upsert_reuses_remote_item_and_reconciles_file() {
        let fixture = Fixture::new();
        let mut input = fixture.input("post:1", "alice/item.jpg", 10);

        let inserted = fixture.catalog.upsert_media(&input).unwrap();
        input.caption = Some("changed".into());
        input.files[0].byte_size = 999;
        input.files[0].mtime = 20;
        input.files[0].last_seen_at = 20;
        input.updated_at = 20;
        let updated = fixture.catalog.upsert_media(&input).unwrap();

        assert_eq!(inserted.disposition, UpsertDisposition::Inserted);
        assert_eq!(updated.disposition, UpsertDisposition::Updated);
        assert_eq!(inserted.media_item_id, updated.media_item_id);
        assert_eq!(inserted.file_ids, updated.file_ids);
        let detail = fixture
            .catalog
            .get_library_item(inserted.media_item_id)
            .unwrap()
            .unwrap();
        assert_eq!(detail.caption.as_deref(), Some("changed"));
        assert_eq!(detail.files.len(), 1);
        assert_eq!(detail.files[0].byte_size, 999);
        assert!(detail.files[0].exists_on_disk);
        assert_eq!(
            fixture.catalog.upsert_media(&input).unwrap().disposition,
            UpsertDisposition::Unchanged
        );
    }

    #[test]
    fn local_keys_do_not_collide_across_roots() {
        assert_ne!(
            local_remote_key(1, Path::new("same/file.jpg")).unwrap(),
            local_remote_key(2, Path::new("same/file.jpg")).unwrap()
        );
        assert_eq!(
            local_remote_key(7, Path::new("same/file.jpg")).unwrap(),
            "local:7:same/file.jpg"
        );
    }

    #[test]
    fn missing_then_seen_again_preserves_history() {
        let fixture = Fixture::new();
        let mut input = fixture.input("post:1", "one.jpg", 10);
        let inserted = fixture.catalog.upsert_media(&input).unwrap();
        let mut recent = fixture.input("post:2", "recent.jpg", 30);
        recent.files[0].last_seen_at = 30;
        let recent_id = fixture.catalog.upsert_media(&recent).unwrap().media_item_id;

        assert_eq!(
            fixture
                .catalog
                .mark_unseen_missing(fixture.first_root.id, 20)
                .unwrap(),
            1
        );
        let missing = fixture
            .catalog
            .get_library_item(inserted.media_item_id)
            .unwrap()
            .unwrap();
        assert!(!missing.files[0].exists_on_disk);
        input.files[0].last_seen_at = 40;
        let restored = fixture.catalog.upsert_media(&input).unwrap();

        assert_eq!(restored.file_ids, inserted.file_ids);
        assert!(
            fixture
                .catalog
                .get_library_item(inserted.media_item_id)
                .unwrap()
                .unwrap()
                .files[0]
                .exists_on_disk
        );
        assert!(
            fixture
                .catalog
                .get_library_item(recent_id)
                .unwrap()
                .unwrap()
                .files[0]
                .exists_on_disk
        );
    }

    #[test]
    fn fts_search_matches_username_shortcode_and_caption() {
        let fixture = Fixture::new();
        let mut input = fixture.input("post:1", "one.jpg", 10);
        input.owner_username = Some("user.name".into());
        input.shortcode = Some("code-special".into());
        input.caption = Some("caption (literal)".into());
        let id = fixture.catalog.upsert_media(&input).unwrap().media_item_id;

        for search in ["user.name", "code-special", "caption (literal)"] {
            let mut query = default_query();
            query.search = Some(search.into());
            assert_eq!(
                fixture.catalog.query_library(&query).unwrap().items[0].id,
                id
            );
        }
        let mut operator = default_query();
        operator.search = Some("caption OR absent".into());
        assert!(fixture
            .catalog
            .query_library(&operator)
            .unwrap()
            .items
            .is_empty());
    }

    #[test]
    fn keyset_pagination_is_stable_when_timestamps_tie() {
        let fixture = Fixture::new();
        for index in 0..5 {
            fixture
                .catalog
                .upsert_media(&fixture.input(&format!("post:{index}"), &format!("{index}.jpg"), 10))
                .unwrap();
        }
        let mut query = default_query();
        query.limit = 2;
        let first = fixture.catalog.query_library(&query).unwrap();
        query.cursor = first.next_cursor.clone();
        let second = fixture.catalog.query_library(&query).unwrap();
        query.cursor = second.next_cursor.clone();
        let third = fixture.catalog.query_library(&query).unwrap();

        let ids: Vec<i64> = first
            .items
            .iter()
            .chain(&second.items)
            .chain(&third.items)
            .map(|item| item.id)
            .collect();
        assert_eq!(ids.len(), 5);
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 5);
        assert!(first.next_cursor.is_some());
        assert!(second.next_cursor.is_some());
        assert!(third.next_cursor.is_none());
    }

    #[test]
    fn availability_filter_requires_at_least_one_existing_file() {
        let fixture = Fixture::new();
        let mut available = fixture.input("post:1", "one.jpg", 10);
        available.files.push(CatalogFileInput {
            relative_path: "one.mp4".into(),
            ordinal: 1,
            kind: MediaFileKind::Video,
            ..available.files[0].clone()
        });
        let available_id = fixture
            .catalog
            .upsert_media(&available)
            .unwrap()
            .media_item_id;
        let missing_id = fixture
            .catalog
            .upsert_media(&fixture.input("post:2", "two.jpg", 10))
            .unwrap()
            .media_item_id;
        fixture
            .catalog
            .mark_unseen_missing(fixture.first_root.id, 11)
            .unwrap();
        let mut refreshed = available.clone();
        refreshed.updated_at = 12;
        refreshed.files.truncate(1);
        refreshed.files[0].last_seen_at = 12;
        fixture.catalog.upsert_media(&refreshed).unwrap();
        let detail = fixture
            .catalog
            .get_library_item(available_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            detail
                .files
                .iter()
                .filter(|file| file.exists_on_disk)
                .count(),
            1
        );
        assert_eq!(
            detail
                .files
                .iter()
                .filter(|file| !file.exists_on_disk)
                .count(),
            1
        );

        let mut query = default_query();
        query.availability = Some(FileAvailability::Available);
        assert_eq!(
            fixture
                .catalog
                .query_library(&query)
                .unwrap()
                .items
                .iter()
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            vec![available_id]
        );
        query.availability = Some(FileAvailability::Missing);
        assert_eq!(
            fixture
                .catalog
                .query_library(&query)
                .unwrap()
                .items
                .iter()
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            vec![missing_id]
        );
    }

    #[test]
    fn resource_count_excludes_metadata_sidecars() {
        let fixture = Fixture::new();
        let mut input = fixture.input("post:1", "photo.jpg", 10);
        input.files.push(CatalogFileInput {
            relative_path: "photo.json".into(),
            ordinal: 1,
            kind: MediaFileKind::Metadata,
            ..input.files[0].clone()
        });
        fixture.catalog.upsert_media(&input).unwrap();

        let card = fixture
            .catalog
            .query_library(&default_query())
            .unwrap()
            .items
            .remove(0);

        assert_eq!(card.resource_count, 1);
    }

    #[test]
    fn path_relink_hides_zero_file_item_but_preserves_its_history() {
        let fixture = Fixture::new();
        let first = fixture
            .catalog
            .upsert_media(&fixture.input("post:first", "shared.jpg", 10))
            .unwrap();
        let second = fixture
            .catalog
            .upsert_media(&fixture.input("post:second", "shared.jpg", 20))
            .unwrap();

        let page = fixture.catalog.query_library(&default_query()).unwrap();
        assert_eq!(
            page.items.iter().map(|card| card.id).collect::<Vec<_>>(),
            vec![second.media_item_id]
        );
        let first_detail = fixture
            .catalog
            .get_library_item(first.media_item_id)
            .unwrap()
            .unwrap();
        assert!(first_detail.files.is_empty());
    }

    #[test]
    fn unsafe_relative_paths_are_rejected() {
        let fixture = Fixture::new();
        for path in [
            PathBuf::from("/absolute.jpg"),
            PathBuf::from("../parent.jpg"),
            PathBuf::from("child/../parent.jpg"),
            PathBuf::from("./cur.jpg"),
        ] {
            let mut input = fixture.input("post:bad", "safe.jpg", 10);
            input.files[0].relative_path = path;
            assert!(matches!(
                fixture.catalog.upsert_media(&input),
                Err(CatalogError::InvalidRelativePath { .. })
            ));
        }
        assert!(matches!(
            local_remote_key(1, Path::new("")),
            Err(CatalogError::InvalidRelativePath { .. })
        ));
        #[cfg(windows)]
        assert!(matches!(
            local_remote_key(1, Path::new(r"C:\prefix.jpg")),
            Err(CatalogError::InvalidRelativePath { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_root_and_relative_paths_are_rejected_without_aliasing() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let temp = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(temp.path().join("catalog.sqlite3")).unwrap();
        let first_root = temp.path().join(OsString::from_vec(vec![b'r', 0xff]));
        let second_root = temp.path().join(OsString::from_vec(vec![b'r', 0xfe]));
        let first_result = catalog.register_root(&first_root, "first");
        assert!(
            matches!(&first_result, Err(CatalogError::NonUtf8Path { .. })),
            "{first_result:?}"
        );
        assert!(matches!(
            catalog.register_root(&second_root, "second"),
            Err(CatalogError::NonUtf8Path { .. })
        ));
        assert!(catalog.list_roots().unwrap().is_empty());

        let first_relative = PathBuf::from(OsString::from_vec(vec![b'f', 0xff]));
        let second_relative = PathBuf::from(OsString::from_vec(vec![b'f', 0xfe]));
        assert!(matches!(
            local_remote_key(1, &first_relative),
            Err(CatalogError::NonUtf8Path { .. })
        ));
        assert!(matches!(
            local_remote_key(1, &second_relative),
            Err(CatalogError::NonUtf8Path { .. })
        ));
    }

    #[test]
    fn oversized_batch_is_rejected_atomically() {
        let fixture = Fixture::new();
        let inputs: Vec<_> = (0..101)
            .map(|index| fixture.input(&format!("post:{index}"), &format!("{index}.jpg"), index))
            .collect();
        assert!(matches!(
            fixture.catalog.upsert_media_batch(&inputs),
            Err(CatalogError::BatchTooLarge {
                size: 101,
                max: 100
            })
        ));
        assert!(fixture
            .catalog
            .query_library(&default_query())
            .unwrap()
            .items
            .is_empty());
    }

    #[test]
    fn cancellable_batch_rolls_back_rows_when_cancelled_at_precommit() {
        let fixture = Fixture::new();
        let inputs = vec![
            fixture.input("post:first", "first.jpg", 10),
            fixture.input("post:second", "second.jpg", 10),
        ];
        let mut precommit_checks = 0;

        let result = fixture.catalog.upsert_media_batch_cancellable(&inputs, || {
            precommit_checks += 1;
            true
        });

        assert!(matches!(
            result,
            Err(CatalogError::Cancelled {
                operation: "committing media upsert"
            })
        ));
        assert_eq!(precommit_checks, 1);
        let conn = fixture.catalog.connect().unwrap();
        for table in ["media_items", "media_files"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} escaped the cancelled transaction");
        }

        let committed = fixture.catalog.upsert_media_batch(&inputs).unwrap();
        assert_eq!(committed.len(), 2);
        assert!(committed
            .iter()
            .all(|result| result.disposition == UpsertDisposition::Inserted));
    }

    #[test]
    fn batch_preflights_all_roots_before_persisting_any_rows() {
        let fixture = Fixture::new();
        let mut valid = fixture.input("post:valid", "valid.jpg", 10);
        valid.source_id = Some(42);
        let mut unknown_root = fixture.input("post:unknown", "unknown.jpg", 20);
        unknown_root.files[0].root_id = 999;

        assert!(matches!(
            fixture.catalog.upsert_media_batch(&[valid, unknown_root]),
            Err(CatalogError::NotFound {
                entity: "library root",
                id: 999
            })
        ));
        let conn = fixture.catalog.connect().unwrap();
        for table in ["media_items", "media_files", "source_media"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} was mutated before root preflight");
        }
    }

    #[test]
    fn duplicate_file_claims_in_a_batch_are_rejected_before_writes() {
        for duplicate_within_one_input in [true, false] {
            let fixture = Fixture::new();
            let mut first = fixture.input("post:first", "album/shared.jpg", 10);
            first.source_id = Some(42);
            let inputs = if duplicate_within_one_input {
                first.files.push(first.files[0].clone());
                vec![first]
            } else {
                vec![first, fixture.input("post:second", "album//shared.jpg", 20)]
            };

            assert!(matches!(
                fixture.catalog.upsert_media_batch(&inputs),
                Err(CatalogError::InvalidInput { .. })
            ));
            let conn = fixture.catalog.connect().unwrap();
            for table in ["media_items", "media_files", "source_media"] {
                let count: i64 = conn
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap();
                assert_eq!(count, 0, "{table} was mutated for a duplicate claim");
            }
        }
    }

    #[test]
    fn invalid_numeric_domain_inputs_are_rejected_before_writes() {
        let fixture = Fixture::new();
        let cases: Vec<CatalogMediaInput> = [
            "root_id",
            "negative_root_id",
            "ordinal",
            "byte_size",
            "source_id",
            "negative_source_id",
            "like_count",
            "comment_count",
        ]
        .into_iter()
        .map(|field| {
            let mut input = fixture.input(&format!("post:{field}"), &format!("{field}.jpg"), 10);
            match field {
                "root_id" => input.files[0].root_id = 0,
                "negative_root_id" => input.files[0].root_id = -1,
                "ordinal" => input.files[0].ordinal = -1,
                "byte_size" => input.files[0].byte_size = -1,
                "source_id" => input.source_id = Some(0),
                "negative_source_id" => input.source_id = Some(-1),
                "like_count" => input.like_count = Some(-1),
                "comment_count" => input.comment_count = Some(-1),
                _ => unreachable!(),
            }
            input
        })
        .collect();

        for input in cases {
            assert!(matches!(
                fixture.catalog.upsert_media(&input),
                Err(CatalogError::InvalidInput { .. })
            ));
        }
        let conn = fixture.catalog.connect().unwrap();
        for table in ["media_items", "media_files", "source_media"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} was mutated for invalid numeric input");
        }

        for source_id in [0, -1] {
            let mut query = default_query();
            query.source_id = Some(source_id);
            assert!(matches!(
                fixture.catalog.query_library(&query),
                Err(CatalogError::InvalidInput { .. })
            ));
        }
    }

    #[test]
    fn malformed_cursor_is_a_typed_error() {
        let fixture = Fixture::new();
        for cursor in ["not base64", "e30"] {
            let mut query = default_query();
            query.cursor = Some(cursor.into());
            assert!(matches!(
                fixture.catalog.query_library(&query),
                Err(CatalogError::InvalidCursor { .. })
            ));
        }
    }

    #[test]
    fn cursor_rejects_sort_search_and_version_mismatches() {
        let fixture = Fixture::new();
        for index in 0..3 {
            fixture
                .catalog
                .upsert_media(&fixture.input(
                    &format!("post:{index}"),
                    &format!("{index}.jpg"),
                    index,
                ))
                .unwrap();
        }
        let mut first_query = default_query();
        first_query.limit = 1;
        let cursor = fixture
            .catalog
            .query_library(&first_query)
            .unwrap()
            .next_cursor
            .unwrap();

        let mut changed_sort = first_query.clone();
        changed_sort.cursor = Some(cursor.clone());
        changed_sort.sort = LibrarySort::ImportedAtDesc;
        assert!(matches!(
            fixture.catalog.query_library(&changed_sort),
            Err(CatalogError::InvalidCursor { .. })
        ));

        let mut changed_search = first_query.clone();
        changed_search.cursor = Some(cursor.clone());
        changed_search.search = Some("alice".into());
        assert!(matches!(
            fixture.catalog.query_library(&changed_search),
            Err(CatalogError::InvalidCursor { .. })
        ));

        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(cursor)
            .unwrap();
        let mut cursor_json: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        cursor_json["version"] = 2.into();
        let unknown_version = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&cursor_json).unwrap());
        let mut unknown = first_query;
        unknown.cursor = Some(unknown_version);
        assert!(matches!(
            fixture.catalog.query_library(&unknown),
            Err(CatalogError::InvalidCursor { .. })
        ));
    }

    #[test]
    fn cursor_accepts_equivalent_reordered_kind_scope() {
        let fixture = Fixture::new();
        let mut post = fixture.input("post:1", "post.jpg", 10);
        post.caption = Some("alpha beta".into());
        fixture.catalog.upsert_media(&post).unwrap();
        let mut reel = fixture.input("reel:1", "reel.mp4", 20);
        reel.kind = MediaItemKind::Reel;
        reel.files[0].kind = MediaFileKind::Video;
        reel.caption = Some("alpha beta".into());
        fixture.catalog.upsert_media(&reel).unwrap();

        let mut query = default_query();
        query.search = Some("  alpha   beta  ".into());
        query.kinds = vec![
            MediaItemKind::Reel,
            MediaItemKind::Post,
            MediaItemKind::Reel,
        ];
        query.limit = 1;
        let first = fixture.catalog.query_library(&query).unwrap();

        query.search = Some("alpha beta".into());
        query.kinds = vec![MediaItemKind::Post, MediaItemKind::Reel];
        query.cursor = first.next_cursor;
        assert_eq!(
            fixture.catalog.query_library(&query).unwrap().items.len(),
            1
        );
    }

    #[test]
    fn duplicate_kind_filters_use_canonical_scope_for_sql_and_cursor() {
        let fixture = Fixture::new();
        fixture
            .catalog
            .upsert_media(&fixture.input("post:1", "one.jpg", 10))
            .unwrap();
        fixture
            .catalog
            .upsert_media(&fixture.input("post:2", "two.jpg", 20))
            .unwrap();

        let mut query = default_query();
        query.kinds = vec![MediaItemKind::Post; 40_000];
        query.limit = 1;
        let first = fixture.catalog.query_library(&query).unwrap();
        assert_eq!(first.items.len(), 1);

        query.kinds = vec![MediaItemKind::Post];
        query.cursor = first.next_cursor;
        assert_eq!(
            fixture.catalog.query_library(&query).unwrap().items.len(),
            1
        );
    }

    #[test]
    fn source_filter_does_not_duplicate_media_cards() {
        let fixture = Fixture::new();
        let mut input = fixture.input("post:1", "photo.jpg", 10);
        input.source_id = Some(42);
        input.files.push(CatalogFileInput {
            relative_path: "video.mp4".into(),
            ordinal: 1,
            kind: MediaFileKind::Video,
            ..input.files[0].clone()
        });
        let id = fixture.catalog.upsert_media(&input).unwrap().media_item_id;
        let mut query = default_query();
        query.source_id = Some(42);
        assert_eq!(
            fixture
                .catalog
                .query_library(&query)
                .unwrap()
                .items
                .iter()
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            vec![id]
        );
    }

    #[test]
    fn preferred_preview_uses_existing_photo_then_video_then_first_file() {
        let fixture = Fixture::new();
        let mut input = fixture.input("post:1", "missing-photo.jpg", 10);
        input.files[0].ordinal = 0;
        input.files.push(CatalogFileInput {
            relative_path: "video.mp4".into(),
            ordinal: 1,
            kind: MediaFileKind::Video,
            ..input.files[0].clone()
        });
        input.files.push(CatalogFileInput {
            relative_path: "photo.jpg".into(),
            ordinal: 2,
            kind: MediaFileKind::Photo,
            ..input.files[0].clone()
        });
        let result = fixture.catalog.upsert_media(&input).unwrap();
        fixture
            .catalog
            .mark_unseen_missing(fixture.first_root.id, 11)
            .unwrap();
        let missing_card = fixture
            .catalog
            .query_library(&default_query())
            .unwrap()
            .items
            .remove(0);
        let fallback = missing_card.preview.unwrap();
        assert_eq!(fallback.file_id, result.file_ids[0]);
        assert!(!fallback.exists_on_disk);

        input.files[1].last_seen_at = 12;
        fixture
            .catalog
            .upsert_media(&CatalogMediaInput {
                files: vec![input.files[1].clone()],
                ..input.clone()
            })
            .unwrap();
        let card = fixture
            .catalog
            .query_library(&default_query())
            .unwrap()
            .items
            .remove(0);
        assert_eq!(card.preview.unwrap().kind, MediaFileKind::Video);

        input.files[2].last_seen_at = 13;
        fixture
            .catalog
            .upsert_media(&CatalogMediaInput {
                files: vec![input.files[2].clone()],
                ..input.clone()
            })
            .unwrap();
        let card = fixture
            .catalog
            .query_library(&default_query())
            .unwrap()
            .items
            .remove(0);
        assert_eq!(card.preview.unwrap().file_id, result.file_ids[2]);
    }

    #[test]
    fn ordinary_filters_are_inclusive_and_imported_sort_is_stable() {
        let fixture = Fixture::new();
        let older = fixture
            .catalog
            .upsert_media(&fixture.input("post:older", "older.jpg", 10))
            .unwrap()
            .media_item_id;
        let mut middle = fixture.input("reel:middle", "middle.mp4", 20);
        middle.kind = MediaItemKind::Reel;
        middle.files[0].kind = MediaFileKind::Video;
        middle.imported_at = 30;
        let middle = fixture.catalog.upsert_media(&middle).unwrap().media_item_id;
        let mut newer = fixture.input("story:newer", "newer.jpg", 30);
        newer.kind = MediaItemKind::Story;
        newer.imported_at = 20;
        let newer = fixture.catalog.upsert_media(&newer).unwrap().media_item_id;

        let mut query = default_query();
        query.kinds = vec![MediaItemKind::Reel];
        assert_eq!(
            fixture.catalog.query_library(&query).unwrap().items[0].id,
            middle
        );

        query.kinds.clear();
        query.taken_after = Some(20);
        query.taken_before = Some(30);
        assert_eq!(
            fixture
                .catalog
                .query_library(&query)
                .unwrap()
                .items
                .iter()
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            vec![newer, middle]
        );

        query.taken_after = None;
        query.taken_before = None;
        query.sort = LibrarySort::ImportedAtDesc;
        assert_eq!(
            fixture
                .catalog
                .query_library(&query)
                .unwrap()
                .items
                .iter()
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            vec![middle, newer, older]
        );
    }

    #[test]
    fn taken_at_sort_uses_expression_index_without_temp_ordering() {
        let fixture = Fixture::new();
        let inputs: Vec<_> = (0..100)
            .map(|index| {
                let mut input =
                    fixture.input(&format!("post:{index}"), &format!("{index}.jpg"), index);
                if index % 2 == 0 {
                    input.taken_at = None;
                }
                input
            })
            .collect();
        fixture.catalog.upsert_media_batch(&inputs).unwrap();

        let conn = fixture.catalog.connect().unwrap();
        let sql = format!(
            "EXPLAIN QUERY PLAN
                 SELECT mi.id FROM {}
                 WHERE EXISTS(SELECT 1 FROM media_files mf WHERE mf.media_item_id = mi.id)
                 ORDER BY COALESCE(mi.taken_at, mi.imported_at) DESC, mi.id DESC
                 LIMIT 60",
            LIBRARY_MEDIA_FROM
        );
        let mut statement = conn.prepare(&sql).unwrap();
        let plan: Vec<String> = statement
            .query_map([], |row| row.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(
            plan.iter()
                .any(|detail| detail.contains("media_items_effective_taken_at_idx")),
            "query plan did not use effective taken-at index: {plan:?}"
        );
        assert!(
            plan.iter().all(|detail| !detail.contains("TEMP B-TREE")),
            "query plan sorted through a temp B-tree: {plan:?}"
        );
    }

    #[test]
    fn selective_library_plan_does_not_force_effective_sort_index() {
        let fixture = Fixture::new();
        let mut input = fixture.input("post:needle", "needle.jpg", 10);
        input.caption = Some("needle".into());
        input.source_id = Some(42);
        fixture.catalog.upsert_media(&input).unwrap();

        let conn = fixture.catalog.connect().unwrap();
        let sql = format!(
            "EXPLAIN QUERY PLAN
             SELECT mi.id FROM {}
             WHERE mi.id IN (SELECT rowid FROM media_fts WHERE media_fts MATCH ?1)
               AND mi.kind IN (?2)
               AND EXISTS(
                 SELECT 1 FROM source_media sm
                 WHERE sm.media_item_id = mi.id AND sm.source_id = ?3
               )
             ORDER BY COALESCE(mi.taken_at, mi.imported_at) DESC, mi.id DESC
             LIMIT 60",
            LIBRARY_MEDIA_FROM
        );
        let mut statement = conn.prepare(&sql).unwrap();
        let plan: Vec<String> = statement
            .query_map(rusqlite::params!["\"needle\"", "post", 42], |row| {
                row.get(3)
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(
            plan.iter()
                .all(|detail| !detail.contains("media_items_effective_taken_at_idx")),
            "selective plan was forced through the sort index: {plan:?}"
        );
    }

    #[test]
    fn limits_default_to_sixty_and_clamp_to_one_hundred() {
        let fixture = Fixture::new();
        for index in 0..105 {
            fixture
                .catalog
                .upsert_media(&fixture.input(
                    &format!("post:{index}"),
                    &format!("{index}.jpg"),
                    index,
                ))
                .unwrap();
        }
        let mut query = default_query();
        query.limit = 0;
        assert_eq!(
            fixture.catalog.query_library(&query).unwrap().items.len(),
            60
        );
        query.limit = 1_000;
        assert_eq!(
            fixture.catalog.query_library(&query).unwrap().items.len(),
            100
        );
    }

    #[test]
    fn scan_timestamps_unknown_roots_and_file_resolution_are_typed() {
        let fixture = Fixture::new();
        fixture
            .catalog
            .begin_scan(fixture.first_root.id, 10)
            .unwrap();
        fixture
            .catalog
            .finish_scan(fixture.first_root.id, 20)
            .unwrap();
        let root = fixture.catalog.list_roots().unwrap().remove(0);
        assert_eq!(root.last_scan_started_at, Some(10));
        assert_eq!(root.last_scan_completed_at, Some(20));
        assert!(matches!(
            fixture.catalog.begin_scan(999, 1),
            Err(CatalogError::NotFound { .. })
        ));
        assert!(matches!(
            fixture.catalog.resolve_file(999),
            Err(CatalogError::NotFound { .. })
        ));
        let inserted = fixture
            .catalog
            .upsert_media(&fixture.input("post:1", "one.jpg", 10))
            .unwrap();
        let resolved = fixture.catalog.resolve_file(inserted.file_ids[0]).unwrap();
        assert_eq!(resolved.root_path, fixture.first_root.path);
        assert_eq!(resolved.relative_path, PathBuf::from("one.jpg"));
    }

    #[test]
    fn cancellable_scan_finalization_rolls_back_missing_and_completion_together() {
        let fixture = Fixture::new();
        let old = fixture
            .catalog
            .upsert_media(&fixture.input("post:old", "old.jpg", 10))
            .unwrap();
        let recent = fixture
            .catalog
            .upsert_media(&fixture.input("post:recent", "recent.jpg", 30))
            .unwrap();
        fixture
            .catalog
            .begin_scan(fixture.first_root.id, 20)
            .unwrap();
        let mut precommit_checks = 0;

        let cancelled =
            fixture
                .catalog
                .finalize_scan_cancellable(fixture.first_root.id, 20, 25, || {
                    precommit_checks += 1;
                    true
                });

        assert!(matches!(
            cancelled,
            Err(CatalogError::Cancelled {
                operation: "committing scan finalization"
            })
        ));
        assert_eq!(precommit_checks, 1);
        for id in [old.media_item_id, recent.media_item_id] {
            assert!(fixture.catalog.get_library_item(id).unwrap().unwrap().files[0].exists_on_disk);
        }
        assert_eq!(
            fixture.catalog.list_roots().unwrap()[0].last_scan_completed_at,
            None
        );

        let missing = fixture
            .catalog
            .finalize_scan_cancellable(fixture.first_root.id, 20, 25, || false)
            .unwrap();
        assert_eq!(missing, 1);
        assert!(
            !fixture
                .catalog
                .get_library_item(old.media_item_id)
                .unwrap()
                .unwrap()
                .files[0]
                .exists_on_disk
        );
        assert!(
            fixture
                .catalog
                .get_library_item(recent.media_item_id)
                .unwrap()
                .unwrap()
                .files[0]
                .exists_on_disk
        );
        assert_eq!(
            fixture.catalog.list_roots().unwrap()[0].last_scan_completed_at,
            Some(25)
        );
    }

    #[test]
    fn scan_finalization_sql_failure_rolls_back_missing_changes() {
        let fixture = Fixture::new();
        let old = fixture
            .catalog
            .upsert_media(&fixture.input("post:old", "old.jpg", 10))
            .unwrap();
        fixture
            .catalog
            .begin_scan(fixture.first_root.id, 20)
            .unwrap();
        let conn = fixture.catalog.connect().unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_scan_completion
             BEFORE UPDATE OF last_scan_completed_at ON library_roots
             BEGIN
               SELECT RAISE(ABORT, 'forced scan completion failure');
             END;",
        )
        .unwrap();

        let result =
            fixture
                .catalog
                .finalize_scan_cancellable(fixture.first_root.id, 20, 25, || false);

        assert!(matches!(result, Err(CatalogError::Sql { .. })));
        assert!(
            fixture
                .catalog
                .get_library_item(old.media_item_id)
                .unwrap()
                .unwrap()
                .files[0]
                .exists_on_disk
        );
        assert_eq!(
            fixture.catalog.list_roots().unwrap()[0].last_scan_completed_at,
            None
        );
    }
}
