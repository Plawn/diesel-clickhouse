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
#[cfg(feature = "native")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use parking_lot::Mutex;

use crate::core::query_source::Table;
use crate::core::result::QueryResult;
use crate::Connection;

/// Number of shards for the async inserter buffer.
/// Using 8 shards provides good parallelism while minimizing memory overhead.
/// This reduces lock contention when multiple tasks write concurrently.
const SHARD_COUNT: usize = 8;

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
        use std::fmt::Write;

        // Pre-allocate for typical settings length
        let mut result = String::with_capacity(150);

        write!(
            result,
            "SETTINGS async_insert=1, wait_for_async_insert={}",
            if self.wait { 1 } else { 0 }
        ).ok();

        if let Some(ms) = self.busy_timeout_ms {
            write!(result, ", async_insert_busy_timeout_ms={}", ms).ok();
        }
        if let Some(size) = self.max_data_size {
            write!(result, ", async_insert_max_data_size={}", size).ok();
        }
        if let Some(count) = self.max_query_number {
            write!(result, ", async_insert_max_query_number={}", count).ok();
        }
        if let Some(dedup) = self.deduplicate_materialized_views {
            write!(
                result,
                ", async_insert_deduplicate={}",
                if dedup { 1 } else { 0 }
            ).ok();
        }
        result
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
            .with_setting("async_insert", "1")
            .with_setting("wait_for_async_insert", if self.wait { "1" } else { "0" });

        if let Some(ms) = self.busy_timeout_ms {
            insert = insert.with_setting("async_insert_busy_timeout_ms", ms.to_string());
        }
        if let Some(size) = self.max_data_size {
            insert = insert.with_setting("async_insert_max_data_size", size.to_string());
        }
        if let Some(count) = self.max_query_number {
            insert = insert.with_setting("async_insert_max_query_number", count.to_string());
        }
        if let Some(dedup) = self.deduplicate_materialized_views {
            insert = insert.with_setting("async_insert_deduplicate", if dedup { "1" } else { "0" });
        }
        insert
    }
}

// Native-specific configuration methods
#[cfg(feature = "native")]
impl AsyncInsertConfig {
    /// Generate a single combined SET command for session-level async insert settings.
    ///
    /// This batches all settings into one command to reduce network round-trips.
    pub fn to_native_set_command(&self) -> String {
        use std::fmt::Write;

        // Pre-allocate for typical command length
        let mut result = String::with_capacity(200);

        write!(
            result,
            "SET async_insert = 1, wait_for_async_insert = {}",
            if self.wait { 1 } else { 0 }
        ).ok();

        if let Some(ms) = self.busy_timeout_ms {
            write!(result, ", async_insert_busy_timeout_ms = {}", ms).ok();
        }
        if let Some(size) = self.max_data_size {
            write!(result, ", async_insert_max_data_size = {}", size).ok();
        }
        if let Some(count) = self.max_query_number {
            write!(result, ", async_insert_max_query_number = {}", count).ok();
        }
        if let Some(dedup) = self.deduplicate_materialized_views {
            write!(
                result,
                ", async_insert_deduplicate = {}",
                if dedup { 1 } else { 0 }
            ).ok();
        }
        result
    }
}

// =============================================================================
// AsyncInsertable Trait - Backend abstraction for batch inserts
// =============================================================================

/// Trait for types that can be batch-inserted using async insert mode.
///
/// This trait abstracts over the backend-specific insert logic, allowing
/// a single `AsyncInserter` implementation to work with both HTTP and Native backends.
///
/// # Implementors
///
/// This trait is automatically implemented for row types based on enabled features:
/// - HTTP: Types implementing `clickhouse::Row + Serialize`
/// - Native: Types implementing `ToNativeBlock`
/// - Both: Types implementing all of the above
#[allow(async_fn_in_trait)]
pub trait AsyncInsertable<T: Table>: Sized + Send {
    /// Insert a batch of rows using the appropriate backend mechanism.
    ///
    /// # Arguments
    ///
    /// * `conn` - The connection to use for insertion
    /// * `config` - Async insert configuration
    /// * `rows` - The rows to insert
    /// * `settings_applied` - Atomic flag for native backend settings (ignored by HTTP)
    async fn batch_insert(
        conn: &Connection,
        config: &AsyncInsertConfig,
        rows: &[Self],
        #[cfg(feature = "native")] settings_applied: &AtomicBool,
    ) -> QueryResult<()>;
}

