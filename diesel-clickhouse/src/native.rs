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

use async_trait::async_trait;
use clickhouse_rs::{Pool, ClientHandle, Block, types::Complex};

use crate::core::backend::{ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::core::connection::ClickHouseConnection as ClickHouseConnectionTrait;
use crate::core::escape::escape_identifier;
use crate::core::query_builder::{AstPass, QueryFragment};
use crate::core::result::{Error, QueryResult};

use std::time::Duration;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

// Re-export clickhouse-rs types for convenience
pub use clickhouse_rs::{Block as NativeBlock, row, types};

/// Type alias for the complex block type used by FromNativeBlock
pub type ComplexBlock = Block<Complex>;

// =============================================================================
// Compression Mode
// =============================================================================

/// Compression mode for native protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeCompression {
    /// No compression (default).
    #[default]
    None,
    /// LZ4 compression.
    Lz4,
}

// =============================================================================
// Client Builder
// =============================================================================

/// Builder for configuring a ClickHouse Native connection.
///
/// All connection parameters (host, port, database, user, password) are required.
/// Optional settings include compression, TLS, timeouts, and pool configuration.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::native::NativeClientBuilder;
/// use diesel_clickhouse::native::NativeCompression;
/// use std::time::Duration;
///
/// let conn = NativeClientBuilder::new()
///     .host("localhost")
///     .port(9000)
///     .database("analytics")
///     .user("default")
///     .password("")
///     .compression(NativeCompression::Lz4)
///     .pool_max(20)
///     .query_timeout(Duration::from_secs(180))
///     .build()
///     .await?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct NativeClientBuilder {
    host: Option<String>,
    port: Option<u16>,
    database: Option<String>,
    user: Option<String>,
    password: Option<String>,
    compression: NativeCompression,
    secure: bool,
    skip_verify: bool,
    connection_timeout: Option<Duration>,
    ping_timeout: Option<Duration>,
    query_timeout: Option<Duration>,
    pool_min: Option<usize>,
    pool_max: Option<usize>,
}

impl NativeClientBuilder {
    /// Create a new Native client builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the host (required).
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Set the port (required).
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the database (required).
    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    /// Set the user (required).
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Set the password (required).
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Set compression mode (optional, default: None).
    pub fn compression(mut self, compression: NativeCompression) -> Self {
        self.compression = compression;
        self
    }

    /// Enable TLS (optional, default: false).
    ///
    /// Requires the `native-tls-native` feature.
    pub fn secure(mut self, enabled: bool) -> Self {
        self.secure = enabled;
        self
    }

    /// Skip TLS certificate verification (optional, default: false).
    ///
    /// Warning: This is insecure and should only be used for testing.
    pub fn skip_verify(mut self, enabled: bool) -> Self {
        self.skip_verify = enabled;
        self
    }

    /// Set connection timeout (optional).
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = Some(timeout);
        self
    }

    /// Set ping timeout (optional).
    pub fn ping_timeout(mut self, timeout: Duration) -> Self {
        self.ping_timeout = Some(timeout);
        self
    }

    /// Set query timeout (optional).
    pub fn query_timeout(mut self, timeout: Duration) -> Self {
        self.query_timeout = Some(timeout);
        self
    }

    /// Set minimum pool size (optional).
    pub fn pool_min(mut self, min: usize) -> Self {
        self.pool_min = Some(min);
        self
    }

    /// Set maximum pool size (optional).
    pub fn pool_max(mut self, max: usize) -> Self {
        self.pool_max = Some(max);
        self
    }

    /// Build and establish the connection.
    ///
    /// Returns a unified `Connection` that can be used with all interfaces.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Required fields (host, port, database, user, password) are not set
    /// - Connection to the server fails
    pub async fn build(self) -> QueryResult<crate::Connection> {
        let host = self.host.ok_or_else(||
            Error::ConnectionError("host is required".to_string()))?;
        let port = self.port.ok_or_else(||
            Error::ConnectionError("port is required".to_string()))?;
        let database = self.database.ok_or_else(||
            Error::ConnectionError("database is required".to_string()))?;
        let user = self.user.ok_or_else(||
            Error::ConnectionError("user is required".to_string()))?;
        let password = self.password.ok_or_else(||
            Error::ConnectionError("password is required".to_string()))?;

        // URL-encode user, password, and database to handle special characters
        let encoded_user = utf8_percent_encode(&user, NON_ALPHANUMERIC).to_string();
        let encoded_password = utf8_percent_encode(&password, NON_ALPHANUMERIC).to_string();
        let encoded_database = utf8_percent_encode(&database, NON_ALPHANUMERIC).to_string();

        // Build URL with query parameters
        let mut url = format!(
            "tcp://{}:{}@{}:{}/{}",
            encoded_user, encoded_password, host, port, encoded_database
        );
        let mut params = Vec::new();

        if self.secure {
            params.push("secure=true".to_string());
        }
        if self.skip_verify {
            params.push("skip_verify=true".to_string());
        }
        if self.compression == NativeCompression::Lz4 {
            params.push("compression=lz4".to_string());
        }
        if let Some(t) = self.connection_timeout {
            params.push(format!("connection_timeout={}ms", t.as_millis()));
        }
        if let Some(t) = self.ping_timeout {
            params.push(format!("ping_timeout={}ms", t.as_millis()));
        }
        if let Some(t) = self.query_timeout {
            params.push(format!("query_timeout={}s", t.as_secs()));
        }
        if let Some(min) = self.pool_min {
            params.push(format!("pool_min={}", min));
        }
        if let Some(max) = self.pool_max {
            params.push(format!("pool_max={}", max));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let native_conn = NativeConnection::establish(&url).await?;

        // Test connection (like HTTP builder does)
        native_conn.execute_raw("SELECT 1").await?;

        Ok(crate::Connection::Native(native_conn))
    }
}

