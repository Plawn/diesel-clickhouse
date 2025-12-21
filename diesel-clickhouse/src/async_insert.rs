//! Async insert mode support for ClickHouse.
//!
//! ClickHouse's async insert mode buffers inserts on the server side,
//! improving throughput for high-frequency small inserts.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::async_insert::{AsyncInserter, AsyncInsertConfig};
//!
//! let config = AsyncInsertConfig::default()
//!     .wait_for_async_insert(true)
//!     .async_insert_busy_timeout_ms(200);
//!
//! let inserter = AsyncInserter::new(&conn, "events", config);
//!
//! // Inserts are buffered on the server
//! inserter.insert(&event1).await?;
//! inserter.insert(&event2).await?;
//!
//! // Optionally wait for flush
//! inserter.flush().await?;
//! ```

use std::marker::PhantomData;

use crate::core::backend::{ClickHouse, GenericQueryBuilder, GenericBindCollector, QueryBuilder};
use crate::core::query_builder::{AstPass, Insertable};
use crate::core::query_source::Table;
use crate::core::result::QueryResult;
use crate::Connection;

/// Configuration for async insert mode.
#[derive(Debug, Clone)]
pub struct AsyncInsertConfig {
    /// Enable async insert mode (default: true).
    pub async_insert: bool,
    /// Wait for async insert to complete before returning.
    pub wait_for_async_insert: bool,
    /// Timeout in ms for busy wait (default: 200ms).
    pub async_insert_busy_timeout_ms: u64,
    /// Maximum data size in bytes before flush (default: 10MB).
    pub async_insert_max_data_size: u64,
    /// Maximum query count before flush (default: 450).
    pub async_insert_max_query_number: u64,
    /// Deduplicate async inserts (for ReplicatedMergeTree).
    pub deduplicate_blocks_in_dependent_materialized_views: bool,
}

impl Default for AsyncInsertConfig {
    fn default() -> Self {
        Self {
            async_insert: true,
            wait_for_async_insert: false,
            async_insert_busy_timeout_ms: 200,
            async_insert_max_data_size: 10_000_000, // 10MB
            async_insert_max_query_number: 450,
            deduplicate_blocks_in_dependent_materialized_views: false,
        }
    }
}

impl AsyncInsertConfig {
    /// Create a new config with async insert enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create config for synchronous behavior (wait for insert).
    pub fn synchronous() -> Self {
        Self {
            async_insert: true,
            wait_for_async_insert: true,
            ..Default::default()
        }
    }

    /// Create config for fire-and-forget behavior.
    pub fn fire_and_forget() -> Self {
        Self {
            async_insert: true,
            wait_for_async_insert: false,
            ..Default::default()
        }
    }

    /// Set whether to wait for async insert.
    pub fn wait_for_async_insert(mut self, wait: bool) -> Self {
        self.wait_for_async_insert = wait;
        self
    }

    /// Set the busy timeout in milliseconds.
    pub fn async_insert_busy_timeout_ms(mut self, timeout: u64) -> Self {
        self.async_insert_busy_timeout_ms = timeout;
        self
    }

    /// Set the maximum data size before flush.
    pub fn async_insert_max_data_size(mut self, size: u64) -> Self {
        self.async_insert_max_data_size = size;
        self
    }

    /// Set the maximum query count before flush.
    pub fn async_insert_max_query_number(mut self, count: u64) -> Self {
        self.async_insert_max_query_number = count;
        self
    }

    /// Enable deduplication for materialized views.
    pub fn deduplicate_materialized_views(mut self, dedupe: bool) -> Self {
        self.deduplicate_blocks_in_dependent_materialized_views = dedupe;
        self
    }

