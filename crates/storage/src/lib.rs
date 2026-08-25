#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod paths;
pub mod sqlite;

pub use paths::DataPaths;
pub use sqlite::Database;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryStats {
    pub items: u64,
    pub revisions: u64,
    pub evidence: u64,
    pub unknown_cases: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditStats {
    pub records: u64,
    pub executions: u64,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("XDG_DATA_HOME is unset and HOME is unavailable")]
    MissingDataHome,
    #[error("insecure data path: {0}")]
    InsecureDataPath(String),
    #[error("SQLite did not enable foreign key enforcement")]
    ForeignKeysUnavailable,
    #[error("SQLite did not enter WAL mode (reported {0})")]
    WalUnavailable(String),
    #[error("migration {version} failed: {source}")]
    Migration {
        version: i64,
        #[source]
        source: rusqlite::Error,
    },
    #[error("invalid memory change: {0}")]
    InvalidMemoryChange(String),
    #[error("invalid internal static query")]
    InvalidStaticQuery,
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}