// =============================================================================
// Direct Block Deserialization (optimized, no JSON intermediate)
// =============================================================================

/// Trait for types that can be deserialized directly from a Native Block row.
///
/// This trait is automatically implemented by `#[derive(Row)]` and provides
/// optimized deserialization without JSON intermediate conversion.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::{Row, native::FromNativeBlock};
///
/// #[derive(Debug, Row)]
/// struct User {
///     id: u64,
///     name: String,
/// }
///
/// // FromNativeBlock is auto-implemented, allowing direct Block deserialization
/// ```
pub trait FromNativeBlock: Sized {
    /// Deserialize a row from a Native Block at the given index.
    fn from_block_row(
        block: &ComplexBlock,
        row_idx: usize,
    ) -> QueryResult<Self>;
}

/// Helper trait for extracting typed values from a Block column.
///
/// This is used by the `#[derive(Row)]` macro to extract individual field values.
pub trait BlockValue: Sized {
    /// Get a value from the block at the given row and column name.
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self>;
}

// Implement BlockValue for common types
impl BlockValue for u8 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get u8 column '{}': {}", column, e)))
    }
}

impl BlockValue for u16 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get u16 column '{}': {}", column, e)))
    }
}

impl BlockValue for u32 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get u32 column '{}': {}", column, e)))
    }
}

impl BlockValue for u64 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get u64 column '{}': {}", column, e)))
    }
}

impl BlockValue for i8 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get i8 column '{}': {}", column, e)))
    }
}

impl BlockValue for i16 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get i16 column '{}': {}", column, e)))
    }
}

impl BlockValue for i32 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get i32 column '{}': {}", column, e)))
    }
}

impl BlockValue for i64 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get i64 column '{}': {}", column, e)))
    }
}

impl BlockValue for f32 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get f32 column '{}': {}", column, e)))
    }
}

impl BlockValue for f64 {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get f64 column '{}': {}", column, e)))
    }
}

impl BlockValue for String {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        let s: &str = block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get String column '{}': {}", column, e)))?;
        Ok(s.to_string())
    }
}

impl BlockValue for bool {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        // ClickHouse stores bools as u8
        let v: u8 = block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(format!("Failed to get bool column '{}': {}", column, e)))?;
        Ok(v != 0)
    }
}

impl<T: BlockValue> BlockValue for Option<T> {
    fn get_value(block: &ComplexBlock, row_idx: usize, column: &str) -> QueryResult<Self> {
        // Try to get the value, return None if it's NULL
        match T::get_value(block, row_idx, column) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None), // Assume error means NULL for Nullable columns
        }
    }
}

/// Convert a native Block to a Vec using FromNativeBlock trait (optimized).
pub fn block_to_vec_optimized<T: FromNativeBlock>(block: &ComplexBlock) -> QueryResult<Vec<T>> {
    let row_count = block.row_count();
    let mut results = Vec::with_capacity(row_count);

    for row_idx in 0..row_count {
        results.push(T::from_block_row(block, row_idx)?);
    }

    Ok(results)
}

// =============================================================================
// Native Connection
// =============================================================================

