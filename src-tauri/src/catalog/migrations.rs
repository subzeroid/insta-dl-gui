use rusqlite::{Connection, TransactionBehavior};
use thiserror::Error;

const LATEST_SCHEMA_VERSION: u32 = 1;

const MIGRATION_V1: &str = r#"
CREATE TABLE library_roots (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  label TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_scan_started_at INTEGER,
  last_scan_completed_at INTEGER
);

CREATE TABLE media_items (
  id INTEGER PRIMARY KEY,
  remote_key TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL CHECK(kind IN ('post','reel','story','avatar')),
  remote_pk TEXT,
  shortcode TEXT,
  owner_pk TEXT,
  owner_username TEXT,
  taken_at INTEGER,
  caption TEXT,
  like_count INTEGER,
  comment_count INTEGER,
  imported_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE media_files (
  id INTEGER PRIMARY KEY,
  media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
  library_root_id INTEGER NOT NULL REFERENCES library_roots(id),
  relative_path TEXT NOT NULL,
  ordinal INTEGER NOT NULL DEFAULT 0,
  kind TEXT NOT NULL CHECK(kind IN ('photo','video','metadata','unknown')),
  byte_size INTEGER NOT NULL,
  mtime INTEGER NOT NULL,
  exists_on_disk INTEGER NOT NULL DEFAULT 1 CHECK(exists_on_disk IN (0,1)),
  last_seen_at INTEGER NOT NULL,
  UNIQUE(library_root_id, relative_path)
);

CREATE TABLE source_media (
  source_id INTEGER NOT NULL,
  media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
  first_discovered_at INTEGER NOT NULL,
  last_discovered_at INTEGER NOT NULL,
  UNIQUE(source_id, media_item_id)
);

CREATE VIRTUAL TABLE media_fts USING fts5(
  owner_username,
  shortcode,
  caption,
  content='media_items',
  content_rowid='id'
);

CREATE TRIGGER media_items_ai AFTER INSERT ON media_items BEGIN
  INSERT INTO media_fts(rowid, owner_username, shortcode, caption)
  VALUES (new.id, new.owner_username, new.shortcode, new.caption);
END;
CREATE TRIGGER media_items_ad AFTER DELETE ON media_items BEGIN
  INSERT INTO media_fts(media_fts, rowid, owner_username, shortcode, caption)
  VALUES ('delete', old.id, old.owner_username, old.shortcode, old.caption);
END;
CREATE TRIGGER media_items_au AFTER UPDATE ON media_items BEGIN
  INSERT INTO media_fts(media_fts, rowid, owner_username, shortcode, caption)
  VALUES ('delete', old.id, old.owner_username, old.shortcode, old.caption);
  INSERT INTO media_fts(rowid, owner_username, shortcode, caption)
  VALUES (new.id, new.owner_username, new.shortcode, new.caption);
END;

CREATE INDEX media_items_taken_at_idx ON media_items(taken_at DESC, id DESC);
CREATE INDEX media_items_imported_at_idx ON media_items(imported_at DESC, id DESC);
CREATE INDEX media_items_kind_idx ON media_items(kind);
CREATE INDEX media_files_item_idx ON media_files(media_item_id);
CREATE INDEX media_files_exists_idx ON media_files(exists_on_disk);
"#;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("catalog schema version {0} is newer than this application supports")]
    NewerVersion(u32),
}

pub fn migrate(conn: &mut Connection) -> Result<(), MigrationError> {
    let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version = schema_version(&transaction)?;
    #[cfg(test)]
    test_hooks::notify_version_read(&transaction);
    if version > LATEST_SCHEMA_VERSION {
        transaction.commit()?;
        return Err(MigrationError::NewerVersion(version));
    }
    if version == LATEST_SCHEMA_VERSION {
        transaction.commit()?;
        return Ok(());
    }

    transaction.execute_batch(MIGRATION_V1)?;
    transaction.execute_batch("PRAGMA user_version = 1;")?;
    transaction.commit()?;
    Ok(())
}

fn schema_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

#[cfg(test)]
fn object_exists(conn: &Connection, object_type: &str, name: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = ?1 AND name = ?2)",
        (object_type, name),
        |row| row.get(0),
    )
    .unwrap()
}

#[cfg(test)]
mod test_hooks {
    use std::path::PathBuf;
    use std::sync::{mpsc::Sender, Mutex};

    use rusqlite::Connection;

    pub struct VersionReadHook {
        database_path: PathBuf,
        sender: Sender<()>,
    }

    static VERSION_READ_HOOK: Mutex<Option<VersionReadHook>> = Mutex::new(None);

    pub fn install_version_read_hook(database_path: PathBuf, sender: Sender<()>) {
        let mut hook = VERSION_READ_HOOK.lock().unwrap();
        assert!(hook.is_none(), "version-read hook already installed");
        *hook = Some(VersionReadHook {
            database_path: database_path.canonicalize().unwrap_or(database_path),
            sender,
        });
    }

    pub fn clear_version_read_hook() {
        *VERSION_READ_HOOK.lock().unwrap() = None;
    }

