//! Async insert support for high-throughput data ingestion.
//!
//! This module provides support for ClickHouse's async insert mode, which buffers
//! data server-side for optimal write performance. This is ideal for high-frequency
//! event streaming where you can tolerate small delays before data becomes queryable.
//!
//! # Backend Implementations
//!
//! Each backend uses its optimal binary format:
//!
//! - **HTTP**: Uses RowBinary format via `clickhouse::Insert` with async insert options
//! - **Native**: Uses native Block format with session-level async insert settings
//!
//! # Usage
//!
//! ```rust,ignore
//! use diesel_clickhouse::async_insert::{AsyncInsertConfig, AsyncInsertExt};
//!
//! // Create an inserter (buffers locally)
//! let inserter = conn.clone().async_inserter::<events::table, NewEvent>(
//!     AsyncInsertConfig::fire_and_forget()
//! );
//!
//! // Write rows (buffered locally, not sent yet)
//! inserter.write(event1).await;
//! inserter.write(event2).await;
//! inserter.write(event3).await;
//!
//! // Send buffered rows to server
//! inserter.flush().await?;
//!
//! // Force server to write its async buffer to disk (optional)
//! inserter.flush_server().await?;
//! ```

use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tokio::sync::Mutex;

use crate::core::query_source::Table;
use crate::core::result::QueryResult;
use crate::Connection;

// =============================================================================
// AsyncInsertConfig
// =============================================================================

/// Configuration for ClickHouse async insert mode.
///
/// This builder configures the server-side async insert behavior.
/// Use preset methods like `fire_and_forget()` or `synchronous()` for common configs.
#[derive(Debug, Clone)]
pub struct AsyncInsertConfig {
    /// Whether to wait for async insert to complete (default: false for fire-and-forget)
    pub(crate) wait: bool,
    /// Timeout in milliseconds before forcing a flush (default: 200)
    pub(crate) busy_timeout_ms: Option<u64>,
    /// Maximum data size in bytes before flush (default: 10MB)
    pub(crate) max_data_size: Option<u64>,
    /// Maximum number of queries before flush (default: 450)
    pub(crate) max_query_number: Option<u64>,
    /// Whether to deduplicate across materialized views (for ReplicatedMergeTree)
    pub(crate) deduplicate_materialized_views: Option<bool>,
}

impl Default for AsyncInsertConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncInsertConfig {
    /// Create a new async insert configuration with defaults.
    pub fn new() -> Self {
        Self {
            wait: false,
            busy_timeout_ms: None,
            max_data_size: None,
            max_query_number: None,
            deduplicate_materialized_views: None,
        }
    }

    /// Create a fire-and-forget configuration (highest throughput).
    ///
    /// The INSERT returns immediately after the server accepts the data.
    /// Data is buffered server-side and flushed asynchronously.
    ///
    /// **Warning**: In case of server crash, buffered data may be lost.
    pub fn fire_and_forget() -> Self {
        Self {
            wait: false,
            busy_timeout_ms: Some(200),
            ..Self::new()
        }
    }

    /// Create a synchronous configuration (highest durability).
    ///
    /// The INSERT waits for the server to flush data to disk before returning.
    pub fn synchronous() -> Self {
        Self {
            wait: true,
            ..Self::new()
        }
    }

    /// Set whether to wait for async insert completion.
    pub fn wait_for_async_insert(mut self, wait: bool) -> Self {
        self.wait = wait;
        self
    }

    /// Set the busy timeout in milliseconds.
    pub fn async_insert_busy_timeout_ms(mut self, ms: u64) -> Self {
        self.busy_timeout_ms = Some(ms);
        self
    }

    /// Set the maximum data size in bytes before flush.
    pub fn async_insert_max_data_size(mut self, bytes: u64) -> Self {
        self.max_data_size = Some(bytes);
        self
    }

    /// Set the maximum number of queries before flush.
    pub fn async_insert_max_query_number(mut self, count: u64) -> Self {
        self.max_query_number = Some(count);
        self
    }

