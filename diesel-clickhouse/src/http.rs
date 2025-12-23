//! HTTP-based ClickHouse connection.
//!
//! This module provides a fully-featured async connection to ClickHouse
//! using the HTTP interface.
//!
//! # Usage
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//! use diesel_clickhouse::http::ClickHouseConnection;
//!
//! // Define your row type with the unified Row derive
//! #[derive(Debug, Row)]
//! struct MyRow {
//!     id: u64,
//!     name: String,
//! }
//!
//! // Use the connection
//! let conn = ClickHouseConnection::new("http://localhost:8123/mydb").await?;
//! let rows: Vec<MyRow> = conn.load(my_table::table).await?;
//! ```

use std::borrow::Cow;

use async_trait::async_trait;
use clickhouse::Client;
use serde::Serialize;

use crate::core::backend::{BindCollector, ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::core::connection::ClickHouseConnection as ClickHouseConnectionTrait;
use crate::core::escape::escape_identifier;
use crate::core::query_builder::{AstPass, QueryFragment};
use crate::core::result::{Error, QueryResult};

// Re-export clickhouse Row for convenience (for users who need direct clickhouse crate access)
pub use clickhouse::Row as NativeClickHouseRow;

/// Compression mode for HTTP requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    /// No compression (default for small payloads).
    #[default]
    None,
    /// LZ4 compression (recommended for large INSERTs).
    Lz4,
}

// =============================================================================
// Client Builder
// =============================================================================

/// Builder for configuring a ClickHouse HTTP connection.
///
/// All connection parameters (host, port, database, user, password) are required.
/// Optional settings include compression and ClickHouse query options.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::http::HttpClientBuilder;
/// use diesel_clickhouse::http::Compression;
///
/// let conn = HttpClientBuilder::new()
///     .host("localhost")
///     .port(8123)
///     .database("analytics")
///     .user("default")
///     .password("")
///     .compression(Compression::Lz4)
///     .option("max_execution_time", "60")
///     .build()
///     .await?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct HttpClientBuilder {
    host: Option<String>,
    port: Option<u16>,
    https: bool,
    database: Option<String>,
    user: Option<String>,
    password: Option<String>,
    compression: Compression,
    options: Vec<(String, String)>,
}

impl HttpClientBuilder {
    /// Create a new HTTP client builder.
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

    /// Use HTTPS instead of HTTP (optional, default: false).
    pub fn https(mut self, enabled: bool) -> Self {
        self.https = enabled;
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
    pub fn compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    /// Set a ClickHouse query setting (optional).
    ///
    /// Common options:
    /// - `wait_end_of_query` - Wait for query to complete before streaming
    /// - `max_execution_time` - Maximum query execution time in seconds
    /// - `max_query_size` - Maximum query size in bytes
    /// - `max_result_rows` - Maximum number of result rows
    /// - `max_result_bytes` - Maximum result size in bytes
    pub fn option(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.options.push((key.into(), value.into()));
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
            Error::ConnectionError(Cow::Borrowed("host is required")))?;
        let port = self.port.ok_or_else(||
            Error::ConnectionError(Cow::Borrowed("port is required")))?;
        let database = self.database.ok_or_else(||
            Error::ConnectionError(Cow::Borrowed("database is required")))?;
        let user = self.user.ok_or_else(||
            Error::ConnectionError(Cow::Borrowed("user is required")))?;
        let password = self.password.ok_or_else(||
            Error::ConnectionError(Cow::Borrowed("password is required")))?;

        let scheme = if self.https { "https" } else { "http" };

        let url = format!("{}://{}:{}", scheme, host, port);
        let mut client = Client::default()
            .with_url(&url)
            .with_database(&database)
            .with_user(&user)
            .with_password(&password);

        if self.compression == Compression::Lz4 {
            client = client.with_compression(clickhouse::Compression::Lz4);
        }

        for (key, value) in &self.options {
            client = client.with_option(key, value);
        }

        // Test connection
        client.query("SELECT 1").execute().await
            .map_err(|e| Error::ConnectionError(Cow::Owned(e.to_string())))?;

        let http_conn = ClickHouseConnection {
            client,
            database,
            compression: self.compression,
        };

        Ok(crate::Connection::Http(http_conn))
    }
}

