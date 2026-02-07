//! Native protocol ClickHouse connection.
//!
//! This module provides a high-performance async connection to ClickHouse
//! using the native binary protocol (TCP port 9000 or 9440 for TLS).
//!
//! The native protocol is faster than HTTP because it uses a binary format
//! and maintains a persistent connection.
//!
//! # Features
//!
//! - `native` - Enable the native backend
//! - `native-tls-native` - Enable TLS support for native backend
//!
//! # Connection URL Format
//!
//! ```text
//! tcp://[user:password@]host[:port]/database[?options]
//! ```
//!
//! ## Options
//!
//! - `secure=true` - Enable TLS (requires `native-tls-native` feature)
//! - `skip_verify=true` - Skip TLS certificate verification (insecure)
//! - `compression=lz4` - Enable LZ4 compression
//! - `connection_timeout=500ms` - Connection timeout
//! - `ping_timeout=500ms` - Ping timeout
//! - `query_timeout=180s` - Query timeout
//! - `pool_min=5` - Minimum pool connections
//! - `pool_max=10` - Maximum pool connections
//!
//! # Usage
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//! use diesel_clickhouse::native::NativeConnection;
//!
//! #[derive(Debug, Row)]
//! struct User {
//!     id: u64,
//!     name: String,
//! }
//!
//! // Plain TCP connection (port 9000)
//! let conn = NativeConnection::establish("tcp://localhost:9000/default").await?;
//!
//! // Query using unified interface
//! let users: Vec<User> = conn.load(users::table.filter(users::active.eq(true))).await?;
//! ```

mod block;
mod builder;
mod column;

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use clickhouse_rs::{Pool, ClientHandle, Block, types::Complex};
use futures::StreamExt;

use crate::core::backend::ClickHouse;
use crate::core::connection::ClickHouseConnection as ClickHouseConnectionTrait;
use crate::core::query_builder::QueryFragment;

// AstPass is only needed for tests (implementing QueryFragment for test structs)
#[cfg(test)]
use crate::core::query_builder::AstPass;
use crate::core::result::{Error, QueryResult};

// Re-export submodule items
pub use block::{
    ComplexBlock, SimpleBlock,
    FromNativeBlock, FromAnyBlock, BlockValue, BlockValueRef,
    block_to_vec_optimized, for_each_row_ref, map_rows_ref,
};
pub use builder::NativeClientBuilder;
pub use column::{ToNativeBlock, IntoBlockColumn, IntoBlockColumnOwned};

// Re-export clickhouse-rs types for convenience
pub use clickhouse_rs::{Block as NativeBlock, row, types};

// Re-export Compression from core for unified API
pub use crate::core::connection::Compression;

/// Deprecated: Use `Compression` instead.
/// This type alias is kept for backward compatibility.
#[deprecated(since = "0.2.0", note = "Use `Compression` instead")]
pub type NativeCompression = Compression;

// =============================================================================
// Native Connection
// =============================================================================

/// A connection to ClickHouse via native binary protocol.
///
/// This provides better performance than HTTP by using ClickHouse's
/// native binary protocol over TCP.
///
/// # Creating a Connection
///
/// Use [`NativeClientBuilder`] to create a connection:
///
/// ```rust,ignore
/// use diesel_clickhouse::native::NativeClientBuilder;
///
/// let conn = NativeClientBuilder::new()
///     .host("localhost")
///     .port(9000)
///     .database("default")
///     .user("default")
///     .password("")
///     .build()
///     .await?;
/// ```
///
/// ## Ports
///
/// - **Port 9000**: Plain TCP (default)
/// - **Port 9440**: TLS-encrypted TCP (use `.secure(true)`)
///
/// # Cloning
///
/// Cloning a `NativeConnection` is cheap - it uses `Arc<str>` for string
/// fields and the underlying pool is also `Arc`-based.
/// A connection to ClickHouse via native binary protocol.
#[derive(Clone)]
pub struct NativeConnection {
    pool: Pool,
    /// Database name (Arc for cheap cloning)
    database: Arc<str>,
    /// Server address (host:port) for Arrow connection (Arc for cheap cloning)
    server_addr: Arc<str>,
}