    /// Enable deduplication across materialized views.
    pub fn deduplicate_materialized_views(mut self, enabled: bool) -> Self {
        self.deduplicate_materialized_views = Some(enabled);
        self
    }

    /// Check if this config waits for completion.
    pub fn is_synchronous(&self) -> bool {
        self.wait
    }

    /// Generate the SQL SETTINGS string for this configuration.
    pub fn to_settings_sql(&self) -> String {
        let mut parts = vec![String::from("async_insert=1")];
        parts.push(format!(
            "wait_for_async_insert={}",
            if self.wait { 1 } else { 0 }
        ));
        if let Some(ms) = self.busy_timeout_ms {
            parts.push(format!("async_insert_busy_timeout_ms={}", ms));
        }
        if let Some(size) = self.max_data_size {
            parts.push(format!("async_insert_max_data_size={}", size));
        }
        if let Some(count) = self.max_query_number {
            parts.push(format!("async_insert_max_query_number={}", count));
        }
        if let Some(dedup) = self.deduplicate_materialized_views {
            parts.push(format!(
                "async_insert_deduplicate={}",
                if dedup { 1 } else { 0 }
            ));
        }
        format!("SETTINGS {}", parts.join(", "))
    }
}

// HTTP-specific configuration methods
#[cfg(feature = "http")]
impl AsyncInsertConfig {
    /// Apply async insert options to a clickhouse Insert (HTTP backend).
    pub fn apply_to_http_insert<R: clickhouse::Row>(
        &self,
        mut insert: clickhouse::insert::Insert<R>,
    ) -> clickhouse::insert::Insert<R> {
        insert = insert
            .with_option("async_insert", "1")
            .with_option("wait_for_async_insert", if self.wait { "1" } else { "0" });

        if let Some(ms) = self.busy_timeout_ms {
            insert = insert.with_option("async_insert_busy_timeout_ms", ms.to_string());
        }
        if let Some(size) = self.max_data_size {
            insert = insert.with_option("async_insert_max_data_size", size.to_string());
        }
        if let Some(count) = self.max_query_number {
            insert = insert.with_option("async_insert_max_query_number", count.to_string());
        }
        if let Some(dedup) = self.deduplicate_materialized_views {
            insert = insert.with_option("async_insert_deduplicate", if dedup { "1" } else { "0" });
        }
        insert
    }
}

// Native-specific configuration methods
#[cfg(feature = "native")]
impl AsyncInsertConfig {
    /// Generate SET commands for session-level async insert settings (Native backend).
    pub fn to_native_set_commands(&self) -> Vec<String> {
        let mut commands = vec![
            "SET async_insert = 1".to_string(),
            format!(
                "SET wait_for_async_insert = {}",
                if self.wait { 1 } else { 0 }
            ),
        ];

        if let Some(ms) = self.busy_timeout_ms {
            commands.push(format!("SET async_insert_busy_timeout_ms = {}", ms));
        }
        if let Some(size) = self.max_data_size {
            commands.push(format!("SET async_insert_max_data_size = {}", size));
        }
        if let Some(count) = self.max_query_number {
            commands.push(format!("SET async_insert_max_query_number = {}", count));
        }
        if let Some(dedup) = self.deduplicate_materialized_views {
            commands.push(format!(
                "SET async_insert_deduplicate = {}",
                if dedup { 1 } else { 0 }
            ));
        }
        commands
    }
}

// =============================================================================
// AsyncInserter
// =============================================================================

/// Buffered async inserter with local batching.
///
/// Rows are buffered locally and only sent to the server when `flush()` is called.
/// This provides true client-side batching combined with server-side async insert mode.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::async_insert::{AsyncInsertConfig, AsyncInserter};
///
/// let inserter = AsyncInserter::<events::table, NewEvent>::new(
///     conn.clone(),
///     AsyncInsertConfig::fire_and_forget(),
/// );
///
/// // Write rows (buffered locally)
/// inserter.write(event1).await;
/// inserter.write(event2).await;
///
/// // Send to server
/// inserter.flush().await?;
///
/// println!("Sent {} rows", inserter.sent_count());
/// ```
pub struct AsyncInserter<T, R> {
    conn: Connection,
    config: AsyncInsertConfig,
    buffer: Mutex<Vec<R>>,
    sent_count: AtomicU64,
    #[cfg(feature = "native")]
    settings_applied: AtomicBool,
    _marker: PhantomData<T>,
}