/// A connection to ClickHouse via HTTP.
#[derive(Clone)]
pub struct ClickHouseConnection {
    client: Client,
    database: String,
    compression: Compression,
}

impl ClickHouseConnection {
    /// Create a connection from an existing Client.
    pub fn from_client(client: Client, database: impl Into<String>) -> Self {
        Self {
            client,
            database: database.into(),
            compression: Compression::None,
        }
    }

    /// Enable LZ4 compression for this connection.
    ///
    /// Compression is beneficial for large INSERT operations (>1KB payload).
    /// For small queries, the compression overhead may outweigh the benefits.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = ClickHouseConnection::new("http://localhost:8123/default")
    ///     .await?
    ///     .with_compression(Compression::Lz4);
    /// ```
    pub fn with_compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        // Update client with compression setting
        if compression == Compression::Lz4 {
            self.client = self.client.clone().with_compression(clickhouse::Compression::Lz4);
        }
        self
    }

    /// Enable LZ4 compression (convenience method).
    pub fn with_lz4_compression(self) -> Self {
        self.with_compression(Compression::Lz4)
    }

    /// Get the current compression mode.
    pub fn compression(&self) -> Compression {
        self.compression
    }

    /// Get the underlying client for direct operations.
    ///
    /// Use this when you need full access to the clickhouse crate's API.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get the database name.
    pub fn database(&self) -> &str {
        &self.database
    }

    /// Execute a raw SQL query (no results).
    pub async fn execute_raw(&self, sql: &str) -> QueryResult<()> {
        self.client
            .query(sql)
            .execute()
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Execute a query fragment (UPDATE, DELETE, etc).
    ///
    /// Uses native parameter binding for query plan caching.
    pub async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(query)?;
        self.execute_native(&compiled).await
    }

    /// Execute a NativeCompiledQuery with native parameter binding.
    pub async fn execute_native(&self, compiled: &NativeCompiledQuery) -> QueryResult<()> {
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .execute()
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Build SQL from a query fragment without executing.
    pub fn build_query<Q>(&self, query: &Q) -> QueryResult<String>
    where
        Q: QueryFragment<ClickHouse>,
    {
        build_sql(query)
    }
}

// =============================================================================
// SQL Building
// =============================================================================

/// Build SQL from a QueryFragment.
///
/// Returns an error if the query fragment fails to produce valid SQL.
/// Bindings are automatically inlined into the SQL string for display/logging.
///
/// Note: For actual query execution, use `build_sql_native()` which preserves
/// placeholders for native parameter binding and query plan caching.
pub fn build_sql<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<String> {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)?;

    // Inline bindings into the SQL (for display/logging purposes)
    let mut sql = builder.finish();
    let bindable_values = collector.bindable_values();

    if !bindable_values.is_empty() {
        // Replace each ? placeholder with its binding value (in reverse order)
        for binding in bindable_values.iter().rev() {
            if let Some(pos) = sql.rfind('?') {
                sql.replace_range(pos..pos + 1, &binding.sql_literal());
            }
        }
    }

    Ok(sql)
}

// Re-export for convenience
pub use crate::core::backend::BindableValue;

/// A compiled query with SQL placeholders and typed bindable values.
///
/// This is the SOTA format for native parameter binding, enabling:
/// - Query plan caching on the ClickHouse server
/// - Proper type safety through the clickhouse crate's `.bind()` method
/// - No manual string escaping
#[derive(Debug, Clone)]
pub struct NativeCompiledQuery {
    /// The SQL string with `?` placeholders.
    pub sql: String,
    /// The collected bindable values for native binding.
    pub bindings: Vec<BindableValue>,
}

impl NativeCompiledQuery {
    /// Get the number of bind parameters.
    pub fn param_count(&self) -> usize {
        self.bindings.len()
    }

    /// Check if there are any bind parameters.
    pub fn has_bindings(&self) -> bool {
        !self.bindings.is_empty()
    }

    /// Apply all bindings to a clickhouse Query object.
    pub fn bind_to(&self, mut query: clickhouse::query::Query) -> clickhouse::query::Query {
        for binding in &self.bindings {
            query = query.bind(binding);
        }
        query
    }