impl NativeConnection {
    /// Create a connection from an existing pool.
    pub fn from_pool(
        pool: Pool,
        database: impl AsRef<str>,
        server_addr: impl AsRef<str>,
    ) -> Self {
        Self {
            pool,
            database: Arc::from(database.as_ref()),
            server_addr: Arc::from(server_addr.as_ref()),
        }
    }

    /// Get the server address (host:port).
    pub fn server_addr(&self) -> &str {
        &self.server_addr
    }

    /// Get the underlying pool for direct operations.
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Get the database name.
    pub fn database(&self) -> &str {
        &self.database
    }

    /// Get a client handle from the pool.
    ///
    /// When the `json` feature is enabled, this automatically applies the
    /// `output_format_native_write_json_as_string` setting on every handle
    /// to ensure JSON columns are serialized as strings. This is necessary
    /// because pool connections may be new sessions without the setting applied.
    pub async fn get_handle(&self) -> QueryResult<ClientHandle> {
        #[allow(unused_mut)]
        let mut handle = self.pool
            .get_handle()
            .await
            .map_err(|e| Error::ConnectionError(Cow::Owned(format!("Failed to get handle: {}", e))))?;

        // Apply JSON-as-string setting on every handle from the pool.
        // The SET command is per-session in ClickHouse, and the pool may
        // return a fresh connection that hasn't been initialized.
        #[cfg(feature = "json")]
        {
            handle
                .execute("SET output_format_native_write_json_as_string = 1")
                .await
                .map_err(|e| Error::ConnectionError(Cow::Owned(format!("Failed to enable JSON support: {}", e))))?;
        }

        Ok(handle)
    }

    /// Enable JSON-as-string mode for ClickHouse 24.10+ JSON type support.
    ///
    /// This setting configures the session to serialize JSON columns as strings
    /// instead of using the native binary format. This is recommended by ClickHouse
    /// for non-C++ clients due to TypeId instability.
    ///
    /// **Note**: When the `json` feature is enabled, this setting is automatically
    /// applied on every `get_handle()` call. This method is only needed if you
    /// want to explicitly enable it on a raw handle outside of `get_handle()`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = NativeConnection::establish("tcp://localhost:9000/default").await?;
    /// conn.enable_json_support().await?;
    /// ```
    #[cfg(feature = "json")]
    pub async fn enable_json_support(&self) -> QueryResult<()> {
        self.execute_raw("SET output_format_native_write_json_as_string = 1").await
    }

    /// Execute a raw SQL query (no results).
    pub async fn execute_raw(&self, sql: &str) -> QueryResult<()> {
        let mut client = self.get_handle().await?;
        client
            .execute(sql)
            .await
            .map_err(Error::query_from)?;
        Ok(())
    }

