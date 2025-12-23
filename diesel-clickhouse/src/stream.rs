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
//! | Native | True streaming (block-based) | O(block_size) per block |
//!
//! Both backends now support true streaming. The Native backend uses a background
//! task that reads blocks and sends rows through a channel, providing memory-efficient
//! processing for large result sets.
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
//! // Stream results row by row - true streaming for both backends!
//! let mut stream = conn.stream::<User, _>(
//!     users::table.filter(users::active.eq(true))
//! ).await?;
//!
//! while let Some(user) = stream.next().await? {
//!     println!("User: {} - {}", user.id, user.name);
//! }
//! ```

use crate::core::result::QueryResult;
#[cfg(any(feature = "http", feature = "native"))]
use crate::core::result::Error;

/// A unified stream of rows from a query.
///
/// # Streaming Behavior
///
/// - **HTTP backend**: True streaming via `RowCursor`. Rows are fetched incrementally
///   from the server, providing O(1) memory usage per row. Ideal for large result sets.
///
/// - **Native backend**: True streaming via `NativeBlockStream`. A background task reads
///   blocks from the server and sends rows through a channel. Memory usage is O(block_size)
///   where block_size is typically ~65K rows.
///
/// Both backends now support memory-efficient streaming for large result sets.
pub enum RowStream<T> {
    /// HTTP backend cursor (true streaming)
    #[cfg(feature = "http")]
    Http(clickhouse::query::RowCursor<T>),

    /// Native backend stream (true block-based streaming via channel)
    #[cfg(feature = "native")]
    Native(NativeBlockStream<T>),
}

/// True streaming iterator over rows from a Native backend.
///
/// This struct uses a background task that reads blocks from the server
/// and sends deserialized rows through a channel. This provides true streaming
/// with O(block_size) memory usage instead of O(n).
#[cfg(feature = "native")]
pub struct NativeBlockStream<T> {
    receiver: tokio::sync::mpsc::Receiver<QueryResult<T>>,
}

#[cfg(feature = "native")]
impl<T> NativeBlockStream<T> {
    /// Create a new block stream from a channel receiver.
    pub fn new(receiver: tokio::sync::mpsc::Receiver<QueryResult<T>>) -> Self {
        Self { receiver }
    }

    /// Get the next row from the stream.
    ///
    /// Returns `Ok(Some(row))` if a row is available,
    /// `Ok(None)` when the stream is exhausted,
    /// or `Err(e)` if an error occurs.
    pub async fn next(&mut self) -> QueryResult<Option<T>> {
        match self.receiver.recv().await {
            Some(Ok(row)) => Ok(Some(row)),
            Some(Err(e)) => Err(e),
            None => Ok(None), // Stream exhausted
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
                    .map_err(Error::query_from)
            }
            #[cfg(feature = "native")]
            RowStream::Native(stream) => stream.next().await,
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
            RowStream::Native(stream) => stream.next().await,
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
impl<T> From<NativeBlockStream<T>> for RowStream<T> {
    fn from(stream: NativeBlockStream<T>) -> Self {
        RowStream::Native(stream)
    }
}
