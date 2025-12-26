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
//! // Fire-and-forget mode (highest throughput)
//! let config = AsyncInsertConfig::fire_and_forget();
//! let inserter = conn.async_inserter::<events::table, NewEvent>(config);
//! inserter.insert(&event).await?;
//!
//! // Synchronous mode (highest durability)
//! let config = AsyncInsertConfig::synchronous();
//! let inserter = conn.async_inserter::<events::table, NewEvent>(config);
//! inserter.insert(&event).await?;
//! ```

use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

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

/// Async inserter that wraps a connection with async insert configuration.
///
/// This provides a convenient API for repeated async inserts with the same
/// configuration. For Native backend, it caches the settings application
/// to avoid re-sending SET commands on every insert.
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
/// // Insert multiple batches
/// inserter.insert_many(&batch1).await?;
/// inserter.insert_many(&batch2).await?;
///
/// println!("Inserted {} rows", inserter.insert_count());
/// ```
pub struct AsyncInserter<T, R> {
    conn: Connection,
    config: AsyncInsertConfig,
    insert_count: AtomicU64,
    #[cfg(feature = "native")]
    settings_applied: AtomicBool,
    _marker: PhantomData<(T, R)>,
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
            insert_count: AtomicU64::new(0),
            #[cfg(feature = "native")]
            settings_applied: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    /// Get the number of rows inserted.
    pub fn insert_count(&self) -> u64 {
        self.insert_count.load(Ordering::Relaxed)
    }

    /// Get the configuration.
    pub fn config(&self) -> &AsyncInsertConfig {
        &self.config
    }

