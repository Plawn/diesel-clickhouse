//! Error types and Result alias for diesel-clickhouse.

use std::borrow::Cow;
use std::sync::Arc;

use ahash::{HashMap as AHashMap, HashMapExt};
use thiserror::Error;

use crate::interner::InternedSchema;

/// Result type alias for diesel-clickhouse operations.
pub type QueryResult<T> = Result<T, Error>;

/// Error types for diesel-clickhouse.
///
/// Uses `Cow<'static, str>` for error messages to avoid allocations
/// when using static error messages, while still supporting dynamic messages.
#[derive(Error, Debug)]
pub enum Error {
    /// Connection error.
    #[error("Connection error: {0}")]
    ConnectionError(Cow<'static, str>),

    /// Query execution error.
    #[error("Query error: {0}")]
    QueryError(Cow<'static, str>),

    /// Insert operation error.
    #[error("Insert error: {0}")]
    InsertError(Cow<'static, str>),

    /// Serialization error (Rust -> ClickHouse).
    #[error("Serialization error: {0}")]
    SerializationError(Cow<'static, str>),

    /// Deserialization error (ClickHouse -> Rust).
    #[error("Deserialization error: {0}")]
    DeserializationError(Cow<'static, str>),

    /// Column access error during block deserialization.
    ///
    /// This variant avoids allocation by storing components separately
    /// and formatting only when displayed.
    #[error("Failed to get {type_name} column '{column}': {reason}")]
    ColumnAccessError {
        /// The expected type name (e.g., "u64", "String")
        type_name: &'static str,
        /// The column name being accessed
        column: Cow<'static, str>,
        /// The underlying error message
        reason: Cow<'static, str>,
    },

    /// Type mismatch between expected and actual types.
    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        expected: Cow<'static, str>,
        actual: Cow<'static, str>,
    },

    /// Column not found in query result.
    #[error("Column not found: {0}")]
    ColumnNotFound(Cow<'static, str>),

    /// No rows returned when at least one was expected.
    #[error("Not found: query returned no results")]
    NotFound,

    /// Connection pool error.
    #[error("Pool error: {0}")]
    PoolError(Cow<'static, str>),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(Cow<'static, str>),

    /// Invalid query construction.
    #[error("Invalid query: {0}")]
    InvalidQuery(Cow<'static, str>),

    /// ClickHouse server returned an error.
    #[error("ClickHouse server error (code {code}): {message}")]
    ServerError {
        code: i32,
        message: Cow<'static, str>,
    },

    /// Query timeout.
    #[error("Query timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// Type parsing error (from ClickHouse type string).
    #[error("Type parse error: {0}")]
    TypeParseError(Cow<'static, str>),

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Multiple errors (e.g., from batch operations).
    #[error("Multiple errors occurred: {}", format_errors(.0))]
    MultipleErrors(Vec<Error>),
}

fn format_errors(errors: &[Error]) -> String {
    use std::fmt::Write;
    let mut result = String::new();
    let mut iter = errors.iter();
    if let Some(first) = iter.next() {
        // First error - no separator
        let _ = write!(result, "{}", first);
        // Remaining errors with separator
        for e in iter {
            let _ = write!(result, "; {}", e);
        }
    }
    result
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

    /// Create a connection error from a dynamic string.
    pub fn connection(msg: impl Into<String>) -> Self {
        Error::ConnectionError(Cow::Owned(msg.into()))
    }

    /// Create a connection error from a static string (no allocation).
    pub fn connection_static(msg: &'static str) -> Self {
        Error::ConnectionError(Cow::Borrowed(msg))
    }

    /// Create a query error from a dynamic string.
    pub fn query(msg: impl Into<String>) -> Self {
        Error::QueryError(Cow::Owned(msg.into()))
    }

    /// Create a query error from a static string (no allocation).
    pub fn query_static(msg: &'static str) -> Self {
        Error::QueryError(Cow::Borrowed(msg))
    }

    /// Create a serialization error from a dynamic string.
    pub fn serialize(msg: impl Into<String>) -> Self {
        Error::SerializationError(Cow::Owned(msg.into()))
    }

    /// Create a deserialization error from a dynamic string.
    pub fn deserialize(msg: impl Into<String>) -> Self {
        Error::DeserializationError(Cow::Owned(msg.into()))
    }

    /// Create a type mismatch error.
    pub fn type_mismatch(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Error::TypeMismatch {
            expected: Cow::Owned(expected.into()),
            actual: Cow::Owned(actual.into()),
        }
    }

    /// Create a column access error (optimized: deferred formatting).
    ///
    /// This is more efficient than using `DeserializationError` with `format!`
    /// because it stores the components separately and only formats when displayed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// block.get(row_idx, column)
    ///     .map_err(|e| Error::column_access("u64", column, e))
    /// ```
    #[inline]
    pub fn column_access(
        type_name: &'static str,
        column: impl Into<String>,
        reason: impl std::fmt::Display,
    ) -> Self {
        Error::ColumnAccessError {
            type_name,
            column: Cow::Owned(column.into()),
            reason: Cow::Owned(reason.to_string()),
        }
    }

    /// Create a column access error with a static column name (zero allocation for column).
    ///
    /// Use this when the column name is a compile-time constant.
    #[inline]
    pub fn column_access_static(
        type_name: &'static str,
        column: &'static str,
        reason: impl std::fmt::Display,
    ) -> Self {
        Error::ColumnAccessError {
            type_name,
            column: Cow::Borrowed(column),
            reason: Cow::Owned(reason.to_string()),
        }
    }

    /// Create a query error from any error type implementing Display.
    ///
    /// This is designed to be used with `.map_err(Error::query_from)`.
    #[inline]
    pub fn query_from(e: impl std::fmt::Display) -> Self {
        Error::QueryError(Cow::Owned(e.to_string()))
    }

    /// Create a connection error from any error type implementing Display.
    ///
    /// This is designed to be used with `.map_err(Error::connection_from)`.
    #[inline]
    pub fn connection_from(e: impl std::fmt::Display) -> Self {
        Error::ConnectionError(Cow::Owned(e.to_string()))
    }

    /// Create an insert error from any error type implementing Display.
    #[inline]
    pub fn insert_from(e: impl std::fmt::Display) -> Self {
        Error::InsertError(Cow::Owned(e.to_string()))
    }
}

// Conversion from diesel-clickhouse-types errors
impl From<diesel_clickhouse_types::DeserializeError> for Error {
    fn from(e: diesel_clickhouse_types::DeserializeError) -> Self {
        Error::DeserializationError(Cow::Owned(e.to_string()))
    }
}

impl From<diesel_clickhouse_types::SerializeError> for Error {
    fn from(e: diesel_clickhouse_types::SerializeError) -> Self {
        Error::SerializationError(Cow::Owned(e.to_string()))
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
        self.map_err(|e| Error::QueryError(Cow::Owned(format!("{}: {}", context.into(), e))))
    }

    fn with_context<F, S>(self, f: F) -> QueryResult<T>
    where
        F: FnOnce() -> S,
        S: Into<String>,
    {
        self.map_err(|e| Error::QueryError(Cow::Owned(format!("{}: {}", f().into(), e))))
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

    /// Convert to an IndexedRow for O(1) column name lookups.
    pub fn into_indexed(self) -> IndexedRow {
        IndexedRow::from_raw(self)
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

/// Optimized row with O(1) column name lookup.
///
/// This row type builds a HashMap from column names to indices,
/// making repeated column lookups by name much faster.
///
/// Use this when you need to access columns by name multiple times.
///
/// Uses `Arc<str>` internally to share column names between the
/// name list and index map without cloning string data.
/// Uses `AHashMap` for ~30% faster lookups than std HashMap.
#[derive(Debug)]
pub struct IndexedRow {
    /// Column names in order (shared with name_to_index via Arc).
    names: Vec<Arc<str>>,
    /// Column values in order.
    values: Vec<Vec<u8>>,
    /// Name to index mapping for O(1) lookup (shares strings with names).
    /// Uses AHash for faster lookups than std HashMap.
    name_to_index: AHashMap<Arc<str>, usize>,
}

impl IndexedRow {
    /// Create a new indexed row with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            names: Vec::with_capacity(capacity),
            values: Vec::with_capacity(capacity),
            name_to_index: AHashMap::with_capacity(capacity),
        }
    }

    /// Create from a RawRow.
    pub fn from_raw(raw: RawRow) -> Self {
        let mut row = Self::with_capacity(raw.columns.len());
        for (name, value) in raw.columns {
            row.add_column(name, value);
        }
        row
    }

    /// Add a column to the row.
    ///
    /// Uses `Arc<str>` to share the column name without cloning string data.
    pub fn add_column(&mut self, name: impl Into<String>, value: Vec<u8>) {
        let name: Arc<str> = Arc::from(name.into());
        let index = self.names.len();
        self.name_to_index.insert(Arc::clone(&name), index); // Cheap pointer clone
        self.names.push(name);
        self.values.push(value);
    }

    /// Get column index by name in O(1).
    #[inline]
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.name_to_index.get(name).copied()
    }
}

impl Row for IndexedRow {
    #[inline]
    fn column_count(&self) -> usize {
        self.names.len()
    }

    #[inline]
    fn get_by_index(&self, index: usize) -> Option<&[u8]> {
        self.values.get(index).map(|v| v.as_slice())
    }

    #[inline]
    fn get_by_name(&self, name: &str) -> Option<&[u8]> {
        // O(1) lookup using the hash map
        self.name_to_index
            .get(name)
            .and_then(|&idx| self.values.get(idx))
            .map(|v| v.as_slice())
    }

    #[inline]
    fn column_name(&self, index: usize) -> Option<&str> {
        self.names.get(index).map(|s| &**s)
    }
}

/// A shared column index cache for multiple rows with the same schema.
///
/// This is useful when processing many rows with the same columns,
/// as it allows sharing the column name -> index mapping.
///
/// Uses `InternedSchema` internally which provides:
/// - O(1) column name lookup via AHash-based HashMap
/// - Interned strings (Symbol) for fast comparison
/// - SmallVec storage for ≤16 columns (no heap allocation)
/// - Global string deduplication across queries
#[derive(Debug, Clone)]
pub struct ColumnIndex {
    inner: InternedSchema,
}

impl ColumnIndex {
    /// Create a new column index from column names.
    ///
    /// Column names are interned globally for deduplication and fast comparison.
    pub fn new(names: Vec<String>) -> Self {
        Self {
            inner: InternedSchema::new(names.iter().map(String::as_str)),
        }
    }

    /// Get column index by name in O(1).
    #[inline]
    pub fn get(&self, name: &str) -> Option<usize> {
        self.inner.find_column(name)
    }

    /// Get column name by index via closure (zero-allocation).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let matches_id = index.with_name(0, |s| s == "id");
    /// ```
    #[inline]
    pub fn with_name<F, R>(&self, index: usize, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.inner.with_column_name(index, f)
    }

    /// Get column name by index.
    ///
    /// Returns a reference to the interned string (effectively `'static`).
    #[inline]
    pub fn name(&self, index: usize) -> Option<&str> {
        self.inner.column_name(index)
    }

    /// Get the number of columns.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the underlying InternedSchema.
    #[inline]
    pub fn as_interned(&self) -> &InternedSchema {
        &self.inner
    }
}

/// A row that uses a shared column index.
///
/// This is more memory-efficient when processing many rows with
/// the same schema, as the column name mapping is shared.
#[derive(Debug)]
pub struct SharedIndexRow<'a> {
    index: &'a ColumnIndex,
    values: Vec<Vec<u8>>,
}

impl<'a> SharedIndexRow<'a> {
    /// Create a new row with a shared column index.
    pub fn new(index: &'a ColumnIndex, values: Vec<Vec<u8>>) -> Self {
        Self { index, values }
    }
}

impl<'a> Row for SharedIndexRow<'a> {
    #[inline]
    fn column_count(&self) -> usize {
        self.values.len()
    }

    #[inline]
    fn get_by_index(&self, index: usize) -> Option<&[u8]> {
        self.values.get(index).map(|v| v.as_slice())
    }

    #[inline]
    fn get_by_name(&self, name: &str) -> Option<&[u8]> {
        self.index.get(name)
            .and_then(|idx| self.values.get(idx))
            .map(|v| v.as_slice())
    }

    #[inline]
    fn column_name(&self, index: usize) -> Option<&str> {
        self.index.name(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_retryable() {
        assert!(Error::connection_static("test").is_retryable());
        assert!(Error::Timeout(std::time::Duration::from_secs(1)).is_retryable());
        assert!(!Error::query_static("test").is_retryable());
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

    #[test]
    fn test_indexed_row() {
        let mut row = IndexedRow::with_capacity(3);
        row.add_column("id", vec![1, 0, 0, 0]);
        row.add_column("name", b"test".to_vec());
        row.add_column("active", vec![1]);

        assert_eq!(row.column_count(), 3);
        assert_eq!(row.get_by_index(0), Some([1, 0, 0, 0].as_slice()));
        assert_eq!(row.get_by_name("name"), Some(b"test".as_slice()));
        assert_eq!(row.get_by_name("active"), Some([1].as_slice()));
        assert_eq!(row.column_index("name"), Some(1));
        assert_eq!(row.column_name(0), Some("id"));
    }

    #[test]
    fn test_column_index() {
        let index = ColumnIndex::new(vec![
            "id".to_string(),
            "name".to_string(),
            "active".to_string(),
        ]);

        assert_eq!(index.len(), 3);
        assert_eq!(index.get("id"), Some(0));
        assert_eq!(index.get("name"), Some(1));
        assert_eq!(index.get("active"), Some(2));
        assert_eq!(index.get("missing"), None);
        assert_eq!(index.name(0), Some("id"));
    }

    #[test]
    fn test_shared_index_row() {
        let index = ColumnIndex::new(vec![
            "id".to_string(),
            "name".to_string(),
        ]);

        let row = SharedIndexRow::new(&index, vec![
            vec![42, 0, 0, 0],
            b"alice".to_vec(),
        ]);

        assert_eq!(row.column_count(), 2);
        assert_eq!(row.get_by_name("id"), Some([42, 0, 0, 0].as_slice()));
        assert_eq!(row.get_by_name("name"), Some(b"alice".as_slice()));
        assert_eq!(row.column_name(1), Some("name"));
    }
}