/// A connection to ClickHouse via native binary protocol.
///
/// This provides better performance than HTTP by using ClickHouse's
/// native binary protocol over TCP.
///
/// # Connection URL Format
///
/// ```text
/// tcp://[user:password@]host[:port]/database[?options]
/// ```
///
/// ## Ports
///
/// - **Port 9000**: Plain TCP (default)
/// - **Port 9440**: TLS-encrypted TCP (use `?secure=true`)
///
/// ## Examples
///
/// ```rust,ignore
/// // Simple connection
/// let conn = NativeConnection::establish("tcp://localhost/default").await?;
///
/// // With authentication and options
/// let conn = NativeConnection::establish(
///     "tcp://admin:secret@ch.example.com:9000/analytics?compression=lz4"
/// ).await?;
///
/// // With TLS
/// let conn = NativeConnection::establish(
///     "tcp://admin:secret@ch.example.com:9440/analytics?secure=true"
/// ).await?;
/// ```
#[derive(Clone)]
pub struct NativeConnection {
    pool: Pool,
    database: String,
}

impl NativeConnection {
    /// Connect to ClickHouse using a connection URL.
    ///
    /// # Connection URL Format
    ///
    /// ```text
    /// tcp://[user:password@]host[:port]/database[?options]
    /// ```
    ///
    /// # Options
    ///
    /// - `secure=true` - Enable TLS
    /// - `skip_verify=true` - Skip TLS certificate verification
    /// - `compression=lz4` - Enable LZ4 compression
    /// - `connection_timeout=500ms` - Connection timeout
    /// - `query_timeout=180s` - Query timeout
    /// - `pool_min=5` - Minimum pool connections
    /// - `pool_max=10` - Maximum pool connections
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = NativeConnection::establish(
    ///     "tcp://default:@localhost:9000/default"
    /// ).await?;
    /// ```
    pub async fn establish(url: &str) -> QueryResult<Self> {
        let pool = Pool::new(url);

        // Extract database from URL
        let database = extract_database_from_url(url);

        // Test connection
        let mut client = pool
            .get_handle()
            .await
            .map_err(|e| Error::ConnectionError(format!("Failed to connect: {}", e)))?;

        client
            .query("SELECT 1")
            .fetch_all()
            .await
            .map_err(|e| Error::ConnectionError(format!("Connection test failed: {}", e)))?;

        Ok(Self { pool, database })
    }

    /// Create a connection from an existing pool.
    pub fn from_pool(pool: Pool, database: impl Into<String>) -> Self {
        Self {
            pool,
            database: database.into(),
        }
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
    pub async fn get_handle(&self) -> QueryResult<ClientHandle> {
        self.pool
            .get_handle()
            .await
            .map_err(|e| Error::ConnectionError(format!("Failed to get handle: {}", e)))
    }

    /// Execute a raw SQL query (no results).
    pub async fn execute_raw(&self, sql: &str) -> QueryResult<()> {
        let mut client = self.get_handle().await?;
        client
            .execute(sql)
            .await
            .map_err(|e| Error::QueryError(e.to_string()))?;
        Ok(())
    }

    /// Execute a query fragment (UPDATE, DELETE, etc).
    pub async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let sql = build_sql(query)?;
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
            .map_err(|e| Error::QueryError(e.to_string()))
    }

    /// Execute a query fragment and return the result block.
    pub async fn query<Q>(&self, query: Q) -> QueryResult<Block<Complex>>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let sql = build_sql(&query)?;
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
            .map_err(|e| Error::QueryError(format!("Insert failed: {}", e)))?;
        Ok(())
    }

