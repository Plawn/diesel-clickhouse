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
//! - **Native**: Uses native Block format with session-level async insert settings via SET commands
//!
//! # Type Requirements
//!
//! For HTTP backend, row types must:
//! - Derive `#[derive(clickhouse::Row, serde::Serialize)]`
//! - Have `Value<'a> = Self` (typically primitive-only fields)
//!
//! For Native backend, row types must implement `ToNativeBlock`.
//!
//! # Usage Modes
//!
//! ## Fire-and-Forget (Highest Throughput)
//!
//! ```rust,ignore
//! use diesel_clickhouse::async_insert::{AsyncInsertConfig, AsyncInsertExt};
//!
//! let config = AsyncInsertConfig::fire_and_forget();
//! let inserter = conn.async_inserter::<events::table, NewEvent>(config);
//!
//! // Insert returns immediately - data is buffered server-side
//! inserter.insert(&event).await?;
//! ```
//!
//! ## Synchronous (Highest Durability)
//!
//! ```rust,ignore
//! let config = AsyncInsertConfig::synchronous();
//! let inserter = conn.async_inserter::<events::table, NewEvent>(config);
//!
//! // Waits for server confirmation before returning
//! inserter.insert(&event).await?;
//! ```
//!
//! # ClickHouse Settings
//!
//! | Setting | Description | Default |
//! |---------|-------------|---------|
//! | `async_insert` | Enable async insert mode | 1 |
//! | `wait_for_async_insert` | Wait for flush before returning | 0 or 1 |
//! | `async_insert_busy_timeout_ms` | Max wait before flush | 200ms |
//! | `async_insert_max_data_size` | Max bytes before flush | 10MB |
//! | `async_insert_max_query_number` | Max queries before flush | 450 |