// HTTP + Native implementation
#[cfg(all(feature = "http", feature = "native"))]
impl<T, R> AsyncInsertable<T> for R
where
    T: Table,
    R: clickhouse::Row + serde::Serialize + crate::native::ToNativeBlock + Send + Sync,
    for<'a> R: clickhouse::Row<Value<'a> = R>,
    for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
{
    async fn batch_insert(
        conn: &Connection,
        config: &AsyncInsertConfig,
        rows: &[Self],
        settings_applied: &AtomicBool,
    ) -> QueryResult<()> {
        match conn {
            Connection::Http(http_conn) => {
                http_conn.async_insert_rows::<T, R>(config, rows).await
            }
            Connection::Native(native_conn) => {
                // Apply settings once per inserter (single batched command)
                if !settings_applied.swap(true, Ordering::SeqCst) {
                    native_conn.execute_raw(&config.to_native_set_command()).await?;
                }
                let block = R::rows_to_block(rows)?;
                native_conn.insert(T::table_name(), block).await
            }
        }
    }
}

// HTTP-only implementation
#[cfg(all(feature = "http", not(feature = "native")))]
impl<T, R> AsyncInsertable<T> for R
where
    T: Table,
    R: clickhouse::Row + serde::Serialize + Send + Sync,
    for<'a> R: clickhouse::Row<Value<'a> = R>,
    for<'a> <R as clickhouse::Row>::Value<'a>: serde::Serialize,
{
    async fn batch_insert(
        conn: &Connection,
        config: &AsyncInsertConfig,
        rows: &[Self],
    ) -> QueryResult<()> {
        let Connection::Http(http_conn) = conn;
        http_conn.async_insert_rows::<T, R>(config, rows).await
    }
}

// Native-only implementation
#[cfg(all(feature = "native", not(feature = "http")))]
impl<T, R> AsyncInsertable<T> for R
where
    T: Table,
    R: crate::native::ToNativeBlock + Send,
{
    async fn batch_insert(
        conn: &Connection,
        config: &AsyncInsertConfig,
        rows: &[Self],
        settings_applied: &AtomicBool,
    ) -> QueryResult<()> {
        let Connection::Native(native_conn) = conn;

        // Apply settings once per inserter (single batched command)
        if !settings_applied.swap(true, Ordering::SeqCst) {
            native_conn.execute_raw(&config.to_native_set_command()).await?;
        }

        let block = R::rows_to_block(rows)?;
        native_conn.insert(T::table_name(), block).await
    }
}

// =============================================================================
// AsyncInserter
// =============================================================================

/// Buffered async inserter with local batching and sharded locking.
///
/// Rows are buffered locally across multiple shards to reduce lock contention
/// when multiple tasks write concurrently. Data is only sent to the server when
/// `flush()` is called.
///
/// # Performance
///
/// The sharded buffer design reduces lock contention under high concurrency by
/// distributing writes across 8 independent locks. Each write acquires only
/// one shard's lock, allowing multiple tasks to write in parallel.
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
/// // Write rows (buffered locally, low contention)
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
    /// Sharded buffers for reduced lock contention under high concurrency.
    /// Each shard is independently locked, allowing parallel writes.
    shards: Box<[Mutex<Vec<R>>; SHARD_COUNT]>,
    /// Atomic counters for lock-free buffered_count().
    /// Updated on write/write_many and reset on flush.
    shard_counts: Box<[AtomicUsize; SHARD_COUNT]>,
    /// Round-robin counter for shard selection.
    shard_counter: AtomicU64,
    sent_count: AtomicU64,
    #[cfg(feature = "native")]
    settings_applied: AtomicBool,
    _marker: PhantomData<T>,
}

/// Helper to create initialized shard array
fn create_shards<R>() -> Box<[Mutex<Vec<R>>; SHARD_COUNT]> {
    Box::new(std::array::from_fn(|_| Mutex::new(Vec::new())))
}