    /// Build the SETTINGS clause for this config.
    pub fn to_settings_sql(&self) -> String {
        let mut settings = Vec::with_capacity(8);

        settings.push(format!("async_insert = {}", if self.async_insert { 1 } else { 0 }));
        settings.push(format!("wait_for_async_insert = {}", if self.wait_for_async_insert { 1 } else { 0 }));
        settings.push(format!("async_insert_busy_timeout_ms = {}", self.async_insert_busy_timeout_ms));
        settings.push(format!("async_insert_max_data_size = {}", self.async_insert_max_data_size));
        settings.push(format!("async_insert_max_query_number = {}", self.async_insert_max_query_number));

        if self.deduplicate_blocks_in_dependent_materialized_views {
            settings.push("deduplicate_blocks_in_dependent_materialized_views = 1".to_string());
        }

        format!("SETTINGS {}", settings.join(", "))
    }
}

/// An async inserter for high-throughput inserts.
///
/// Uses ClickHouse's async_insert mode to buffer inserts on the server,
/// reducing round-trip overhead for many small inserts.
pub struct AsyncInserter<'a, T: Table, R: Insertable<T>> {
    conn: &'a Connection,
    table_name: &'static str,
    config: AsyncInsertConfig,
    insert_count: std::sync::atomic::AtomicU64,
    _marker: PhantomData<(T, R)>,
}

impl<'a, T: Table, R: Insertable<T>> AsyncInserter<'a, T, R> {
    /// Create a new async inserter.
    pub fn new(conn: &'a Connection, config: AsyncInsertConfig) -> Self {
        Self {
            conn,
            table_name: T::table_name(),
            config,
            insert_count: std::sync::atomic::AtomicU64::new(0),
            _marker: PhantomData,
        }
    }

    /// Create with default config.
    pub fn with_defaults(conn: &'a Connection) -> Self {
        Self::new(conn, AsyncInsertConfig::default())
    }

    /// Insert a single row asynchronously.
    pub async fn insert(&self, row: &R) -> QueryResult<()> {
        let sql = self.build_insert_sql(std::slice::from_ref(row));
        self.conn.execute(&sql).await?;
        self.insert_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Insert multiple rows asynchronously.
    pub async fn insert_many(&self, rows: &[R]) -> QueryResult<()> {
        if rows.is_empty() {
            return Ok(());
        }

        let sql = self.build_insert_sql(rows);
        self.conn.execute(&sql).await?;
        self.insert_count.fetch_add(rows.len() as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Flush pending inserts on the server.
    ///
    /// This executes a SYSTEM FLUSH ASYNC INSERT QUEUE command.
    pub async fn flush(&self) -> QueryResult<()> {
        self.conn.execute("SYSTEM FLUSH ASYNC INSERT QUEUE").await
    }

    /// Get the number of inserts performed.
    pub fn insert_count(&self) -> u64 {
        self.insert_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the config.
    pub fn config(&self) -> &AsyncInsertConfig {
        &self.config
    }

    /// Build INSERT SQL with async settings.
    fn build_insert_sql(&self, rows: &[R]) -> String {
        let columns = R::column_names();

        // Estimate capacity
        let capacity = 100 + columns.len() * 20 + rows.len() * 50;
        let mut sql = String::with_capacity(capacity);

        sql.push_str("INSERT INTO ");
        sql.push_str(self.table_name);

        if !columns.is_empty() {
            sql.push_str(" (");
            for (i, col) in columns.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push('`');
                sql.push_str(col);
                sql.push('`');
            }
            sql.push(')');
        }

        sql.push_str(" ");
        sql.push_str(&self.config.to_settings_sql());
        sql.push_str(" VALUES ");

        // Build values
        let mut builder = GenericQueryBuilder::default();
        let mut collector = GenericBindCollector::default();

        for (i, row) in rows.iter().enumerate() {
            if i > 0 {
                builder.push_sql(", ");
            }
            builder.push_sql("(");
            {
                let mut pass = AstPass::<ClickHouse>::new(&mut builder, &mut collector);
                let _ = row.write_value(&mut pass);
            }
            builder.push_sql(")");
        }

        sql.push_str(&builder.finish());
        sql
    }
}

/// Extension trait for Connection to create async inserters.
pub trait AsyncInsertExt {
    /// Create an async inserter for a table.
    fn async_inserter<T: Table, R: Insertable<T>>(
        &self,
        config: AsyncInsertConfig,
    ) -> AsyncInserter<'_, T, R>;

    /// Execute a query with async insert settings.
    fn execute_async_insert(
        &self,
        sql: &str,
        config: &AsyncInsertConfig,
    ) -> impl std::future::Future<Output = QueryResult<()>> + Send;
}

impl AsyncInsertExt for Connection {
    fn async_inserter<T: Table, R: Insertable<T>>(
        &self,
        config: AsyncInsertConfig,
    ) -> AsyncInserter<'_, T, R> {
        AsyncInserter::new(self, config)
    }

    async fn execute_async_insert(
        &self,
        sql: &str,
        config: &AsyncInsertConfig,
    ) -> QueryResult<()> {
        let full_sql = format!("{} {}", sql, config.to_settings_sql());
        self.execute(&full_sql).await
    }
}

/// A buffered async inserter that batches inserts locally before sending.
///
/// Combines local batching with server-side async insert for maximum throughput.
pub struct BufferedAsyncInserter<'a, T: Table, R: Insertable<T> + Clone> {
    inner: AsyncInserter<'a, T, R>,
    buffer: std::sync::Mutex<Vec<R>>,
    buffer_size: usize,
}

impl<'a, T: Table, R: Insertable<T> + Clone> BufferedAsyncInserter<'a, T, R> {
    /// Create a new buffered async inserter.
    pub fn new(conn: &'a Connection, config: AsyncInsertConfig, buffer_size: usize) -> Self {
        Self {
            inner: AsyncInserter::new(conn, config),
            buffer: std::sync::Mutex::new(Vec::with_capacity(buffer_size)),
            buffer_size,
        }
    }

