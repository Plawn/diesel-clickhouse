//! Error types and Result alias for diesel-clickhouse.

use thiserror::Error;

/// Result type alias for diesel-clickhouse operations.
pub type QueryResult<T> = Result<T, Error>;

/// Error types for diesel-clickhouse.
#[derive(Error, Debug)]
pub enum Error {
    /// Connection error.
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// Query execution error.
    #[error("Query error: {0}")]
    QueryError(String),

    /// Insert operation error.
    #[error("Insert error: {0}")]
    InsertError(String),

    /// Serialization error (Rust -> ClickHouse).
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Deserialization error (ClickHouse -> Rust).
    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    /// Type mismatch between expected and actual types.
    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        expected: String,
        actual: String,
    },

    /// Column not found in query result.
    #[error("Column not found: {0}")]
    ColumnNotFound(String),

    /// No rows returned when at least one was expected.
    #[error("Not found: query returned no results")]
    NotFound,

    /// Connection pool error.
    #[error("Pool error: {0}")]
    PoolError(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Invalid query construction.
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    /// ClickHouse server returned an error.
    #[error("ClickHouse server error (code {code}): {message}")]
    ServerError {
        code: i32,
        message: String,
    },

    /// Query timeout.
    #[error("Query timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Multiple errors (e.g., from batch operations).
    #[error("Multiple errors occurred: {}", format_errors(.0))]
    MultipleErrors(Vec<Error>),
}

fn format_errors(errors: &[Error]) -> String {
    errors.iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

impl Error {
    /// Returns true if this error is retryable.
    ///
    /// Connection errors and timeouts are typically retryable,
    /// while query errors and type mismatches are not.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::ConnectionError(_) |
            Error::Timeout(_) |
            Error::PoolError(_)
        )
    }

    /// Returns true if this is a "not found" error.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::NotFound)
    }

    /// Returns the ClickHouse error code if this is a server error.
    pub fn server_code(&self) -> Option<i32> {
        match self {
            Error::ServerError { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// Create a connection error.
    pub fn connection(msg: impl Into<String>) -> Self {
        Error::ConnectionError(msg.into())
    }

    /// Create a query error.
    pub fn query(msg: impl Into<String>) -> Self {
        Error::QueryError(msg.into())
    }

    /// Create a serialization error.
    pub fn serialize(msg: impl Into<String>) -> Self {
        Error::SerializationError(msg.into())
    }

    /// Create a deserialization error.
    pub fn deserialize(msg: impl Into<String>) -> Self {
        Error::DeserializationError(msg.into())
    }

    /// Create a type mismatch error.
    pub fn type_mismatch(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Error::TypeMismatch {
            expected: expected.into(),
            actual: actual.into(),
        }
    }
}

// Conversion from diesel-clickhouse-types errors
impl From<diesel_clickhouse_types::DeserializeError> for Error {
    fn from(e: diesel_clickhouse_types::DeserializeError) -> Self {
        Error::DeserializationError(e.to_string())
    }
}

impl From<diesel_clickhouse_types::SerializeError> for Error {
    fn from(e: diesel_clickhouse_types::SerializeError) -> Self {
        Error::SerializationError(e.to_string())
    }
}

/// Extension trait for adding context to errors.
pub trait ResultExt<T> {
    /// Add context to an error.
    fn context(self, context: impl Into<String>) -> QueryResult<T>;

    /// Add context using a closure (only called on error).
    fn with_context<F, S>(self, f: F) -> QueryResult<T>
    where
        F: FnOnce() -> S,
        S: Into<String>;
}

impl<T> ResultExt<T> for QueryResult<T> {
    fn context(self, context: impl Into<String>) -> QueryResult<T> {
        self.map_err(|e| Error::QueryError(format!("{}: {}", context.into(), e)))
    }

    fn with_context<F, S>(self, f: F) -> QueryResult<T>
    where
        F: FnOnce() -> S,
        S: Into<String>,
    {
        self.map_err(|e| Error::QueryError(format!("{}: {}", f().into(), e)))
    }
}

/// A database row that can report column information.
pub trait Row {
    /// Get the number of columns in this row.
    fn column_count(&self) -> usize;

    /// Get a column value by index.
    fn get_by_index(&self, index: usize) -> Option<&[u8]>;

    /// Get a column value by name.
    fn get_by_name(&self, name: &str) -> Option<&[u8]>;

    /// Get the column name by index.
    fn column_name(&self, index: usize) -> Option<&str>;
}

/// Placeholder row implementation for development.
#[derive(Debug, Default)]
pub struct RawRow {
    columns: Vec<(String, Vec<u8>)>,
}

impl RawRow {
    /// Create a new empty row.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a column to the row.
    pub fn add_column(&mut self, name: impl Into<String>, value: Vec<u8>) {
        self.columns.push((name.into(), value));
    }
}

impl Row for RawRow {
    fn column_count(&self) -> usize {
        self.columns.len()
    }

    fn get_by_index(&self, index: usize) -> Option<&[u8]> {
        self.columns.get(index).map(|(_, v)| v.as_slice())
    }

    fn get_by_name(&self, name: &str) -> Option<&[u8]> {
        self.columns.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_slice())
    }

    fn column_name(&self, index: usize) -> Option<&str> {
        self.columns.get(index).map(|(n, _)| n.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_retryable() {
        assert!(Error::ConnectionError("test".into()).is_retryable());
        assert!(Error::Timeout(std::time::Duration::from_secs(1)).is_retryable());
        assert!(!Error::QueryError("test".into()).is_retryable());
        assert!(!Error::NotFound.is_retryable());
    }

    #[test]
    fn test_raw_row() {
        let mut row = RawRow::new();
        row.add_column("id", vec![1, 0, 0, 0]);
        row.add_column("name", b"test".to_vec());

        assert_eq!(row.column_count(), 2);
        assert_eq!(row.get_by_index(0), Some([1, 0, 0, 0].as_slice()));
        assert_eq!(row.get_by_name("name"), Some(b"test".as_slice()));
        assert_eq!(row.column_name(0), Some("id"));
    }
}