    /// Insert data using raw SQL VALUES.
    ///
    /// # Safety
    ///
    /// The `values_sql` parameter is inserted directly into the SQL query.
    /// The caller is responsible for properly escaping any user-provided data
    /// within `values_sql` to prevent SQL injection.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.insert_values("users", "(1, 'Alice'), (2, 'Bob')").await?;
    /// ```
    pub async fn insert_values(&self, table: &str, values_sql: &str) -> QueryResult<()> {
        let escaped_table = escape_identifier(table);
        let sql = format!("INSERT INTO {} VALUES {}", escaped_table, values_sql);
        self.execute_raw(&sql).await
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Extract database name from connection URL.
fn extract_database_from_url(url: &str) -> String {
    // Parse: tcp://user:pass@host:port/database?options
    if let Some(path_start) = url.find("://") {
        let rest = &url[path_start + 3..];
        // Find the path after host:port
        if let Some(slash_pos) = rest.find('/') {
            let after_slash = &rest[slash_pos + 1..];
            // Remove query string
            let db = after_slash.split('?').next().unwrap_or("default");
            if !db.is_empty() {
                return db.to_string();
            }
        }
    }
    "default".to_string()
}

/// Build SQL from a QueryFragment.
///
/// Returns an error if the query fragment fails to produce valid SQL.
pub fn build_sql<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<String> {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)?;
    Ok(builder.finish())
}

// =============================================================================
// Query Execution Extensions
// =============================================================================

/// Extension trait for query fragments to get SQL.
pub trait ToSql: QueryFragment<ClickHouse> {
    /// Convert to SQL string.
    ///
    /// Returns an error if the query fragment fails to produce valid SQL.
    fn to_sql_string(&self) -> QueryResult<String> {
        build_sql(self)
    }
}

impl<T: QueryFragment<ClickHouse>> ToSql for T {}

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
// Unified ClickHouseConnection Implementation
// =============================================================================

impl NativeConnection {
    // =========================================================================
    // Optimized Loading (direct Block deserialization)
    // =========================================================================

    /// Load rows using optimized direct Block deserialization.
    ///
    /// This method deserializes rows directly from the native Block without
    /// JSON intermediate conversion, providing 2-3x better performance than
    /// `load_json()`.
    ///
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
    /// // Optimized: direct Block → struct deserialization
    /// let users: Vec<User> = conn.load_optimized(users::table.select_all()).await?;
    /// ```
    pub async fn load_optimized<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        let sql = build_sql(&query)?;
        self.load_optimized_raw(&sql).await
    }

    /// Load rows from raw SQL using optimized direct Block deserialization.
    ///
    /// This is the raw SQL version of `load_optimized()`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = conn.load_optimized_raw("SELECT id, name FROM users").await?;
    /// ```
    pub async fn load_optimized_raw<T: FromNativeBlock + Send>(&self, sql: &str) -> QueryResult<Vec<T>> {
        let block = self.query_raw(sql).await?;
        block_to_vec_optimized(&block)
    }

    /// Load a single row using optimized deserialization.
    ///
    /// Returns an error if no rows are returned.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = conn.load_optimized_one(users::table.filter(users::id.eq(1))).await?;
    /// ```
    pub async fn load_optimized_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        let mut results = self.load_optimized(query).await?;
        results.pop().ok_or_else(|| Error::NotFound)
    }

    /// Load an optional single row using optimized deserialization.
    ///
    /// Returns `None` if no rows are returned.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = conn.load_optimized_optional(
    ///     users::table.filter(users::id.eq(1))
    /// ).await?;
    /// ```
    pub async fn load_optimized_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        let mut results = self.load_optimized(query).await?;
        Ok(results.pop())
    }
}

#[async_trait]
impl ClickHouseConnectionTrait for NativeConnection {
    async fn establish(url: &str) -> QueryResult<Self> {
        Self::establish(url).await
    }

    async fn execute_raw(&self, sql: &str) -> QueryResult<()> {
        NativeConnection::execute_raw(self, sql).await
    }

    async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        let sql = build_sql(query)?;
        self.execute_raw(&sql).await
    }

    fn build_sql<Q>(&self, query: &Q) -> QueryResult<String>
    where
        Q: QueryFragment<ClickHouse>,
    {
        build_sql(query)
    }

    fn database(&self) -> &str {
        &self.database
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::query_builder::SelectStatement;

    #[test]
    fn test_build_sql() {
        struct TestTable;
        impl QueryFragment<ClickHouse> for TestTable {
            fn walk_ast<'b>(
                &'b self,
                mut pass: AstPass<'_, 'b, ClickHouse>,
            ) -> QueryResult<()> {
                pass.push_sql("test_table");
                Ok(())
            }
        }

        let query = SelectStatement::new(TestTable);
        let result = build_sql(&query).expect("failed to build SQL");
        assert_eq!(result, "SELECT * FROM test_table");
    }

    #[test]
    fn test_extract_database() {
        assert_eq!(
            extract_database_from_url("tcp://localhost:9000/mydb"),
            "mydb"
        );
        assert_eq!(
            extract_database_from_url("tcp://user:pass@localhost/analytics"),
            "analytics"
        );
        assert_eq!(
            extract_database_from_url("tcp://localhost:9000/test?secure=true"),
            "test"
        );
        assert_eq!(
            extract_database_from_url("tcp://localhost:9000/"),
            "default"
        );
        assert_eq!(
            extract_database_from_url("tcp://localhost"),
            "default"
        );
    }
}