    pub fn notify_version_read(conn: &Connection) {
        let database_path = conn.path().map(PathBuf::from);
        let hook = VERSION_READ_HOOK.lock().unwrap();
        if hook
            .as_ref()
            .is_some_and(|hook| database_path.as_ref() == Some(&hook.database_path))
        {
            let _ = hook.as_ref().unwrap().sender.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::test_hooks::{clear_version_read_hook, install_version_read_hook};
    use super::{migrate, object_exists, schema_version, MIGRATION_V1};

    #[test]
    fn migrates_empty_database_to_v1() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 1);
        for table in [
            "library_roots",
            "media_items",
            "media_files",
            "source_media",
            "media_fts",
        ] {
            assert!(object_exists(&conn, "table", table), "missing {table}");
        }
        for trigger in ["media_items_ai", "media_items_ad", "media_items_au"] {
            assert!(
                object_exists(&conn, "trigger", trigger),
                "missing {trigger}"
            );
        }
        for index in [
            "media_items_taken_at_idx",
            "media_items_imported_at_idx",
            "media_items_kind_idx",
            "media_files_item_idx",
            "media_files_exists_idx",
        ] {
            assert!(object_exists(&conn, "index", index), "missing {index}");
        }
    }

    #[test]
    fn migration_is_repeatable() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 1);
    }

    #[test]
    fn conflicting_first_table_keeps_schema_version_zero() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE library_roots(id TEXT);")
            .unwrap();

        assert!(migrate(&mut conn).is_err());
        assert_eq!(schema_version(&conn).unwrap(), 0);
    }

    #[test]
    fn failed_migration_rolls_back_version_and_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE media_items(id TEXT);")
            .unwrap();
        assert!(migrate(&mut conn).is_err());
        assert_eq!(schema_version(&conn).unwrap(), 0);
        assert!(!object_exists(&conn, "table", "library_roots"));
        assert!(object_exists(&conn, "table", "media_items"));
    }

    #[test]
    fn fts_tracks_media_item_insert_update_and_delete() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO media_items(remote_key, kind, owner_username, shortcode, caption, imported_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            ("post:1", "post", "alice", "first", "original caption", 1, 1),
        )
        .unwrap();
        assert!(fts_matches(&conn, "alice"));
        assert!(fts_matches(&conn, "original"));

        conn.execute(
            "UPDATE media_items SET owner_username = ?1, caption = ?2 WHERE remote_key = ?3",
            ("bob", "revised caption", "post:1"),
        )
        .unwrap();
        assert!(!fts_matches(&conn, "alice"));
        assert!(!fts_matches(&conn, "original"));
        assert!(fts_matches(&conn, "bob"));
        assert!(fts_matches(&conn, "revised"));

        conn.execute("DELETE FROM media_items WHERE remote_key = ?1", ["post:1"])
            .unwrap();
        assert!(!fts_matches(&conn, "bob"));
        assert!(!fts_matches(&conn, "revised"));
    }

    #[test]
    fn rejects_newer_schema_version_without_mutating_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 2;").unwrap();

        assert!(matches!(
            migrate(&mut conn),
            Err(super::MigrationError::NewerVersion(2))
        ));
        assert_eq!(schema_version(&conn).unwrap(), 2);
        assert!(!object_exists(&conn, "table", "library_roots"));
    }

    #[test]
    fn concurrent_file_migrations_reserve_write_before_reading_version() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("catalog.sqlite3");
        let first = Connection::open(&database_path).unwrap();
        first
            .execute_batch("PRAGMA busy_timeout = 5000; BEGIN IMMEDIATE;")
            .unwrap();

        let (version_read_tx, version_read_rx) = mpsc::channel();
        install_version_read_hook(database_path.clone(), version_read_tx);
        let (started_tx, started_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let second_database_path = database_path.clone();
        let second = std::thread::spawn(move || {
            let mut conn = Connection::open(second_database_path).unwrap();
            conn.execute_batch("PRAGMA busy_timeout = 5000;").unwrap();
            started_tx.send(()).unwrap();
            result_tx.send(migrate(&mut conn)).unwrap();
        });

        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let version_was_read_before_release = version_read_rx
            .recv_timeout(Duration::from_millis(300))
            .is_ok();

        first.execute_batch(MIGRATION_V1).unwrap();
        first
            .execute_batch("PRAGMA user_version = 1; COMMIT;")
            .unwrap();
        let second_result = result_rx.recv_timeout(Duration::from_secs(6)).unwrap();
        second.join().unwrap();
        clear_version_read_hook();

        assert!(
            !version_was_read_before_release,
            "second migration read v0 before reserving the write transaction"
        );
        second_result.unwrap();

        let conn = Connection::open(database_path).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 1);
        assert!(object_exists(&conn, "table", "library_roots"));
    }

    fn fts_matches(conn: &Connection, term: &str) -> bool {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM media_fts WHERE media_fts MATCH ?1)",
            [term],
            |row| row.get(0),
        )
        .unwrap()
    }
}