    /// Push a row to the buffer.
    ///
    /// Automatically flushes when the buffer is full.
    ///
    /// # Panics
    ///
    /// Panics if the internal Mutex is poisoned.
    pub async fn push(&self, row: R) -> QueryResult<()> {
        let should_flush = {
            let mut buffer = self.buffer.lock()
                .expect("BufferedAsyncInserter Mutex poisoned");
            buffer.push(row);
            buffer.len() >= self.buffer_size
        };

        if should_flush {
            self.flush_buffer().await?;
        }

        Ok(())
    }

    /// Flush the local buffer to the server.
    ///
    /// # Panics
    ///
    /// Panics if the internal Mutex is poisoned.
    pub async fn flush_buffer(&self) -> QueryResult<()> {
        let rows: Vec<R> = {
            let mut buffer = self.buffer.lock()
                .expect("BufferedAsyncInserter Mutex poisoned");
            std::mem::take(&mut *buffer)
        };

        if !rows.is_empty() {
            self.inner.insert_many(&rows).await?;
        }

        Ok(())
    }

    /// Flush both local buffer and server queue.
    pub async fn flush_all(&self) -> QueryResult<()> {
        self.flush_buffer().await?;
        self.inner.flush().await
    }

    /// Get the number of rows currently buffered locally.
    ///
    /// # Panics
    ///
    /// Panics if the internal Mutex is poisoned.
    pub fn buffered_count(&self) -> usize {
        self.buffer.lock()
            .expect("BufferedAsyncInserter Mutex poisoned")
            .len()
    }

    /// Get total insert count.
    pub fn insert_count(&self) -> u64 {
        self.inner.insert_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_insert_config() {
        let config = AsyncInsertConfig::default();
        assert!(config.async_insert);
        assert!(!config.wait_for_async_insert);
    }

    #[test]
    fn test_async_insert_config_builder() {
        let config = AsyncInsertConfig::new()
            .wait_for_async_insert(true)
            .async_insert_busy_timeout_ms(500);

        assert!(config.wait_for_async_insert);
        assert_eq!(config.async_insert_busy_timeout_ms, 500);
    }

    #[test]
    fn test_settings_sql() {
        let config = AsyncInsertConfig::synchronous();
        let sql = config.to_settings_sql();

        assert!(sql.contains("async_insert = 1"));
        assert!(sql.contains("wait_for_async_insert = 1"));
    }
}