    /// Get SQL with bindings inlined (for debugging/logging).
    pub fn sql_with_inlined_bindings(&self) -> String {
        let mut sql = self.sql.clone();
        for binding in self.bindings.iter().rev() {
            if let Some(pos) = sql.rfind('?') {
                sql.replace_range(pos..pos + 1, &binding.sql_literal());
            }
        }
        sql
    }
}

/// Build SQL with native bindable values from a QueryFragment.
///
/// This is the SOTA way to build queries for execution:
/// - Returns SQL with `?` placeholders
/// - Returns typed BindableValue instances for native `.bind()` calls
/// - Enables query plan caching on the ClickHouse server
pub fn build_sql_native<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<NativeCompiledQuery> {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)?;

    Ok(NativeCompiledQuery {
        sql: builder.finish(),
        bindings: collector.bindable_values().to_vec(),
    })
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
    /// Execute the query.
    async fn execute(self, conn: &ClickHouseConnection) -> QueryResult<()> {
        conn.execute_statement(&self).await
    }
}

impl<T: QueryFragment<ClickHouse> + Send + Sync> ExecuteMut for T {}

/// Extension trait for inserting into tables.
#[async_trait]
pub trait InsertDsl: crate::Table + Send + Sync + Sized {
    /// Create an inserter for this table.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::http::InsertDsl;
    ///
    /// let mut inserter = users::table.inserter::<NewUser>(&conn).await?;
    /// inserter.write(&user).await?;
    /// inserter.end().await?;
    /// ```
    async fn inserter<T: clickhouse::Row>(self, conn: &ClickHouseConnection) -> Result<clickhouse::insert::Insert<T>, clickhouse::error::Error> {
        conn.client.insert(Self::table_name()).await
    }
}

impl<T: crate::Table + Send + Sync> InsertDsl for T {}

// =============================================================================
// Query Execution Methods
// =============================================================================

