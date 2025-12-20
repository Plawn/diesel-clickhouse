//! Error types for migrations.

use std::path::PathBuf;

/// Result type for migration operations.
pub type Result<T> = std::result::Result<T, MigrationError>;

/// Errors that can occur during migration operations.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    /// Migration directory not found.
    #[error("Migration directory not found: {0}")]
    DirectoryNotFound(PathBuf),

    /// Migration file not found.
    #[error("Migration file not found: {path} (expected {expected})")]
    FileNotFound {
        path: PathBuf,
        expected: &'static str,
    },

    /// Invalid migration name format.
    #[error("Invalid migration name: {name}. Expected format: YYYYMMDDHHMMSS_name or NNNNN_name")]
    InvalidMigrationName { name: String },

    /// Migration already exists.
    #[error("Migration already exists: {0}")]
    MigrationExists(String),

    /// Migration not found in database.
    #[error("Migration not found: {0}")]
    MigrationNotFound(String),

    /// SQL execution error.
    #[error("SQL execution error in migration '{migration}': {message}")]
    SqlError {
        migration: String,
        message: String,
    },

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Database connection error.
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// Migration version mismatch.
    #[error("Migration version mismatch: expected {expected}, found {found}")]
    VersionMismatch {
        expected: String,
        found: String,
    },

    /// No pending migrations.
    #[error("No pending migrations to run")]
    NoPendingMigrations,

    /// No migrations to revert.
    #[error("No migrations to revert")]
    NoMigrationsToRevert,

    /// UTF-8 decoding error.
    #[error("UTF-8 error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    /// Parse error for migration content.
    #[error("Parse error: {0}")]
    ParseError(String),
}

impl MigrationError {
    /// Create a SQL error.
    pub fn sql_error(migration: impl Into<String>, message: impl Into<String>) -> Self {
        Self::SqlError {
            migration: migration.into(),
            message: message.into(),
        }
    }

    /// Create a database error.
    pub fn database_error(message: impl Into<String>) -> Self {
        Self::DatabaseError(message.into())
    }
}

impl From<diesel_clickhouse_core::result::Error> for MigrationError {
    fn from(err: diesel_clickhouse_core::result::Error) -> Self {
        Self::DatabaseError(err.to_string())
    }
}