    /// Execute a query fragment (UPDATE, DELETE, etc).
    pub async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let sql = build_sql_interpolated(query)?;
        self.execute_raw(&sql).await
    }

    /// Build SQL from a query fragment without executing.
    pub fn build_query<Q>(&self, query: &Q) -> QueryResult<String>
    where
        Q: QueryFragment<ClickHouse>,
    {
        build_sql(query)
    }

    /// Execute a query and return the result block.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let block = conn.query_raw("SELECT id, name FROM users").await?;
    /// for row in block.rows() {
    ///     let id: u64 = row.get("id")?;
    ///     let name: String = row.get("name")?;
    ///     println!("{}: {}", id, name);
    /// }
    /// ```
    pub async fn query_raw(&self, sql: &str) -> QueryResult<Block<Complex>> {
        let mut client = self.get_handle().await?;
        client
            .query(sql)
            .fetch_all()
            .await
            .map_err(Error::query_from)
    }

    /// Execute a query fragment and return the result block.
    pub async fn query<Q>(&self, query: Q) -> QueryResult<Block<Complex>>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let sql = build_sql_interpolated(&query)?;
        self.query_raw(&sql).await
    }

    /// Insert a block of data into a table.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::native::NativeBlock;
    ///
    /// let block = NativeBlock::new()
    ///     .column("id", vec![1u64, 2, 3])
    ///     .column("name", vec!["Alice", "Bob", "Charlie"]);
    ///
    /// conn.insert("users", block).await?;
    /// ```
    pub async fn insert(&self, table: &str, block: Block) -> QueryResult<()> {
        let mut client = self.get_handle().await?;
        client
            .insert(table, block)
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(format!("Insert failed: {}", e))))?;
        Ok(())
    }

    /// Insert rows using the optimized native Block API.
    ///
    /// This method provides the best performance for bulk inserts by using
    /// ClickHouse's native binary Block format instead of generating SQL VALUES.
    ///
    /// The row type must implement `ToNativeBlock`, which is automatically
    /// generated by the `#[row]` attribute for types that also derive `Insertable`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    /// use diesel_clickhouse::native::ToNativeBlock;
    ///
    /// #[row]
    /// #[derive(Debug, Clone, Insertable)]
    /// #[diesel_clickhouse(table = users)]
    /// struct NewUser {
    ///     id: u64,
    ///     name: String,
    ///     active: bool,
    /// }
    ///
    /// let users = vec![
    ///     NewUser { id: 1, name: "Alice".into(), active: true },
    ///     NewUser { id: 2, name: "Bob".into(), active: false },
    /// ];
    ///
    /// // Optimized insert via Block API
    /// conn.insert_native("users", &users).await?;
    /// ```
    pub async fn insert_native<T: ToNativeBlock>(&self, table: &str, rows: &[T]) -> QueryResult<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let block = T::rows_to_block(rows)?;
        self.insert(table, block).await
    }

    /// Insert rows using optimized Block API, taking ownership to avoid clones.
    ///
    /// This is more efficient than `insert_native` for types containing `String`
    /// or `Vec<T>` fields, as it moves the data instead of cloning.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users = vec![
    ///     NewUser { id: 1, name: "Alice".into(), active: true },
    ///     NewUser { id: 2, name: "Bob".into(), active: false },
    /// ];
    ///
    /// // Takes ownership, avoids cloning strings
    /// conn.insert_native_owned("users", users).await?;
    /// ```
    pub async fn insert_native_owned<T: ToNativeBlock + Clone>(&self, table: &str, rows: Vec<T>) -> QueryResult<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let block = T::rows_into_block(rows)?;
        self.insert(table, block).await
    }
}

// =============================================================================
// Loading Methods
// =============================================================================

impl NativeConnection {
    /// Load rows using direct Block deserialization.
    ///
    /// This method deserializes rows directly from the native Block.
    /// Types must implement `FromNativeBlock`, which is automatically generated
    /// by `#[derive(Row)]`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(Debug, Row)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// let users: Vec<User> = conn.load(users::table.select_all()).await?;
    /// ```
    pub async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        let sql = build_sql_interpolated(&query)?;
        self.load_raw(&sql).await
    }

    /// Load rows from raw SQL.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = conn.load_raw("SELECT id, name FROM users").await?;
    /// ```
    pub async fn load_raw<T: FromNativeBlock + Send>(&self, sql: &str) -> QueryResult<Vec<T>> {
        let block = self.query_raw(sql).await?;
        block_to_vec_optimized(&block)
    }

    /// Load a single row.
    ///
    /// Returns an error if no rows are returned.
    ///
    /// This method automatically appends `LIMIT 1` to the query for efficiency,
    /// avoiding loading all rows when only one is needed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = conn.load_one(users::table.filter(users::id.eq(1))).await?;
    /// ```
    pub async fn load_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        // Append LIMIT 1 to avoid loading all rows
        // Pre-allocate string to avoid format! overhead
        let base_sql = build_sql_interpolated(&query)?;
        let sql = insert_limit_1(base_sql);
        let mut results: Vec<T> = self.load_raw(&sql).await?;
        results.pop().ok_or(Error::NotFound)
    }

    /// Load an optional single row.
    ///
    /// Returns `None` if no rows are returned.
    ///
    /// This method automatically appends `LIMIT 1` to the query for efficiency,
    /// avoiding loading all rows when only one is needed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = conn.load_optional(
    ///     users::table.filter(users::id.eq(1))
    /// ).await?;
    /// ```
    pub async fn load_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        // Append LIMIT 1 to avoid loading all rows
        // Pre-allocate string to avoid format! overhead
        let base_sql = build_sql_interpolated(&query)?;
        let sql = insert_limit_1(base_sql);
        let mut results: Vec<T> = self.load_raw(&sql).await?;
        Ok(results.pop())
    }
}

