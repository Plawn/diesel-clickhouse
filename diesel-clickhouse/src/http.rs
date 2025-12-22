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
            Error::ConnectionError("host is required".to_string()))?;
        let port = self.port.ok_or_else(||
            Error::ConnectionError("port is required".to_string()))?;
        let database = self.database.ok_or_else(||
            Error::ConnectionError("database is required".to_string()))?;
        let user = self.user.ok_or_else(||
            Error::ConnectionError("user is required".to_string()))?;
        let password = self.password.ok_or_else(||
            Error::ConnectionError("password is required".to_string()))?;

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
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

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
    /// Create a new connection from a URL.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = ClickHouseConnection::establish("http://localhost:8123/my_database").await?;
    /// ```
    pub async fn new(url: &str) -> QueryResult<Self> {
        let parsed = url::Url::parse(url)
            .map_err(|e| Error::ConnectionError(format!("Invalid URL: {}", e)))?;

        let path = parsed.path().trim_start_matches('/');
        let database = if path.is_empty() {
            "default".to_owned()
        } else {
            path.to_owned()
        };

        // Build URL efficiently without nested format! calls
        let base_url = match parsed.port() {
            Some(port) => {
                let mut url = String::with_capacity(64);
                url.push_str(parsed.scheme());
                url.push_str("://");
                url.push_str(parsed.host_str().unwrap_or("localhost"));
                url.push(':');
                let mut buf = itoa::Buffer::new();
                url.push_str(buf.format(port));
                url
            }
            None => {
                let mut url = String::with_capacity(64);
                url.push_str(parsed.scheme());
                url.push_str("://");
                url.push_str(parsed.host_str().unwrap_or("localhost"));
                url
            }
        };

        let mut client = Client::default()
            .with_url(&base_url)
            .with_database(&database);

        // Extract user/password from URL if present
        let username = parsed.username();
        if !username.is_empty() {
            client = client.with_user(username);
        }
        if let Some(password) = parsed.password() {
            client = client.with_password(password);
        }

        // Test connection
        client
            .query("SELECT 1")
            .execute()
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        Ok(Self { client, database, compression: Compression::None })
    }

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
            .map_err(|e| Error::QueryError(e.to_string()))
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
            .map_err(|e| Error::QueryError(e.to_string()))
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
            .map_err(|e| Error::QueryError(e.to_string()))
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
            .map_err(|e| Error::QueryError(e.to_string()))
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
        query.execute().await.map_err(|e| Error::QueryError(e.to_string()))
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
            .map_err(|e| Error::QueryError(e.to_string()))
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
            .map_err(|e| Error::QueryError(e.to_string()))
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
            .map_err(|e| Error::QueryError(e.to_string()))
    }
}

// =============================================================================
// Zero-Copy API
// =============================================================================