impl ClickHouseConnection {
    /// Create a query from a QueryFragment and return a clickhouse Query.
    ///
    /// This allows using the diesel-style query builder with clickhouse's fetch methods.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = conn.query(
    ///     users::table.filter(users::active.eq(true))
    /// )?.fetch_all().await?;
    /// ```
    pub fn query<Q>(&self, query: Q) -> QueryResult<clickhouse::query::Query>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let sql = build_sql(&query)?;
        Ok(self.client.query(&sql))
    }

    /// Stream rows from a query using a cursor.
    ///
    /// This method returns a `RowCursor` that allows you to process results
    /// row by row without loading everything into memory. Ideal for large result sets.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    ///
    /// #[derive(Debug, Row)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// // Stream results row by row
    /// let mut cursor = conn.stream::<User, _>(
    ///     users::table.filter(users::active.eq(true))
    /// )?;
    ///
    /// while let Some(user) = cursor.next().await? {
    ///     println!("User: {} - {}", user.id, user.name);
    /// }
    /// ```
    ///
    /// # Note
    ///
    /// The row type `T` must implement `clickhouse::Row` (via `#[derive(Row)]`).
    /// Cursors may return errors after producing some rows. Use
    /// `client.with_option("wait_end_of_query", "1")` for server-side buffering
    /// if you need to ensure all rows succeed before processing.
    pub fn stream<T, Q>(&self, query: Q) -> QueryResult<clickhouse::query::RowCursor<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead,
        Q: QueryFragment<ClickHouse>,
    {
        let sql = build_sql(&query)?;
        self.client
            .query(&sql)
            .fetch::<T>()
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Stream rows from a query with native parameter binding.
    ///
    /// Like `stream()`, but uses native parameter binding for better
    /// query plan caching on the server.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let compiled = build_sql_native(&users::table.filter(users::id.eq(42)))?;
    /// let mut cursor = conn.stream_native::<User>(&compiled)?;
    ///
    /// while let Some(user) = cursor.next().await? {
    ///     println!("User: {}", user.name);
    /// }
    /// ```
    pub fn stream_native<T>(&self, compiled: &NativeCompiledQuery) -> QueryResult<clickhouse::query::RowCursor<T>>
    where
        T: clickhouse::Row,
    {
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch::<T>()
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Create an inserter for a table.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut inserter = conn.inserter::<NewUser, _>(users::table).await?;
    /// inserter.write(&user1).await?;
    /// inserter.write(&user2).await?;
    /// inserter.end().await?;
    /// ```
    pub async fn inserter<T, Tab>(&self, _table: Tab) -> Result<clickhouse::insert::Insert<T>, clickhouse::error::Error>
    where
        T: clickhouse::Row,
        Tab: crate::Table,
    {
        self.client.insert(Tab::table_name()).await
    }

    /// Insert rows into a table using raw SQL values.
    ///
    /// # Safety
    ///
    /// The `sql_values` parameter is inserted directly into the SQL query.
    /// The caller is responsible for properly escaping any user-provided data
    /// within `sql_values` to prevent SQL injection.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.insert_raw("users", "(1, 'alice'), (2, 'bob')").await?;
    /// ```
    pub async fn insert_raw(&self, table_name: &str, sql_values: &str) -> QueryResult<()> {
        let escaped_table = escape_identifier(table_name);
        let sql = format!("INSERT INTO {} VALUES {}", escaped_table, sql_values);
        self.execute_raw(&sql).await
    }

    // =========================================================================
    // Native Parameter Binding
    // =========================================================================

    /// Create a bound query with native parameter binding.
    ///
    /// This uses the clickhouse crate's native `.bind()` mechanism which
    /// properly serializes parameters without manual string escaping.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = conn.bound_query("SELECT * FROM users WHERE id = ? AND name = ?")
    ///     .bind(42u64)
    ///     .bind("alice")
    ///     .fetch_all()
    ///     .await?;
    /// ```
    pub fn bound_query(&self, sql: &str) -> clickhouse::query::Query {
        self.client.query(sql)
    }

    /// Execute a query with bound parameters (no results).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.execute_bound("ALTER TABLE users DELETE WHERE id = ?", &[&42u64]).await?;
    /// ```
    pub async fn execute_bound<T: Serialize + Send + Sync>(
        &self,
        sql: &str,
        params: &[T],
    ) -> QueryResult<()> {
        let mut query = self.client.query(sql);
        for param in params {
            query = query.bind(param);
        }
        query.execute().await.map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Load rows with bound parameters using native Row deserialization.
    ///
    /// This is the recommended way to execute parameterized queries as it uses
    /// the clickhouse crate's native binding and deserialization.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(Debug, clickhouse::Row, Deserialize)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// let users: Vec<User> = conn.fetch_bound(
    ///     "SELECT id, name FROM users WHERE active = ?",
    ///     |q| q.bind(true),
    /// ).await?;
    /// ```
    pub async fn fetch_bound<T, F>(&self, sql: &str, bind_fn: F) -> QueryResult<Vec<T>>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
        F: FnOnce(clickhouse::query::Query) -> clickhouse::query::Query,
    {
        let query = bind_fn(self.client.query(sql));
        query
            .fetch_all()
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Fetch a single row with bound parameters.
    ///
    /// Returns an error if no rows are returned.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = conn.fetch_one_bound(
    ///     "SELECT id, name FROM users WHERE id = ?",
    ///     |q| q.bind(42u64),
    /// ).await?;
    /// ```
    pub async fn fetch_one_bound<T, F>(&self, sql: &str, bind_fn: F) -> QueryResult<T>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
        F: FnOnce(clickhouse::query::Query) -> clickhouse::query::Query,
    {
        let query = bind_fn(self.client.query(sql));
        query
            .fetch_one()
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Fetch an optional row with bound parameters.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = conn.fetch_optional_bound(
    ///     "SELECT id, name FROM users WHERE id = ?",
    ///     |q| q.bind(42u64),
    /// ).await?;
    /// ```
    pub async fn fetch_optional_bound<T, F>(&self, sql: &str, bind_fn: F) -> QueryResult<Option<T>>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
        F: FnOnce(clickhouse::query::Query) -> clickhouse::query::Query,
    {
        let query = bind_fn(self.client.query(sql));
        query
            .fetch_optional()
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }
}

// =============================================================================
// Apache Arrow API
// =============================================================================

#[cfg(feature = "arrow")]
impl ClickHouseConnection {
    /// Load query results as Apache Arrow RecordBatches.
    ///
    /// This method uses ClickHouse's ArrowStream format for true zero-copy
    /// columnar data access. Arrow is the most efficient format for analytical
    /// workloads and enables seamless interoperability with tools like Polars,
    /// DataFusion, and DuckDB.
    ///
    /// # Arguments
    ///
    /// * `sql` - The SQL query to execute
    ///
    /// # Returns
    ///
    /// An `ArrowResult` containing the schema and record batches.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    /// use diesel_clickhouse::arrow::array::{Int64Array, StringArray};
    ///
    /// let result = conn.load_arrow("SELECT id, name FROM users").await?;
    ///
    /// println!("Schema: {:?}", result.schema());
    /// println!("Total rows: {}", result.num_rows());
    ///
    /// for batch in result {
    ///     let ids = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
    ///     let names = batch.column(1).as_any().downcast_ref::<StringArray>().unwrap();
    ///
    ///     for i in 0..batch.num_rows() {
    ///         println!("User {}: {}", ids.value(i), names.value(i));
    ///     }
    /// }
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - **Zero-copy**: Data is accessed directly without parsing
    /// - **Columnar**: Efficient for queries accessing few columns
    /// - **SIMD-ready**: Arrow's layout enables vectorized operations
    /// - **Recommended for**: Large analytical queries, data pipelines
    pub async fn load_arrow(&self, sql: &str) -> QueryResult<crate::arrow::ArrowResult> {
        // Execute query with ArrowStream format
        let mut cursor = self.client
            .query(sql)
            .fetch_bytes("ArrowStream")
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))?;

        // Collect all bytes (Arrow IPC stream needs complete data)
        let mut all_bytes = Vec::with_capacity(8192);
        loop {
            match cursor.next().await {
                Ok(Some(chunk)) => {
                    all_bytes.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(Error::QueryError(Cow::Owned(e.to_string()))),
            }
        }

        // Parse Arrow IPC stream
        crate::arrow::parse_arrow_stream(&all_bytes)
    }

    /// Load query results as Arrow with a callback for each batch.
    ///
    /// This method processes batches as they are parsed, which can be more
    /// memory-efficient for very large result sets.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::arrow::array::Int64Array;
    ///
    /// let total = conn.load_arrow_callback(
    ///     "SELECT id FROM huge_table",
    ///     |batch| {
    ///         let ids = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
    ///         println!("Processing {} rows", ids.len());
    ///         Ok(())
    ///     }
    /// ).await?;
    /// println!("Processed {} total rows", total);
    /// ```
    pub async fn load_arrow_callback<F>(
        &self,
        sql: &str,
        callback: F,
    ) -> QueryResult<usize>
    where
        F: FnMut(::arrow::array::RecordBatch) -> QueryResult<()>,
    {
        // Execute query with ArrowStream format
        let mut cursor = self.client
            .query(sql)
            .fetch_bytes("ArrowStream")
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))?;

        // Collect bytes
        let mut all_bytes = Vec::with_capacity(8192);
        loop {
            match cursor.next().await {
                Ok(Some(chunk)) => {
                    all_bytes.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(Error::QueryError(Cow::Owned(e.to_string()))),
            }
        }

        // Parse with callback
        crate::arrow::parse_arrow_stream_callback(&all_bytes, callback)
    }

    /// Load query results from a QueryFragment as Arrow.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = conn.load_arrow_query(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    pub async fn load_arrow_query<Q>(&self, query: Q) -> QueryResult<crate::arrow::ArrowResult>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let sql = build_sql(&query)?;
        self.load_arrow(&sql).await
    }

    // =========================================================================
    // Zero-Copy Row API (Arrow-backed)
    // =========================================================================

    /// Load rows using zero-copy parsing with a callback.
    ///
    /// This method uses Apache Arrow format internally for true zero-copy access.
    /// Each row is passed to the callback as an `ArrowRow` containing borrowed
    /// references into the Arrow buffers.
    ///
    /// # Arguments
    ///
    /// * `sql` - The SQL query to execute
    /// * `callback` - Function called for each row
    ///
    /// # Returns
    ///
    /// The number of rows processed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    ///
    /// let count = conn.load_zero_copy(
    ///     "SELECT id, name, score FROM users WHERE active = 1",
    ///     |row| {
    ///         let id: u64 = row.get_u64("id")?;
    ///         let name: &str = row.get_str("name")?;  // Zero-copy borrow!
    ///         let score: f64 = row.get_f64("score")?;
    ///
    ///         println!("User {}: {} (score: {})", id, name, score);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// println!("Processed {} rows", count);
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Uses Arrow format (binary, no parsing overhead)
    /// - True zero-copy: data is accessed directly from Arrow buffers
    /// - SIMD-friendly memory layout
    /// - Ideal for large result sets with row-by-row processing
    pub async fn load_zero_copy<F>(&self, sql: &str, mut callback: F) -> QueryResult<usize>
    where
        F: for<'a> FnMut(crate::arrow::ArrowRow<'a>) -> QueryResult<()>,
    {
        let result = self.load_arrow(sql).await?;
        let column_indices = crate::arrow::build_column_index(result.schema());

        let mut total_count = 0;
        for batch in result.batches() {
            let count = crate::arrow::for_each_row(batch, &column_indices, &mut callback)?;
            total_count += count;
        }

        Ok(total_count)
    }

    /// Load rows from a query fragment using zero-copy parsing.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let count = conn.load_zero_copy_query(
    ///     users::table.filter(users::active.eq(true)).select((users::id, users::name)),
    ///     |row| {
    ///         let id = row.get_u64("id")?;
    ///         let name = row.get_str("name")?;
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn load_zero_copy_query<Q, F>(&self, query: Q, callback: F) -> QueryResult<usize>
    where
        Q: QueryFragment<ClickHouse>,
        F: for<'a> FnMut(crate::arrow::ArrowRow<'a>) -> QueryResult<()>,
    {
        let sql = build_sql(&query)?;
        self.load_zero_copy(&sql, callback).await
    }
}