// =============================================================================
// Streaming Methods
// =============================================================================

impl NativeConnection {
    /// Stream rows from a query with a callback.
    ///
    /// This method uses true network streaming - blocks are fetched incrementally
    /// from the server and processed one at a time. Memory usage is O(block_size)
    /// instead of O(total_rows).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.stream_for_each(
    ///     users::table.filter(users::active.eq(true)),
    ///     |user: User| {
    ///         println!("User: {}", user.name);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn stream_for_each<T, Q, F>(&self, query: Q, callback: F) -> QueryResult<()>
    where
        T: FromAnyBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> QueryResult<()>,
    {
        let sql = build_sql_interpolated(&query)?;
        self.stream_for_each_raw(&sql, callback).await
    }

    /// Stream rows from raw SQL with a callback.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.stream_for_each_raw(
    ///     "SELECT id, name FROM users",
    ///     |user: User| {
    ///         println!("User: {}", user.name);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn stream_for_each_raw<T, F>(&self, sql: &str, mut callback: F) -> QueryResult<()>
    where
        T: FromAnyBlock + Send,
        F: FnMut(T) -> QueryResult<()>,
    {
        let mut client = self.get_handle().await?;
        let mut block_stream = client.query(sql).stream_blocks();

        while let Some(block_result) = block_stream.next().await {
            let block = block_result.map_err(Error::query_from)?;
            for row_idx in 0..block.row_count() {
                let item = T::from_any_block(&block, row_idx)?;
                callback(item)?;
            }
        }
        Ok(())
    }

    /// Stream rows from a query with an async callback.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.stream_for_each_async(
    ///     users::table.filter(users::active.eq(true)),
    ///     |user: User| async move {
    ///         process_user(user).await;
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn stream_for_each_async<T, Q, F, Fut>(&self, query: Q, callback: F) -> QueryResult<()>
    where
        T: FromAnyBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = QueryResult<()>>,
    {
        let sql = build_sql_interpolated(&query)?;
        self.stream_for_each_async_raw(&sql, callback).await
    }

    /// Stream rows from raw SQL with an async callback.
    pub async fn stream_for_each_async_raw<T, F, Fut>(&self, sql: &str, mut callback: F) -> QueryResult<()>
    where
        T: FromAnyBlock + Send,
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = QueryResult<()>>,
    {
        let mut client = self.get_handle().await?;
        let mut block_stream = client.query(sql).stream_blocks();

        while let Some(block_result) = block_stream.next().await {
            let block = block_result.map_err(Error::query_from)?;
            for row_idx in 0..block.row_count() {
                let item = T::from_any_block(&block, row_idx)?;
                callback(item).await?;
            }
        }
        Ok(())
    }

    /// Stream rows from a query, returning an async iterator.
    ///
    /// This method provides true network streaming - a background task reads blocks
    /// from the server and sends rows through a channel. Memory usage is O(block_size)
    /// instead of O(total_rows).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut stream = conn.stream::<User, _>(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    ///
    /// while let Some(user) = stream.next().await? {
    ///     println!("User: {}", user.name);
    /// }
    /// ```
    pub fn stream<T, Q>(&self, query: Q) -> QueryResult<crate::stream::NativeBlockStream<T>>
    where
        T: FromAnyBlock + Send + 'static,
        Q: QueryFragment<ClickHouse> + Send,
    {
        let sql = build_sql_interpolated(&query)?;
        Ok(self.stream_raw(&sql))
    }

