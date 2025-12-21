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
use serde::de::DeserializeOwned;

// =============================================================================
// JSON Parsing (with optional SIMD acceleration)
// =============================================================================

/// Parse JSON from a string slice.
/// Uses simd-json when the feature is enabled for faster parsing.
#[cfg(feature = "simd-json")]
#[inline]
fn parse_json_str<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    // simd-json requires a mutable buffer
    let mut bytes = s.as_bytes().to_vec();
    simd_json::from_slice(&mut bytes)
        .map_err(|e| format!("Failed to parse JSON: {}", e))
}

#[cfg(not(feature = "simd-json"))]
#[inline]
fn parse_json_str<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_json::from_str(s)
        .map_err(|e| format!("Failed to parse JSON: {}", e))
}

/// Parse JSON from a byte slice.
/// Uses simd-json when the feature is enabled for faster parsing.
#[cfg(feature = "simd-json")]
#[inline]
fn parse_json_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    // simd-json requires a mutable buffer
    let mut bytes = bytes.to_vec();
    simd_json::from_slice(&mut bytes)
        .map_err(|e| format!("Failed to parse JSON: {}", e))
}

#[cfg(not(feature = "simd-json"))]
#[inline]
fn parse_json_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes)
        .map_err(|e| format!("Failed to parse JSON: {}", e))
}

use crate::core::backend::{ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::core::connection::{AsyncConnection, ClickHouseConnection as ClickHouseConnectionTrait};
use crate::core::deserialize::FromRow;
use crate::core::escape::escape_identifier;
use crate::core::query_builder::{AstPass, QueryFragment};
use crate::core::result::{Error, QueryResult};
use crate::core::row::ClickHouseRow as ClickHouseRowTrait;

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

        let client = Client::default()
            .with_url(&base_url)
            .with_database(&database);

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
}

#[async_trait]
impl AsyncConnection for ClickHouseConnection {
    type Backend = ClickHouse;

    async fn establish(url: &str) -> QueryResult<Self> {
        Self::new(url).await
    }

    async fn execute(&mut self, sql: &str) -> QueryResult<()> {
        self.execute_raw(sql).await
    }

    async fn load<T, U>(&mut self, query: T) -> QueryResult<Vec<U>>
    where
        T: QueryFragment<Self::Backend> + Send,
        U: FromRow + Send,
    {
        let sql = build_sql(&query)?;
        // Direct load through FromRow is not supported - use client().query() instead
        // This is because clickhouse crate requires specific Row derive
        Err(Error::QueryError(format!(
            "Use conn.client().query(\"{}\").fetch_all::<YourRowType>().await instead",
            sql
        )))
    }

    async fn execute_query<T>(&mut self, query: T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + Send,
    {
        let sql = build_sql(&query)?;
        self.execute_raw(&sql).await?;
        Ok(0) // ClickHouse doesn't easily return affected rows
    }
}

// =============================================================================
// SQL Building
// =============================================================================

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

    /// Load rows from a raw SQL query using JSON format (for serde types).
    ///
    /// This method uses ClickHouse's JSONEachRow format to deserialize results
    /// into any type that implements `serde::Deserialize`.
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
    /// let users: Vec<User> = conn.load_json("SELECT id, name FROM users").await?;
    /// ```
    pub async fn load_json<T: DeserializeOwned + Send>(&self, sql: &str) -> QueryResult<Vec<T>> {
        // Execute query and get bytes cursor with JSONEachRow format
        let mut cursor = self.client
            .query(sql)
            .fetch_bytes("JSONEachRow")
            .map_err(|e| Error::QueryError(e.to_string()))?;

        // Pre-allocate with reasonable initial capacity to reduce reallocations
        // 4KB is a good starting point for typical query results
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

        // Parse JSONEachRow format: one JSON object per line
        let text = String::from_utf8(all_bytes)
            .map_err(|e| Error::DeserializationError(e.to_string()))?;

        // Estimate row count based on newlines for pre-allocation
        let estimated_rows = text.bytes().filter(|&b| b == b'\n').count().max(1);
        let mut results = Vec::with_capacity(estimated_rows);

        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let row: T = parse_json_str(line)
                .map_err(|e| Error::DeserializationError(format!("{} - line: {}", e, line)))?;
            results.push(row);
        }