    /// Force the server to flush its async insert buffer.
    pub async fn flush(&self) -> QueryResult<()> {
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
    /// Insert a single row.
    pub async fn insert(&self, row: &R) -> QueryResult<()> {
        self.insert_many(std::slice::from_ref(row)).await
    }

    /// Insert multiple rows.
    pub async fn insert_many(&self, rows: &[R]) -> QueryResult<()> {
        if rows.is_empty() {
            return Ok(());
        }

        match &self.conn {
            Connection::Http(conn) => {
                conn.async_insert_rows::<T, R>(&self.config, rows).await?;
            }
            Connection::Native(conn) => {
                // Apply settings once per inserter
                if !self.settings_applied.swap(true, Ordering::SeqCst) {
                    for cmd in self.config.to_native_set_commands() {
                        conn.execute_raw(&cmd).await?;
                    }
                }
                let block = R::rows_to_block(rows)?;
                conn.insert(T::table_name(), block).await?;
            }
        }

        self.insert_count.fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
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
    /// Insert a single row.
    pub async fn insert(&self, row: &R) -> QueryResult<()> {
        self.insert_many(std::slice::from_ref(row)).await
    }

    /// Insert multiple rows.
    pub async fn insert_many(&self, rows: &[R]) -> QueryResult<()> {
        if rows.is_empty() {
            return Ok(());
        }

        let Connection::Http(conn) = &self.conn;
        conn.async_insert_rows::<T, R>(&self.config, rows).await?;
        self.insert_count.fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }
}

// Native only
#[cfg(all(feature = "native", not(feature = "http")))]
impl<T, R> AsyncInserter<T, R>
where
    T: Table,
    R: crate::native::ToNativeBlock + Send,
{
    /// Insert a single row.
    pub async fn insert(&self, row: &R) -> QueryResult<()> {
        self.insert_many(std::slice::from_ref(row)).await
    }

    /// Insert multiple rows.
    pub async fn insert_many(&self, rows: &[R]) -> QueryResult<()> {
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

        let block = R::rows_to_block(rows)?;
        conn.insert(T::table_name(), block).await?;
        self.insert_count.fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }
}

// =============================================================================
// BufferedAsyncInserter
// =============================================================================

/// A buffered async inserter with local batching.
///
/// Buffers rows locally before sending them to the server,
/// reducing network round-trips while still using async insert mode.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::async_insert::{AsyncInsertConfig, BufferedAsyncInserter};
///
/// let inserter = BufferedAsyncInserter::<events::table, NewEvent>::new(
///     conn.clone(),
///     AsyncInsertConfig::fire_and_forget(),
///     1000, // buffer size
/// );
///
/// // Push rows one at a time - they're batched locally
/// for event in events {
///     inserter.push(event).await?;
/// }
///
/// // Flush remaining rows
/// inserter.flush_all().await?;
/// ```
pub struct BufferedAsyncInserter<T, R> {
    inner: AsyncInserter<T, R>,
    buffer: Arc<Mutex<Vec<R>>>,
    buffer_size: usize,
}

impl<T, R> BufferedAsyncInserter<T, R>
where
    T: Table,
    R: Clone,
{
    /// Get the number of rows currently in the local buffer.
    pub async fn buffered_count(&self) -> usize {
        self.buffer.lock().await.len()
    }

    /// Get the total number of rows sent to the server.
    pub fn insert_count(&self) -> u64 {
        self.inner.insert_count()
    }

    /// Get the configuration.
    pub fn config(&self) -> &AsyncInsertConfig {
        self.inner.config()
    }
}

// HTTP + Native
#[cfg(all(feature = "http", feature = "native"))]
impl<T, R> BufferedAsyncInserter<T, R>
where
    T: Table,
    R: clickhouse::Row + serde::Serialize + crate::native::ToNativeBlock + Clone + Send + Sync,
    for<'a> R: clickhouse::Row<Value<'a> = R>,
    for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
{
    /// Create a new buffered async inserter.
    pub fn new(conn: Connection, config: AsyncInsertConfig, buffer_size: usize) -> Self {
        Self {
            inner: AsyncInserter::new(conn, config),
            buffer: Arc::new(Mutex::new(Vec::with_capacity(buffer_size))),
            buffer_size,
        }
    }

    /// Push a row to the local buffer, flushing if buffer is full.
    pub async fn push(&self, row: R) -> QueryResult<()> {
        let should_flush = {
            let mut buffer = self.buffer.lock().await;
            buffer.push(row);
            buffer.len() >= self.buffer_size
        };
        if should_flush {
            self.flush_buffer().await?;
        }
        Ok(())
    }

    /// Flush the local buffer to the server.
    pub async fn flush_buffer(&self) -> QueryResult<()> {
        let rows = std::mem::take(&mut *self.buffer.lock().await);
        if !rows.is_empty() {
            self.inner.insert_many(&rows).await?;
        }
        Ok(())
    }

    /// Flush both local buffer and server async insert queue.
    pub async fn flush_all(&self) -> QueryResult<()> {
        self.flush_buffer().await?;
        self.inner.flush().await
    }
}

// HTTP only
#[cfg(all(feature = "http", not(feature = "native")))]
impl<T, R> BufferedAsyncInserter<T, R>
where
    T: Table,
    R: clickhouse::Row + serde::Serialize + Clone + Send + Sync,
    for<'a> R: clickhouse::Row<Value<'a> = R>,
    for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
{
    /// Create a new buffered async inserter.
    pub fn new(conn: Connection, config: AsyncInsertConfig, buffer_size: usize) -> Self {
        Self {
            inner: AsyncInserter::new(conn, config),
            buffer: Arc::new(Mutex::new(Vec::with_capacity(buffer_size))),
            buffer_size,
        }
    }

    /// Push a row to the local buffer, flushing if buffer is full.
    pub async fn push(&self, row: R) -> QueryResult<()> {
        let should_flush = {
            let mut buffer = self.buffer.lock().await;
            buffer.push(row);
            buffer.len() >= self.buffer_size
        };
        if should_flush {
            self.flush_buffer().await?;
        }
        Ok(())
    }

    /// Flush the local buffer to the server.
    pub async fn flush_buffer(&self) -> QueryResult<()> {
        let rows = std::mem::take(&mut *self.buffer.lock().await);
        if !rows.is_empty() {
            self.inner.insert_many(&rows).await?;
        }
        Ok(())
    }

    /// Flush both local buffer and server async insert queue.
    pub async fn flush_all(&self) -> QueryResult<()> {
        self.flush_buffer().await?;
        self.inner.flush().await
    }
}

// Native only
#[cfg(all(feature = "native", not(feature = "http")))]
impl<T, R> BufferedAsyncInserter<T, R>
where
    T: Table,
    R: crate::native::ToNativeBlock + Clone + Send,
{
    /// Create a new buffered async inserter.
    pub fn new(conn: Connection, config: AsyncInsertConfig, buffer_size: usize) -> Self {
        Self {
            inner: AsyncInserter::new(conn, config),
            buffer: Arc::new(Mutex::new(Vec::with_capacity(buffer_size))),
            buffer_size,
        }
    }

    /// Push a row to the local buffer, flushing if buffer is full.
    pub async fn push(&self, row: R) -> QueryResult<()> {
        let should_flush = {
            let mut buffer = self.buffer.lock().await;
            buffer.push(row);
            buffer.len() >= self.buffer_size
        };
        if should_flush {
            self.flush_buffer().await?;
        }
        Ok(())
    }

    /// Flush the local buffer to the server.
    pub async fn flush_buffer(&self) -> QueryResult<()> {
        let rows = std::mem::take(&mut *self.buffer.lock().await);
        if !rows.is_empty() {
            self.inner.insert_many(&rows).await?;
        }
        Ok(())
    }

    /// Flush both local buffer and server async insert queue.
    pub async fn flush_all(&self) -> QueryResult<()> {
        self.flush_buffer().await?;
        self.inner.flush().await
    }
}

// =============================================================================
// AsyncInsertExt - Extension trait for Connection
// =============================================================================

/// Extension trait for async insert operations on connections.
pub trait AsyncInsertExt {
    /// Create an async inserter for a table.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::async_insert::{AsyncInsertConfig, AsyncInsertExt};
    ///
    /// let inserter = conn.async_inserter::<events::table, NewEvent>(
    ///     AsyncInsertConfig::fire_and_forget()
    /// );
    /// inserter.insert(&event).await?;
    /// ```
    fn async_inserter<T, R>(self, config: AsyncInsertConfig) -> AsyncInserter<T, R>
    where
        T: Table;

    /// Create a buffered async inserter for a table.
    fn buffered_async_inserter<T, R>(
        self,
        config: AsyncInsertConfig,
        buffer_size: usize,
    ) -> BufferedAsyncInserter<T, R>
    where
        T: Table,
        R: Clone;
}

impl AsyncInsertExt for Connection {
    fn async_inserter<T, R>(self, config: AsyncInsertConfig) -> AsyncInserter<T, R>
    where
        T: Table,
    {
        AsyncInserter::new(self, config)
    }

    fn buffered_async_inserter<T, R>(
        self,
        config: AsyncInsertConfig,
        buffer_size: usize,
    ) -> BufferedAsyncInserter<T, R>
    where
        T: Table,
        R: Clone,
    {
        BufferedAsyncInserter {
            inner: AsyncInserter::new(self, config),
            buffer: Arc::new(Mutex::new(Vec::with_capacity(buffer_size))),
            buffer_size,
        }
    }
}
