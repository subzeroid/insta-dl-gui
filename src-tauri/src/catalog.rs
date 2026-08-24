pub mod migrations;
pub mod models;
mod repository;

pub use models::*;
pub use repository::local_remote_key;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::Connection;
use thiserror::Error;

#[derive(Clone)]
pub struct Catalog {
    path: Arc<PathBuf>,
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("could not create catalog directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not open catalog database {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("could not configure catalog database {path}: {source}")]
    Configure {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("could not migrate catalog database {path}: {source}")]
    Migrate {
        path: PathBuf,
        #[source]
        source: migrations::MigrationError,
    },
    #[error("I/O error while {operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("SQLite error while {operation}: {source}")]
    Sql {
        operation: &'static str,
        #[source]
        source: rusqlite::Error,
    },
    #[error("{entity} {id} was not found")]
    NotFound { entity: &'static str, id: i64 },
    #[error("invalid relative path {path}")]
    InvalidRelativePath { path: PathBuf },
    #[error("path is not valid UTF-8: {path}")]
    NonUtf8Path { path: PathBuf },
    #[error("invalid catalog input: {message}")]
    InvalidInput { message: String },
    #[error("invalid library cursor: {message}")]
    InvalidCursor { message: String },
    #[error("catalog batch has {size} items; maximum is {max}")]
    BatchTooLarge { size: usize, max: usize },
}

impl Catalog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CatalogError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|source| CatalogError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let catalog = Self {
            path: Arc::new(path),
        };
        let mut conn = catalog.connect()?;
        migrations::migrate(&mut conn).map_err(|source| CatalogError::Migrate {
            path: (*catalog.path).clone(),
            source,
        })?;
        Ok(catalog)
    }

    fn connect(&self) -> Result<Connection, CatalogError> {
        let conn = Connection::open(self.path.as_ref()).map_err(|source| CatalogError::Open {
            path: (*self.path).clone(),
            source,
        })?;
        conn.execute_batch(
            "
                PRAGMA foreign_keys = ON;
                PRAGMA journal_mode = WAL;
                PRAGMA busy_timeout = 5000;
            ",
        )
        .map_err(|source| CatalogError::Configure {
            path: (*self.path).clone(),
            source,
        })?;
        Ok(conn)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::Catalog;

    #[test]
    fn open_creates_parent_directory_and_configures_connections() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("nested/catalog.sqlite3");
        let catalog = Catalog::open(&database_path).unwrap();

        assert!(database_path.parent().unwrap().is_dir());
        let conn = catalog.connect().unwrap();
        assert_eq!(
            conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        assert_eq!(
            conn.query_row("PRAGMA busy_timeout", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            5000
        );
    }
}