    /// Stream rows from raw SQL, returning an async iterator.
    ///
    /// This method provides true lazy streaming with zero channel overhead.
    /// Data is fetched on-demand when `next()` is called.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut stream = conn.stream_raw::<User>("SELECT id, name FROM users");
    ///
    /// while let Some(user) = stream.next().await? {
    ///     println!("User: {}", user.name);
    /// }
    /// ```
    pub fn stream_raw<T>(&self, sql: impl Into<String>) -> crate::stream::NativeBlockStream<T>
    where
        T: FromAnyBlock + Send + 'static,
    {
        let pool = self.pool.clone();
        // Use Into<String> to avoid allocation when caller already has String
        let sql = sql.into();

        let stream = async_stream::stream! {
            // Get connection handle
            let mut client = match pool.get_handle().await {
                Ok(c) => c,
                Err(e) => {
                    yield Err(Error::ConnectionError(Cow::Owned(
                        format!("Failed to get connection handle: {}", e)
                    )));
                    return;
                }
            };

            // Stream blocks from server
            let mut block_stream = client.query(&sql).stream_blocks();

            while let Some(block_result) = block_stream.next().await {
                match block_result {
                    Ok(block) => {
                        // Yield each row from the block
                        for row_idx in 0..block.row_count() {
                            yield T::from_any_block(&block, row_idx);
                        }
                    }
                    Err(e) => {
                        yield Err(Error::query_from(e));
                        return;
                    }
                }
            }
        };

        crate::stream::NativeBlockStream::new(stream)
    }
}

// =============================================================================
// ClickHouseConnection Trait Implementation
// =============================================================================

#[async_trait]
impl ClickHouseConnectionTrait for NativeConnection {
    async fn execute_raw(&self, sql: &str) -> QueryResult<()> {
        NativeConnection::execute_raw(self, sql).await
    }