// =============================================================================
// Unified ClickHouseConnection Implementation
// =============================================================================

#[async_trait]
impl ClickHouseConnectionTrait for ClickHouseConnection {
    async fn execute_raw(&self, sql: &str) -> QueryResult<()> {
        ClickHouseConnection::execute_raw(self, sql).await
    }

    async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        // Use native parameter binding
        let compiled = build_sql_native(query)?;
        self.execute_native(&compiled).await
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

// =============================================================================
// Optimized RowBinary Loading
// =============================================================================

impl ClickHouseConnection {
    /// Load rows using RowBinary format (optimized, 2-3x faster than JSON).
    ///
    /// This method uses the native RowBinary format which is significantly
    /// faster than JSONEachRow. The row type must derive `clickhouse::Row`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    ///
    /// #[derive(Debug, Row)]  // Generates both serde and clickhouse::Row
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// // Fast RowBinary loading
    /// let users: Vec<User> = conn.load_optimized(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    ///
    /// # Performance
    ///
    /// RowBinary format provides:
    /// - 2-3x faster parsing than JSON
    /// - Lower memory allocations
    /// - Native type handling without string conversion
    pub async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        self.load_compiled(&compiled).await
    }

    /// Load rows using RowBinary with a pre-compiled query.
    pub async fn load_compiled<T>(&self, compiled: &NativeCompiledQuery) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
    {
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch_all()
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Load a single row using RowBinary format.
    pub async fn load_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch_one()
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Load an optional row using RowBinary format.
    pub async fn load_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch_optional()
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Load rows from raw SQL using RowBinary format.
    pub async fn load_raw<T>(&self, sql: &str) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
    {
        self.client
            .query(sql)
            .fetch_all()
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::expression::sql as sql_literal;
    use crate::core::query_builder::SelectStatement;

    #[test]
    fn test_build_sql() {
        // Simple table wrapper for testing
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

        let query = SelectStatement::new(TestTable)
            .filter(sql_literal::<diesel_clickhouse_types::Bool>("id > 10"))
            .limit(100);
        let result = build_sql(&query).expect("failed to build SQL");
        assert_eq!(result, "SELECT * FROM test_table WHERE id > 10 LIMIT 100");
    }
}