use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::query_builder::Insertable;
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
    wait: bool,
    /// Timeout in milliseconds before forcing a flush (default: 200)
    busy_timeout_ms: Option<u64>,
    /// Maximum data size in bytes before flush (default: 10MB)
    max_data_size: Option<u64>,
    /// Maximum number of queries before flush (default: 450)
    max_query_number: Option<u64>,
    /// Whether to deduplicate across materialized views (for ReplicatedMergeTree)
    deduplicate_materialized_views: Option<bool>,
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
            max_data_size: None,
            max_query_number: None,
            deduplicate_materialized_views: None,
        }
    }

    /// Create a synchronous configuration (highest durability).
    ///
    /// The INSERT waits for the server to flush data to disk before returning.
    pub fn synchronous() -> Self {
        Self {
            wait: true,
            busy_timeout_ms: None,
            max_data_size: None,
            max_query_number: None,
            deduplicate_materialized_views: None,
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

    /// Check if this config waits for completion.
    pub fn is_synchronous(&self) -> bool {
        self.wait
    }

    /// Generate SET commands for Native backend.
    #[cfg(feature = "native")]
    pub(crate) fn to_set_commands(&self) -> Vec<String> {
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
// HTTP Backend - Optimized AsyncInserter using RowBinary format
// =============================================================================

/// HTTP-optimized async inserter using RowBinary format.
///
/// This inserter uses the `clickhouse` crate's `Insert` API which provides:
/// - RowBinary format (most efficient for HTTP)
/// - Streaming writes
/// - Async insert options via `.with_option()`
///
/// # Type Requirements
///
/// The row type `R` must:
/// - Derive `#[derive(clickhouse::Row, serde::Serialize)]`
/// - Have `Value<'a> = R` (typically means primitive-only fields, or use borrowed types)
///
/// For types with owned fields like `String`, you need to ensure the clickhouse::Row
/// derive generates compatible Value types.
#[cfg(feature = "http")]
pub struct HttpAsyncInserter<T, R> {
    conn: crate::http::ClickHouseConnection,
    config: AsyncInsertConfig,
    insert_count: AtomicU64,
    _marker: PhantomData<(T, R)>,
}

#[cfg(feature = "http")]
impl<T, R> HttpAsyncInserter<T, R>
where
    T: Table,
    R: Insertable<T> + clickhouse::Row + serde::Serialize + Send + Sync,
{
    /// Create a new HTTP async inserter.
    pub fn new(conn: crate::http::ClickHouseConnection, config: AsyncInsertConfig) -> Self {
        Self {
            conn,
            config,
            insert_count: AtomicU64::new(0),
            _marker: PhantomData,
        }
    }

    /// Get the number of insert operations performed.
    pub fn insert_count(&self) -> u64 {
        self.insert_count.load(Ordering::Relaxed)
    }

    /// Get the configuration.
    pub fn config(&self) -> &AsyncInsertConfig {
        &self.config
    }

    /// Insert a single row using RowBinary format.
    pub async fn insert(&self, row: &R) -> QueryResult<()>
    where
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        self.insert_many(std::slice::from_ref(row)).await
    }

    /// Insert multiple rows using RowBinary format.
    ///
    /// Uses the clickhouse crate's binary serialization for maximum efficiency.
    pub async fn insert_many(&self, rows: &[R]) -> QueryResult<()>
    where
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        if rows.is_empty() {
            return Ok(());
        }

        // Create inserter with async insert options
        let mut insert = self
            .conn
            .client()
            .insert::<R>(T::table_name())
            .await
            .map_err(crate::core::result::Error::query_from)?;

        // Apply async insert settings
        insert = insert
            .with_option("async_insert", "1")
            .with_option(
                "wait_for_async_insert",
                if self.config.wait { "1" } else { "0" },
            );

        if let Some(ms) = self.config.busy_timeout_ms {
            insert = insert.with_option("async_insert_busy_timeout_ms", ms.to_string());
        }
        if let Some(size) = self.config.max_data_size {
            insert = insert.with_option("async_insert_max_data_size", size.to_string());
        }
        if let Some(count) = self.config.max_query_number {
            insert = insert.with_option("async_insert_max_query_number", count.to_string());
        }
        if let Some(dedup) = self.config.deduplicate_materialized_views {
            insert = insert.with_option(
                "async_insert_deduplicate",
                if dedup { "1" } else { "0" },
            );
        }

        // Write rows using RowBinary format
        for row in rows {
            insert
                .write(row)
                .await
                .map_err(crate::core::result::Error::query_from)?;
        }

        insert
            .end()
            .await
            .map_err(crate::core::result::Error::query_from)?;

        self.insert_count
            .fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// Force the server to flush its async insert buffer.
    pub async fn flush(&self) -> QueryResult<()> {
        self.conn
            .execute_raw("SYSTEM FLUSH ASYNC INSERT QUEUE")
            .await
    }
}

// =============================================================================
// Native Backend - Optimized AsyncInserter using native Block format
// =============================================================================

/// Native-optimized async inserter using native Block format.
///
/// This inserter uses the `clickhouse-rs` crate's Block API which provides:
/// - Native binary Block format (most efficient for Native protocol)
/// - Columnar data layout
/// - Session-level async insert settings via SET commands
#[cfg(feature = "native")]
pub struct NativeAsyncInserter<T, R> {
    conn: crate::native::NativeConnection,
    config: AsyncInsertConfig,
    insert_count: AtomicU64,
    settings_applied: std::sync::atomic::AtomicBool,
    _marker: PhantomData<(T, R)>,
}

#[cfg(feature = "native")]
impl<T, R> NativeAsyncInserter<T, R>
where
    T: Table,
    R: Insertable<T> + crate::native::ToNativeBlock + Send,
{
    /// Create a new Native async inserter.
    pub fn new(conn: crate::native::NativeConnection, config: AsyncInsertConfig) -> Self {
        Self {
            conn,
            config,
            insert_count: AtomicU64::new(0),
            settings_applied: std::sync::atomic::AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    /// Get the number of insert operations performed.
    pub fn insert_count(&self) -> u64 {
        self.insert_count.load(Ordering::Relaxed)
    }

    /// Get the configuration.
    pub fn config(&self) -> &AsyncInsertConfig {
        &self.config
    }

    /// Apply async insert settings to the session (done once per inserter).
    async fn ensure_settings(&self) -> QueryResult<()> {
        if !self.settings_applied.swap(true, Ordering::SeqCst) {
            for cmd in self.config.to_set_commands() {
                self.conn.execute_raw(&cmd).await?;
            }
        }
        Ok(())
    }

    /// Insert a single row using optimized native Block format.
    pub async fn insert(&self, row: &R) -> QueryResult<()> {
        self.insert_many(std::slice::from_ref(row)).await
    }

    /// Insert multiple rows using optimized native Block format.
    pub async fn insert_many(&self, rows: &[R]) -> QueryResult<()> {
        if rows.is_empty() {
            return Ok(());
        }

        self.ensure_settings().await?;

        let block = R::rows_to_block(rows)?;
        self.conn.insert(T::table_name(), block).await?;

        self.insert_count
            .fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// Force the server to flush its async insert buffer.
    pub async fn flush(&self) -> QueryResult<()> {
        self.conn
            .execute_raw("SYSTEM FLUSH ASYNC INSERT QUEUE")
            .await
    }
}

// =============================================================================
// Unified AsyncInserter - Works with Connection enum
// =============================================================================

/// Unified async inserter that works with both HTTP and Native backends.
///
/// This struct automatically uses the optimal format for each backend:
/// - HTTP: SQL VALUES format with async insert settings
/// - Native: Native Block format via `clickhouse-rs::Block`
pub struct AsyncInserter<T, R> {
    conn: Connection,
    config: AsyncInsertConfig,
    insert_count: AtomicU64,
    _marker: PhantomData<(T, R)>,
}

impl<T, R> AsyncInserter<T, R>
where
    T: Table,
    R: Insertable<T>,
{
    /// Create a new async inserter from a unified Connection.
    pub fn new(conn: Connection, config: AsyncInsertConfig) -> Self {
        Self {
            conn,
            config,
            insert_count: AtomicU64::new(0),
            _marker: PhantomData,
        }
    }

    /// Get the number of insert operations performed.
    pub fn insert_count(&self) -> u64 {
        self.insert_count.load(Ordering::Relaxed)
    }

    /// Get the configuration.
    pub fn config(&self) -> &AsyncInsertConfig {
        &self.config
    }

    /// Insert a single row.
    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn insert(&self, row: &R) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + crate::native::ToNativeBlock + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        self.insert_many(std::slice::from_ref(row)).await
    }

    /// Insert a single row (http-only build).
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn insert(&self, row: &R) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        self.insert_many(std::slice::from_ref(row)).await
    }

    /// Insert a single row (native-only build).
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn insert(&self, row: &R) -> QueryResult<()>
    where
        R: crate::native::ToNativeBlock + Send,
    {
        self.insert_many(std::slice::from_ref(row)).await
    }

    /// Insert multiple rows.
    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn insert_many(&self, rows: &[R]) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + crate::native::ToNativeBlock + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        if rows.is_empty() {
            return Ok(());
        }

        match &self.conn {
            Connection::Http(_) => self.insert_many_http(rows).await,
            Connection::Native(_) => self.insert_many_native(rows).await,
        }
    }

    /// Insert multiple rows (http-only build).
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn insert_many(&self, rows: &[R]) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        if rows.is_empty() {
            return Ok(());
        }
        self.insert_many_http(rows).await
    }

    /// Insert multiple rows (native-only build).
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn insert_many(&self, rows: &[R]) -> QueryResult<()>
    where
        R: crate::native::ToNativeBlock + Send,
    {
        if rows.is_empty() {
            return Ok(());
        }
        self.insert_many_native(rows).await
    }

    /// HTTP implementation of insert_many using RowBinary format.
    #[cfg(feature = "http")]
    async fn insert_many_http(&self, rows: &[R]) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        // Get HTTP connection
        let Connection::Http(http_conn) = &self.conn else {
            return Err(crate::core::result::Error::query_static(
                "Expected HTTP connection",
            ));
        };

        // Create inserter with async insert options
        let mut insert = http_conn
            .client()
            .insert::<R>(T::table_name())
            .await
            .map_err(crate::core::result::Error::query_from)?;

        // Apply async insert settings
        insert = insert
            .with_option("async_insert", "1")
            .with_option(
                "wait_for_async_insert",
                if self.config.wait { "1" } else { "0" },
            );

        if let Some(ms) = self.config.busy_timeout_ms {
            insert = insert.with_option("async_insert_busy_timeout_ms", ms.to_string());
        }
        if let Some(size) = self.config.max_data_size {
            insert = insert.with_option("async_insert_max_data_size", size.to_string());
        }
        if let Some(count) = self.config.max_query_number {
            insert = insert.with_option("async_insert_max_query_number", count.to_string());
        }
        if let Some(dedup) = self.config.deduplicate_materialized_views {
            insert = insert.with_option(
                "async_insert_deduplicate",
                if dedup { "1" } else { "0" },
            );
        }

        // Write rows using RowBinary format
        for row in rows {
            insert
                .write(row)
                .await
                .map_err(crate::core::result::Error::query_from)?;
        }

        insert
            .end()
            .await
            .map_err(crate::core::result::Error::query_from)?;

        self.insert_count
            .fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// Native implementation of insert_many using Block format.
    #[cfg(feature = "native")]
    async fn insert_many_native(&self, rows: &[R]) -> QueryResult<()>
    where
        R: crate::native::ToNativeBlock,
    {
        // Apply async insert settings
        for cmd in self.config.to_set_commands() {
            self.conn.execute(&cmd).await?;
        }

        // Use native block format
        let block = R::rows_to_block(rows)?;

        // Get native connection
        let Connection::Native(native_conn) = &self.conn else {
            return Err(crate::core::result::Error::query_static(
                "Expected native connection",
            ));
        };

        native_conn.insert(T::table_name(), block).await?;
        self.insert_count
            .fetch_add(rows.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// Force the server to flush its async insert buffer.
    pub async fn flush(&self) -> QueryResult<()> {
        self.conn.execute("SYSTEM FLUSH ASYNC INSERT QUEUE").await
    }
}

// =============================================================================
// BufferedAsyncInserter - Local batching + async insert
// =============================================================================

/// A buffered async inserter with local batching.
///
/// Buffers rows locally before sending them to the server,
/// reducing network round-trips while still using async insert mode.
pub struct BufferedAsyncInserter<T, R> {
    inner: AsyncInserter<T, R>,
    buffer: Arc<Mutex<Vec<R>>>,
    buffer_size: usize,
}

impl<T, R> BufferedAsyncInserter<T, R>
where
    T: Table,
    R: Insertable<T> + Clone,
{
    /// Create a new buffered async inserter.
    pub fn new(conn: &Connection, config: AsyncInsertConfig, buffer_size: usize) -> Self {
        Self {
            inner: AsyncInserter::new(conn.clone(), config),
            buffer: Arc::new(Mutex::new(Vec::with_capacity(buffer_size))),
            buffer_size,
        }
    }

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

    /// Push a row to the local buffer.
    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn push(&self, row: R) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + crate::native::ToNativeBlock + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
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

    /// Push a row to the local buffer (http-only build).
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn push(&self, row: R) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
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

    /// Push a row to the local buffer (native-only build).
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn push(&self, row: R) -> QueryResult<()>
    where
        R: crate::native::ToNativeBlock + Send,
    {
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
    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn flush_buffer(&self) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + crate::native::ToNativeBlock + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        let rows = {
            let mut buffer = self.buffer.lock().await;
            std::mem::take(&mut *buffer)
        };

        if !rows.is_empty() {
            self.inner.insert_many(&rows).await?;
        }
        Ok(())
    }

    /// Flush the local buffer to the server (http-only build).
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn flush_buffer(&self) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        let rows = {
            let mut buffer = self.buffer.lock().await;
            std::mem::take(&mut *buffer)
        };

        if !rows.is_empty() {
            self.inner.insert_many(&rows).await?;
        }
        Ok(())
    }

    /// Flush the local buffer to the server (native-only build).
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn flush_buffer(&self) -> QueryResult<()>
    where
        R: crate::native::ToNativeBlock + Send,
    {
        let rows = {
            let mut buffer = self.buffer.lock().await;
            std::mem::take(&mut *buffer)
        };

        if !rows.is_empty() {
            self.inner.insert_many(&rows).await?;
        }
        Ok(())
    }

    /// Flush both local buffer and server async insert queue.
    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn flush_all(&self) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + crate::native::ToNativeBlock + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        self.flush_buffer().await?;
        self.inner.flush().await
    }

    /// Flush both local buffer and server async insert queue (http-only build).
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn flush_all(&self) -> QueryResult<()>
    where
        R: clickhouse::Row + serde::Serialize + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
    {
        self.flush_buffer().await?;
        self.inner.flush().await
    }

    /// Flush both local buffer and server async insert queue (native-only build).
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn flush_all(&self) -> QueryResult<()>
    where
        R: crate::native::ToNativeBlock + Send,
    {
        self.flush_buffer().await?;
        self.inner.flush().await
    }
}

// =============================================================================
// AsyncInsertExt - Extension trait for Connection
// =============================================================================

/// Extension trait for async insert operations on connections.
#[allow(async_fn_in_trait)]
pub trait AsyncInsertExt {
    /// Create an async inserter for a table.
    fn async_inserter<T, R>(&self, config: AsyncInsertConfig) -> AsyncInserter<T, R>
    where
        T: Table,
        R: Insertable<T>;

    /// Execute a raw SQL INSERT with async insert settings.
    async fn execute_async_insert(&self, sql: &str, config: AsyncInsertConfig) -> QueryResult<()>;
}

impl AsyncInsertExt for Connection {
    fn async_inserter<T, R>(&self, config: AsyncInsertConfig) -> AsyncInserter<T, R>
    where
        T: Table,
        R: Insertable<T>,
    {
        AsyncInserter::new(self.clone(), config)
    }

    async fn execute_async_insert(&self, sql: &str, config: AsyncInsertConfig) -> QueryResult<()> {
        let settings = config.to_settings_sql();
        let sql_upper = sql.to_uppercase();
        let insert_sql = if let Some(values_pos) = sql_upper.find("VALUES") {
            format!("{} {} {}", &sql[..values_pos], settings, &sql[values_pos..])
        } else if let Some(select_pos) = sql_upper.find("SELECT") {
            format!("{} {} {}", &sql[..select_pos], settings, &sql[select_pos..])
        } else {
            format!("{} {}", sql, settings)
        };

        self.execute(&insert_sql).await
    }
}