impl ClickHouseConnection {
    /// Load rows using zero-copy parsing with a callback.
    ///
    /// This method uses ClickHouse's TabSeparated format and processes rows
    /// without allocating owned data structures. Each row is passed to the
    /// callback as a `ZeroCopyRow` containing borrowed references into the
    /// response buffer.
    ///
    /// # Arguments
    ///
    /// * `sql` - The SQL query to execute
    /// * `columns` - Column names in the order they appear in the SELECT clause
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
    /// // Process rows without allocating String/Vec for each row
    /// let count = conn.load_zero_copy(
    ///     "SELECT id, name, score FROM users WHERE active = 1",
    ///     &["id", "name", "score"],
    ///     |row| {
    ///         let id: u64 = row.get_u64("id")?;
    ///         let name: &str = row.get_str("name")?;  // Borrowed!
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
    /// - Uses TabSeparated format (faster to parse than JSON)
    /// - No allocations per row (values are borrowed from response buffer)
    /// - Ideal for large result sets with simple data access patterns
    /// - For complex nested types (Array, Map), consider using `load_json()` instead
    pub async fn load_zero_copy<F>(
        &self,
        sql: &str,
        columns: &[&str],
        callback: F,
    ) -> QueryResult<usize>
    where
        F: for<'a, 'b> FnMut(crate::zero_copy::ZeroCopyRow<'a, 'b>) -> QueryResult<()>,
    {
        // Execute query with TabSeparated format (no header)
        let mut cursor = self.client
            .query(sql)
            .fetch_bytes("TabSeparated")
            .map_err(|e| Error::QueryError(e.to_string()))?;

        // Collect all bytes (we need the full buffer for zero-copy)
        let mut all_bytes = Vec::with_capacity(4096);
        loop {
            match cursor.next().await {
                Ok(Some(chunk)) => {
                    all_bytes.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(Error::QueryError(e.to_string())),
            }
        }

        // Parse using zero-copy TSV parser
        let parser = crate::zero_copy::TsvParser::new(&all_bytes, columns);
        parser.for_each(callback)
    }

    /// Load rows using zero-copy streaming parsing with a callback.
    ///
    /// Unlike `load_zero_copy`, this method processes rows as chunks arrive
    /// from the network, which can reduce memory usage for very large result sets.
    /// However, rows that span chunk boundaries require buffering.
    ///
    /// # Arguments
    ///
    /// * `sql` - The SQL query to execute
    /// * `columns` - Column names in the order they appear in the SELECT clause
    /// * `callback` - Function called for each row
    ///
    /// # Returns
    ///
    /// The number of rows processed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Stream-process a very large result set
    /// let count = conn.load_zero_copy_streaming(
    ///     "SELECT * FROM huge_table",
    ///     &["id", "data"],
    ///     |row| {
    ///         // Process each row as it arrives
    ///         let id = row.get_u64("id")?;
    ///         // ...
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn load_zero_copy_streaming<F>(
        &self,
        sql: &str,
        columns: &[&str],
        mut callback: F,
    ) -> QueryResult<usize>
    where
        F: for<'a, 'b> FnMut(crate::zero_copy::ZeroCopyRow<'a, 'b>) -> QueryResult<()>,
    {
        let mut cursor = self.client
            .query(sql)
            .fetch_bytes("TabSeparated")
            .map_err(|e| Error::QueryError(e.to_string()))?;

        let mut parser = crate::zero_copy::StreamingTsvParser::new(columns);
        let mut total_count = 0;

        loop {
            match cursor.next().await {
                Ok(Some(chunk)) => {
                    let count = parser.process_chunk(&chunk, &mut callback)?;
                    total_count += count;
                }
                Ok(None) => {
                    // Process any remaining data
                    let count = parser.finish(&mut callback)?;
                    total_count += count;
                    break;
                }
                Err(e) => return Err(Error::QueryError(e.to_string())),
            }
        }

        Ok(total_count)
    }

    /// Load rows from a query fragment using zero-copy parsing.
    ///
    /// This is a convenience method that builds SQL from a query fragment
    /// and then uses zero-copy parsing.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let count = conn.load_zero_copy_query(
    ///     users::table.filter(users::active.eq(true)).select((users::id, users::name)),
    ///     &["id", "name"],
    ///     |row| {
    ///         let id = row.get_u64("id")?;
    ///         let name = row.get_str("name")?;
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn load_zero_copy_query<Q, F>(
        &self,
        query: Q,
        columns: &[&str],
        callback: F,
    ) -> QueryResult<usize>
    where
        Q: QueryFragment<ClickHouse>,
        F: for<'a, 'b> FnMut(crate::zero_copy::ZeroCopyRow<'a, 'b>) -> QueryResult<()>,
    {
        let sql = build_sql(&query)?;
        self.load_zero_copy(&sql, columns, callback).await
    }
}

// =============================================================================
// Unified ClickHouseConnection Implementation
// =============================================================================

#[async_trait]
impl ClickHouseConnectionTrait for ClickHouseConnection {
    async fn establish(url: &str) -> QueryResult<Self> {
        Self::new(url).await
    }

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
    /// let users: Vec<User> = conn.load_binary(
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
    pub async fn load_binary<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        self.load_binary_native(&compiled).await
    }

    /// Load rows using RowBinary with a pre-compiled query.
    pub async fn load_binary_native<T>(&self, compiled: &NativeCompiledQuery) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
    {
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch_all()
            .await
            .map_err(|e| Error::QueryError(e.to_string()))
    }

    /// Load a single row using RowBinary format.
    pub async fn load_binary_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch_one()
            .await
            .map_err(|e| Error::QueryError(e.to_string()))
    }

    /// Load an optional row using RowBinary format.
    pub async fn load_binary_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch_optional()
            .await
            .map_err(|e| Error::QueryError(e.to_string()))
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
