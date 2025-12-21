//! Unified Row trait for deserializing query results.
//!
//! This module provides a backend-agnostic way to deserialize rows from ClickHouse.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//! use diesel_clickhouse::row::FromSqlRow;
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize, FromSqlRow)]
//! struct User {
//!     id: u64,
//!     name: String,
//!     active: bool,
//! }
//!
//! let conn = Connection::establish("http://localhost:8123/default").await?;
//! let users: Vec<User> = conn.fetch_all(users::table.filter(users::active.eq(true))).await?;
//! ```

use serde::de::DeserializeOwned;

use crate::core::backend::ClickHouse;
use crate::core::query_builder::QueryFragment;
use crate::core::result::{Error, QueryResult};

/// Trait for types that can be deserialized from a SQL row.
///
/// This is automatically implemented for any type that implements `serde::Deserialize`.
/// Use `#[derive(Deserialize)]` from serde to get this for free.
pub trait FromSqlRow: Sized {
    /// Deserialize from a row.
    fn from_row<R: RowData>(row: &R) -> QueryResult<Self>;
}

/// Trait for accessing row data.
///
/// This abstracts over different row representations from various backends.
pub trait RowData {
    /// Get a column value by name.
    fn get<T: DeserializeOwned>(&self, column: &str) -> QueryResult<T>;

    /// Get all column names.
    fn columns(&self) -> &[String];
}

// Blanket implementation for serde Deserialize types
// This works because we'll convert rows to a format serde can handle
impl<T: DeserializeOwned> FromSqlRow for T {
    fn from_row<R: RowData>(row: &R) -> QueryResult<Self> {
        // Create a map from column names to values
        // Then deserialize using serde
        let columns = row.columns();
        let mut map = serde_json::Map::new();

        for col in columns {
            // Try to get as various types and serialize to JSON value
            if let Ok(v) = row.get::<i64>(col) {
                map.insert(col.clone(), serde_json::Value::Number(v.into()));
            } else if let Ok(v) = row.get::<u64>(col) {
                map.insert(col.clone(), serde_json::Value::Number(v.into()));
            } else if let Ok(v) = row.get::<f64>(col) {
                if let Some(n) = serde_json::Number::from_f64(v) {
                    map.insert(col.clone(), serde_json::Value::Number(n));
                }
            } else if let Ok(v) = row.get::<bool>(col) {
                map.insert(col.clone(), serde_json::Value::Bool(v));
            } else if let Ok(v) = row.get::<String>(col) {
                map.insert(col.clone(), serde_json::Value::String(v));
            }
        }

        serde_json::from_value(serde_json::Value::Object(map))
            .map_err(|e| Error::DeserializationError(e.to_string()))
    }
}

/// Query result wrapper that can be iterated over.
pub struct QueryRows<T> {
    rows: Vec<T>,
}

impl<T> QueryRows<T> {
    /// Create from a vector of rows.
    pub fn new(rows: Vec<T>) -> Self {
        Self { rows }
    }

    /// Convert to a vector.
    pub fn into_vec(self) -> Vec<T> {
        self.rows
    }

    /// Get the number of rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Get the first row, if any.
    pub fn first(self) -> Option<T> {
        self.rows.into_iter().next()
    }
}

impl<T> IntoIterator for QueryRows<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.rows.into_iter()
    }
}

impl<T> From<Vec<T>> for QueryRows<T> {
    fn from(rows: Vec<T>) -> Self {
        Self::new(rows)
    }
}