    async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        let sql = build_sql_interpolated(query)?;
        self.execute_raw(&sql).await
    }

    fn build_sql<Q>(&self, query: &Q) -> QueryResult<String>
    where
        Q: QueryFragment<ClickHouse>,
    {
        build_sql_interpolated(query)
    }

    fn database(&self) -> &str {
        &self.database
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

// Re-export from core
pub use crate::core::sql_builder::{build_sql, CompiledQuery, compile_query};

/// Build SQL from a QueryFragment with bind values interpolated inline.
///
/// This is required for the native backend since clickhouse-rs doesn't support
/// bind parameters like the HTTP backend does. All `?` placeholders are replaced
/// with their actual SQL literal values.
///
/// # Example
///
/// ```rust,ignore
/// let sql = build_sql_interpolated(&users::table.filter(users::id.eq(42)))?;
/// // sql: "SELECT * FROM `users` WHERE `id` = 42"
/// ```
pub fn build_sql_interpolated<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<String> {
    compile_query(fragment)?.to_interpolated_sql()
}

/// Insert " LIMIT 1" into a SQL string at the correct position.
///
/// - Returns SQL unchanged if it already contains ` LIMIT `
/// - Inserts ` LIMIT 1` before ` FORMAT ` or ` SETTINGS ` (whichever comes first)
/// - Appends at end if neither is present
#[inline]
fn insert_limit_1(sql: String) -> String {
    // Already has a LIMIT clause — don't add another
    if sql.contains(" LIMIT ") {
        return sql;
    }

    // Find the earliest position of FORMAT or SETTINGS
    let insert_pos = [" FORMAT ", " SETTINGS "]
        .iter()
        .filter_map(|kw| sql.find(kw))
        .min();

    match insert_pos {
        Some(pos) => {
            let mut result = String::with_capacity(sql.len() + 8);
            result.push_str(&sql[..pos]);
            result.push_str(" LIMIT 1");
            result.push_str(&sql[pos..]);
            result
        }
        None => {
            let mut s = sql;
            s.push_str(" LIMIT 1");
            s
        }
    }
}

// =============================================================================
// Query Execution Extensions
// =============================================================================

// Re-export ToSqlString from core
pub use crate::core::sql_builder::ToSqlString;

/// Extension trait for executing mutations.
#[async_trait]
pub trait ExecuteMut: QueryFragment<ClickHouse> + Send + Sync + Sized {
    /// Execute the query on a native connection.
    async fn execute_native(self, conn: &NativeConnection) -> QueryResult<()> {
        conn.execute_statement(&self).await
    }
}

impl<T: QueryFragment<ClickHouse> + Send + Sync> ExecuteMut for T {}

// =============================================================================
// Async Insert Support
// =============================================================================

impl NativeConnection {
    /// Insert rows using async insert mode with Block format.
    ///
    /// This method uses ClickHouse's async insert mode which buffers data
    /// server-side for optimal write performance.
    ///
    /// Note: This applies the async insert settings before each insert.
    /// For better performance with many small inserts, use `AsyncInserter`
    /// which caches the settings application.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::async_insert::AsyncInsertConfig;
    ///
    /// let config = AsyncInsertConfig::fire_and_forget();
    /// conn.async_insert_rows::<events::table, _>(&config, &events).await?;
    /// ```
    pub async fn async_insert_rows<T, R>(
        &self,
        config: &crate::async_insert::AsyncInsertConfig,
        rows: &[R],
    ) -> QueryResult<()>
    where
        T: crate::core::query_source::Table,
        R: ToNativeBlock + Send,
    {
        if rows.is_empty() {
            return Ok(());
        }

        // Apply async insert settings (single batched command)
        self.execute_raw(&config.to_native_set_command()).await?;

        let block = R::rows_to_block(rows)?;
        self.insert(T::table_name(), block).await
    }

    /// Force the server to flush its async insert buffer.
    pub async fn flush_async_insert_queue(&self) -> QueryResult<()> {
        self.execute_raw("SYSTEM FLUSH ASYNC INSERT QUEUE").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::query_builder::SelectStatement;
    use crate::core::test_utils::RawTable;

    #[test]
    fn test_build_sql() {
        let query = SelectStatement::new(RawTable("test_table"));
        let result = build_sql(&query).expect("failed to build SQL");
        assert_eq!(result, "SELECT * FROM test_table");
    }

    #[test]
    fn test_insert_limit_1_plain_query() {
        let sql = "SELECT * FROM users".to_string();
        assert_eq!(insert_limit_1(sql), "SELECT * FROM users LIMIT 1");
    }

    #[test]
    fn test_insert_limit_1_existing_limit() {
        let sql = "SELECT * FROM users LIMIT 100".to_string();
        assert_eq!(insert_limit_1(sql), "SELECT * FROM users LIMIT 100");
    }

    #[test]
    fn test_insert_limit_1_with_format() {
        let sql = "SELECT * FROM users FORMAT JSONEachRow".to_string();
        assert_eq!(insert_limit_1(sql), "SELECT * FROM users LIMIT 1 FORMAT JSONEachRow");
    }

    #[test]
    fn test_insert_limit_1_with_settings() {
        let sql = "SELECT * FROM users SETTINGS max_threads=4".to_string();
        assert_eq!(insert_limit_1(sql), "SELECT * FROM users LIMIT 1 SETTINGS max_threads=4");
    }

    #[test]
    fn test_insert_limit_1_with_format_and_settings() {
        let sql = "SELECT * FROM users FORMAT JSON SETTINGS max_threads=4".to_string();
        assert_eq!(insert_limit_1(sql), "SELECT * FROM users LIMIT 1 FORMAT JSON SETTINGS max_threads=4");
    }
}