        Ok(results)
    }

    /// Load rows from a raw SQL query using streaming JSON parsing.
    ///
    /// This method parses JSON rows as they arrive, reducing memory usage
    /// for large result sets. Each row is passed to the callback function.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.load_json_streaming("SELECT * FROM large_table", |user: User| {
    ///     println!("Got user: {:?}", user);
    ///     Ok(()) // Return Err to stop iteration early
    /// }).await?;
    /// ```
    pub async fn load_json_streaming<T, F>(
        &self,
        sql: &str,
        mut callback: F,
    ) -> QueryResult<usize>
    where
        T: DeserializeOwned + Send,
        F: FnMut(T) -> QueryResult<()> + Send,
    {
        let mut cursor = self.client
            .query(sql)
            .fetch_bytes("JSONEachRow")
            .map_err(|e| Error::QueryError(e.to_string()))?;

        let mut count = 0;
        let mut buffer = Vec::with_capacity(4096);
        let mut line_start = 0;

        loop {
            match cursor.next().await {
                Ok(Some(chunk)) => {
                    buffer.extend_from_slice(&chunk);

                    // Process complete lines in the buffer
                    while let Some(newline_pos) = buffer[line_start..].iter().position(|&b| b == b'\n') {
                        let line_end = line_start + newline_pos;
                        let line = &buffer[line_start..line_end];

                        if !line.is_empty() && !line.iter().all(|&b| b == b' ' || b == b'\t' || b == b'\r') {
                            // Parse the line as JSON
                            let row: T = parse_json_slice(line)
                                .map_err(|e| Error::DeserializationError(format!(
                                    "{} - line: {}",
                                    e,
                                    String::from_utf8_lossy(line)
                                )))?;

                            callback(row)?;
                            count += 1;
                        }

                        line_start = line_end + 1;
                    }

                    // Keep only the incomplete line in the buffer
                    if line_start > 0 {
                        buffer.drain(..line_start);
                        line_start = 0;
                    }
                }
                Ok(None) => {
                    // Process any remaining data in buffer
                    if !buffer.is_empty() {
                        let line = buffer.trim_ascii();
                        if !line.is_empty() {
                            let row: T = parse_json_slice(line)
                                .map_err(|e| Error::DeserializationError(format!(
                                    "{} - line: {}",
                                    e,
                                    String::from_utf8_lossy(line)
                                )))?;
                            callback(row)?;
                            count += 1;
                        }
                    }
                    break;
                }
                Err(e) => return Err(Error::QueryError(e.to_string())),
            }
        }

        Ok(count)
    }

    /// Load rows into a Vec using streaming parsing.
    ///
    /// This is like `load_json` but uses streaming parsing internally,
    /// which can be more memory-efficient for very large result sets.
    pub async fn load_json_streamed<T: DeserializeOwned + Send>(&self, sql: &str) -> QueryResult<Vec<T>> {
        // Estimate initial capacity from a quick scan
        let mut results = Vec::with_capacity(1024);

        self.load_json_streaming(sql, |row| {
            results.push(row);
            Ok(())
        }).await?;

        Ok(results)
    }
}

/// Trait extension for trimming ASCII whitespace from byte slices.
trait TrimAscii {
    fn trim_ascii(&self) -> &[u8];
}

impl TrimAscii for [u8] {
    fn trim_ascii(&self) -> &[u8] {
        let start = self.iter().position(|&b| !b.is_ascii_whitespace()).unwrap_or(self.len());
        let end = self.iter().rposition(|&b| !b.is_ascii_whitespace()).map(|i| i + 1).unwrap_or(0);
        if start < end {
            &self[start..end]
        } else {
            &[]
        }
    }
}

impl TrimAscii for Vec<u8> {
    fn trim_ascii(&self) -> &[u8] {
        self.as_slice().trim_ascii()
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
        let sql = build_sql(query)?;
        self.execute_raw(&sql).await
    }

    async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: ClickHouseRowTrait,
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        let sql = build_sql(&query)?;
        self.load_json(&sql).await
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
// Migration Support
// =============================================================================

#[cfg(feature = "migrations")]
mod migration_impl {
    use super::*;
    use diesel_clickhouse_migrations::{MigrationConnection, MigrationError, Result as MigrationResult};

    #[async_trait]
    impl MigrationConnection for ClickHouseConnection {
        async fn execute(&mut self, sql: &str) -> MigrationResult<()> {
            self.execute_raw(sql).await.map_err(|e| MigrationError::SqlError {
                migration: "".to_string(),
                message: e.to_string(),
            })
        }

        async fn query_exists(&mut self, sql: &str) -> MigrationResult<bool> {
            let result: Option<u8> = self.client.query(sql).fetch_optional().await
                .map_err(|e| MigrationError::SqlError {
                    migration: "".to_string(),
                    message: e.to_string(),
                })?;
            Ok(result.is_some())
        }

        async fn query_scalar_string(&mut self, sql: &str) -> MigrationResult<Option<String>> {
            let result: Option<String> = self.client.query(sql).fetch_optional().await
                .map_err(|e| MigrationError::SqlError {
                    migration: "".to_string(),
                    message: e.to_string(),
                })?;
            Ok(result)
        }

        async fn query_versions(&mut self, sql: &str) -> MigrationResult<Vec<String>> {
            let versions: Vec<String> = self.client.query(sql).fetch_all().await
                .map_err(|e| MigrationError::SqlError {
                    migration: "".to_string(),
                    message: e.to_string(),
                })?;
            Ok(versions)
        }

        fn database_name(&self) -> &str {
            &self.database
        }
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