impl<T, R> AsyncInserter<T, R>
where
    T: Table,
{
    /// Create a new async inserter.
    pub fn new(conn: Connection, config: AsyncInsertConfig) -> Self {
        Self {
            conn,
            config,
            buffer: Mutex::new(Vec::new()),
            sent_count: AtomicU64::new(0),
            #[cfg(feature = "native")]
            settings_applied: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    /// Create a new async inserter with pre-allocated buffer capacity.
    pub fn with_capacity(conn: Connection, config: AsyncInsertConfig, capacity: usize) -> Self {
        Self {
            conn,
            config,
            buffer: Mutex::new(Vec::with_capacity(capacity)),
            sent_count: AtomicU64::new(0),
            #[cfg(feature = "native")]
            settings_applied: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    /// Get the number of rows currently buffered locally.
    pub async fn buffered_count(&self) -> usize {
        self.buffer.lock().await.len()
    }

    /// Get the total number of rows sent to the server.
    pub fn sent_count(&self) -> u64 {
        self.sent_count.load(Ordering::Relaxed)
    }

    /// Get the configuration.
    pub fn config(&self) -> &AsyncInsertConfig {
        &self.config
    }

    /// Force the server to flush its async insert buffer to disk.
    ///
    /// This is useful when you need to query data immediately after inserting.
    /// Note: This flushes ALL async insert buffers on the server, not just yours.
    pub async fn flush_server(&self) -> QueryResult<()> {
        self.conn.execute("SYSTEM FLUSH ASYNC INSERT QUEUE").await
    }
}

// HTTP + Native
#[cfg(all(feature = "http", feature = "native"))]
impl<T, R> AsyncInserter<T, R>
where
    T: Table,
    R: clickhouse::Row + serde::Serialize + crate::native::ToNativeBlock + Send + Sync,
    for<'a> R: clickhouse::Row<Value<'a> = R>,
    for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
{
    /// Write a single row to the local buffer.
    ///
    /// The row is not sent to the server until `flush()` is called.
    pub async fn write(&self, row: R) {
        self.buffer.lock().await.push(row);
    }

    /// Write multiple rows to the local buffer.
    ///
    /// The rows are not sent to the server until `flush()` is called.
    pub async fn write_many(&self, rows: impl IntoIterator<Item = R>) {
        self.buffer.lock().await.extend(rows);
    }

    /// Flush the local buffer to the server.
    ///
    /// Sends all buffered rows using the configured async insert settings.
    /// After this call, the local buffer is empty.
    pub async fn flush(&self) -> QueryResult<()> {
        let rows = std::mem::take(&mut *self.buffer.lock().await);
        if rows.is_empty() {
            return Ok(());
        }

        match &self.conn {
            Connection::Http(conn) => {
                conn.async_insert_rows::<T, R>(&self.config, &rows).await?;
            }
            Connection::Native(conn) => {
                // Apply settings once per inserter
                if !self.settings_applied.swap(true, Ordering::SeqCst) {
                    for cmd in self.config.to_native_set_commands() {
                        conn.execute_raw(&cmd).await?;
                    }
                }
                let block = R::rows_to_block(&rows)?;
                conn.insert(T::table_name(), block).await?;
            }
        }

        self.sent_count.fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// Flush local buffer and then flush server's async insert queue.
    ///
    /// Use this when you need data to be immediately queryable.
    pub async fn flush_all(&self) -> QueryResult<()> {
        self.flush().await?;
        self.flush_server().await
    }
}

// HTTP only
#[cfg(all(feature = "http", not(feature = "native")))]
impl<T, R> AsyncInserter<T, R>
where
    T: Table,
    R: clickhouse::Row + serde::Serialize + Send + Sync,
    for<'a> R: clickhouse::Row<Value<'a> = R>,
    for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
{
    /// Write a single row to the local buffer.
    pub async fn write(&self, row: R) {
        self.buffer.lock().await.push(row);
    }

    /// Write multiple rows to the local buffer.
    pub async fn write_many(&self, rows: impl IntoIterator<Item = R>) {
        self.buffer.lock().await.extend(rows);
    }

    /// Flush the local buffer to the server.
    pub async fn flush(&self) -> QueryResult<()> {
        let rows = std::mem::take(&mut *self.buffer.lock().await);
        if rows.is_empty() {
            return Ok(());
        }

        let Connection::Http(conn) = &self.conn;
        conn.async_insert_rows::<T, R>(&self.config, &rows).await?;
        self.sent_count.fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// Flush local buffer and then flush server's async insert queue.
    pub async fn flush_all(&self) -> QueryResult<()> {
        self.flush().await?;
        self.flush_server().await
    }
}

// Native only
#[cfg(all(feature = "native", not(feature = "http")))]
impl<T, R> AsyncInserter<T, R>
where
    T: Table,
    R: crate::native::ToNativeBlock + Send,
{
    /// Write a single row to the local buffer.
    pub async fn write(&self, row: R) {
        self.buffer.lock().await.push(row);
    }

    /// Write multiple rows to the local buffer.
    pub async fn write_many(&self, rows: impl IntoIterator<Item = R>) {
        self.buffer.lock().await.extend(rows);
    }

    /// Flush the local buffer to the server.
    pub async fn flush(&self) -> QueryResult<()> {
        let rows = std::mem::take(&mut *self.buffer.lock().await);
        if rows.is_empty() {
            return Ok(());
        }

        let Connection::Native(conn) = &self.conn;

        // Apply settings once per inserter
        if !self.settings_applied.swap(true, Ordering::SeqCst) {
            for cmd in self.config.to_native_set_commands() {
                conn.execute_raw(&cmd).await?;
            }
        }

        let block = R::rows_to_block(&rows)?;
        conn.insert(T::table_name(), block).await?;
        self.sent_count.fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// Flush local buffer and then flush server's async insert queue.
    pub async fn flush_all(&self) -> QueryResult<()> {
        self.flush().await?;
        self.flush_server().await
    }
}

// =============================================================================
// AsyncInsertExt - Extension trait for Connection
// =============================================================================

/// Extension trait for async insert operations on connections.
pub trait AsyncInsertExt {
    /// Create an async inserter for a table.
    ///
    /// The inserter buffers rows locally until `flush()` is called.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::async_insert::{AsyncInsertConfig, AsyncInsertExt};
    ///
    /// let inserter = conn.clone().async_inserter::<events::table, NewEvent>(
    ///     AsyncInsertConfig::fire_and_forget()
    /// );
    ///
    /// // Buffer rows locally
    /// inserter.write(event1).await;
    /// inserter.write(event2).await;
    ///
    /// // Send to server
    /// inserter.flush().await?;
    /// ```
    fn async_inserter<T, R>(self, config: AsyncInsertConfig) -> AsyncInserter<T, R>
    where
        T: Table;

    /// Create an async inserter with pre-allocated buffer capacity.
    fn async_inserter_with_capacity<T, R>(
        self,
        config: AsyncInsertConfig,
        capacity: usize,
    ) -> AsyncInserter<T, R>
    where
        T: Table;
}

impl AsyncInsertExt for Connection {
    fn async_inserter<T, R>(self, config: AsyncInsertConfig) -> AsyncInserter<T, R>
    where
        T: Table,
    {
        AsyncInserter::new(self, config)
    }

    fn async_inserter_with_capacity<T, R>(
        self,
        config: AsyncInsertConfig,
        capacity: usize,
    ) -> AsyncInserter<T, R>
    where
        T: Table,
    {
        AsyncInserter::with_capacity(self, config, capacity)
    }
}