/// Helper to create initialized shard array with capacity per shard
fn create_shards_with_capacity<R>(capacity_per_shard: usize) -> Box<[Mutex<Vec<R>>; SHARD_COUNT]> {
    Box::new(std::array::from_fn(|_| Mutex::new(Vec::with_capacity(capacity_per_shard))))
}

/// Helper to create initialized atomic shard counters
fn create_shard_counts() -> Box<[AtomicUsize; SHARD_COUNT]> {
    Box::new(std::array::from_fn(|_| AtomicUsize::new(0)))
}

impl<T, R> AsyncInserter<T, R>
where
    T: Table,
{
    /// Create a new async inserter with sharded buffering.
    pub fn new(conn: Connection, config: AsyncInsertConfig) -> Self {
        Self {
            conn,
            config,
            shards: create_shards(),
            shard_counts: create_shard_counts(),
            shard_counter: AtomicU64::new(0),
            sent_count: AtomicU64::new(0),
            #[cfg(feature = "native")]
            settings_applied: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    /// Create a new async inserter with pre-allocated buffer capacity.
    ///
    /// The capacity is distributed across all shards.
    pub fn with_capacity(conn: Connection, config: AsyncInsertConfig, capacity: usize) -> Self {
        let capacity_per_shard = capacity.div_ceil(SHARD_COUNT);
        Self {
            conn,
            config,
            shards: create_shards_with_capacity(capacity_per_shard),
            shard_counts: create_shard_counts(),
            shard_counter: AtomicU64::new(0),
            sent_count: AtomicU64::new(0),
            #[cfg(feature = "native")]
            settings_applied: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    /// Select a shard using round-robin distribution.
    #[inline]
    fn select_shard(&self) -> usize {
        (self.shard_counter.fetch_add(1, Ordering::Relaxed) as usize) % SHARD_COUNT
    }

    /// Get the number of rows currently buffered locally across all shards.
    ///
    /// This is lock-free and uses atomic counters for each shard.
    #[inline]
    pub fn buffered_count(&self) -> usize {
        self.shard_counts.iter().map(|c| c.load(Ordering::Relaxed)).sum()
    }

    /// Get the total number of rows sent to the server.
    #[inline]
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

// Unified implementation using AsyncInsertable trait
impl<T, R> AsyncInserter<T, R>
where
    T: Table,
    R: AsyncInsertable<T>,
{
    /// Write a single row to the local buffer.
    ///
    /// Uses sharded locking to reduce contention when multiple tasks write concurrently.
    /// The row is not sent to the server until `flush()` is called.
    pub fn write(&self, row: R) {
        let shard = self.select_shard();
        self.shards[shard].lock().push(row);
        self.shard_counts[shard].fetch_add(1, Ordering::Relaxed);
    }

    /// Write multiple rows to the local buffer.
    ///
    /// Uses sharded locking to reduce contention. All rows go to the same shard
    /// for efficiency (avoiding multiple lock acquisitions).
    /// The rows are not sent to the server until `flush()` is called.
    pub fn write_many(&self, rows: impl IntoIterator<Item = R>) {
        let shard = self.select_shard();
        let mut guard = self.shards[shard].lock();
        let prev_len = guard.len();
        guard.extend(rows);
        let added = guard.len() - prev_len;
        drop(guard);
        self.shard_counts[shard].fetch_add(added, Ordering::Relaxed);
    }

    /// Flush the local buffer to the server.
    ///
    /// Collects all buffered rows from all shards and sends them using the
    /// configured async insert settings. After this call, all shard buffers are empty.
    pub async fn flush(&self) -> QueryResult<()> {
        // Use atomic counters for total_len (lock-free)
        let total_len: usize = self.shard_counts.iter().map(|c| c.load(Ordering::Relaxed)).sum();
        if total_len == 0 {
            return Ok(());
        }

        let mut rows = Vec::with_capacity(total_len);
        for (i, shard) in self.shards.iter().enumerate() {
            rows.append(&mut *shard.lock());
            // Reset the atomic counter for this shard
            self.shard_counts[i].store(0, Ordering::Relaxed);
        }

        R::batch_insert(
            &self.conn,
            &self.config,
            &rows,
            #[cfg(feature = "native")]
            &self.settings_applied,
        )
        .await?;

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
