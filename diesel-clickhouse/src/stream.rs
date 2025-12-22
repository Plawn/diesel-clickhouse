//! Unified streaming interface for query results.
//!
//! This module provides `RowStream`, a unified way to stream query results
//! that works with both HTTP and Native backends.
//!
//! # Backend Differences
//!
//! | Backend | Streaming Type | Memory Usage |
//! |---------|---------------|--------------|
//! | HTTP | True streaming | O(1) per row |
//! | Native | Buffered iteration | O(n) - all rows loaded |
//!
//! **Important:** The Native backend (`clickhouse-rs` crate) does not support true
//! streaming - it loads all results into memory with `fetch_all()`. For large result
//! sets, prefer the HTTP backend which provides genuine row-by-row streaming.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//!
//! #[derive(Debug, Row)]
//! struct User {
//!     id: u64,
//!     name: String,
//! }
//!
//! // Stream results row by row
//! // HTTP: true streaming (memory efficient)
//! // Native: buffered iteration (all rows in memory)
//! let mut stream = conn.stream::<User, _>(
//!     users::table.filter(users::active.eq(true))
//! ).await?;
//!
//! while let Some(user) = stream.next().await? {
//!     println!("User: {} - {}", user.id, user.name);
//! }
//! ```

use crate::core::result::QueryResult;
#[cfg(feature = "http")]
use crate::core::result::Error;

/// A unified stream of rows from a query.
///
/// # Streaming Behavior
///
/// - **HTTP backend**: True streaming via `RowCursor`. Rows are fetched incrementally
///   from the server, providing O(1) memory usage per row. Ideal for large result sets.
///
/// - **Native backend**: Buffered iteration. Due to limitations in the `clickhouse-rs`
///   crate (which only provides `fetch_all()`), all rows are loaded into memory first,
///   then iterated. Memory usage is O(n) where n is the number of rows.
///
/// For memory-efficient processing of large result sets, use the HTTP backend.
pub enum RowStream<T> {
    /// HTTP backend cursor (true streaming)
    #[cfg(feature = "http")]
    Http(clickhouse::query::RowCursor<T>),

    /// Native backend iterator (block loaded in memory)
    #[cfg(feature = "native")]
    Native(NativeRowIter<T>),
}

/// Iterator over rows from a Native backend Block.
#[cfg(feature = "native")]
pub struct NativeRowIter<T> {
    rows: std::vec::IntoIter<T>,
}

#[cfg(feature = "native")]
impl<T> NativeRowIter<T> {
    /// Create a new iterator from a vector of rows.
    pub fn new(rows: Vec<T>) -> Self {
        Self {
            rows: rows.into_iter(),
        }
    }
}


// HTTP-specific implementation (requires clickhouse::Row bounds)
// RowOwned ensures T::Value<'_> = T (owned type, not borrowed)
// RowRead is required for cursor.next()
#[cfg(feature = "http")]
impl<T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead> RowStream<T> {
    /// Get the next row from the stream.
    ///
    /// Returns `Ok(Some(row))` if a row is available,
    /// `Ok(None)` when the stream is exhausted,
    /// or `Err(e)` if an error occurs.
    pub async fn next(&mut self) -> QueryResult<Option<T>> {
        match self {
            RowStream::Http(cursor) => {
                cursor
                    .next()
                    .await
                    .map_err(|e| Error::QueryError(e.to_string()))
            }
            #[cfg(feature = "native")]
            RowStream::Native(iter) => Ok(iter.rows.next()),
        }
    }

    /// Collect all remaining rows into a vector.
    pub async fn collect(mut self) -> QueryResult<Vec<T>> {
        let mut results = Vec::new();
        while let Some(row) = self.next().await? {
            results.push(row);
        }
        Ok(results)
    }

    /// Process each row with a closure.
    pub async fn for_each<F>(mut self, mut f: F) -> QueryResult<()>
    where
        F: FnMut(T),
    {
        while let Some(row) = self.next().await? {
            f(row);
        }
        Ok(())
    }

    /// Process each row with an async closure.
    pub async fn for_each_async<F, Fut>(mut self, mut f: F) -> QueryResult<()>
    where
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        while let Some(row) = self.next().await? {
            f(row).await;
        }
        Ok(())
    }
}

// Native-only implementation (no HTTP dependency)
#[cfg(all(feature = "native", not(feature = "http")))]
impl<T> RowStream<T> {
    /// Get the next row from the stream.
    pub async fn next(&mut self) -> QueryResult<Option<T>> {
        match self {
            RowStream::Native(iter) => Ok(iter.rows.next()),
        }
    }

    /// Collect all remaining rows into a vector.
    pub async fn collect(mut self) -> QueryResult<Vec<T>> {
        let mut results = Vec::new();
        while let Some(row) = self.next().await? {
            results.push(row);
        }
        Ok(results)
    }

    /// Process each row with a closure.
    pub async fn for_each<F>(mut self, mut f: F) -> QueryResult<()>
    where
        F: FnMut(T),
    {
        while let Some(row) = self.next().await? {
            f(row);
        }
        Ok(())
    }

    /// Process each row with an async closure.
    pub async fn for_each_async<F, Fut>(mut self, mut f: F) -> QueryResult<()>
    where
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        while let Some(row) = self.next().await? {
            f(row).await;
        }
        Ok(())
    }
}

#[cfg(feature = "http")]
impl<T> From<clickhouse::query::RowCursor<T>> for RowStream<T> {
    fn from(cursor: clickhouse::query::RowCursor<T>) -> Self {
        RowStream::Http(cursor)
    }
}

#[cfg(feature = "native")]
impl<T> From<Vec<T>> for RowStream<T> {
    fn from(rows: Vec<T>) -> Self {
        RowStream::Native(NativeRowIter::new(rows))
    }
}
