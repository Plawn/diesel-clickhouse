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
use serde::de::DeserializeOwned;

use crate::core::backend::{ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::core::connection::{AsyncConnection, ClickHouseConnection as ClickHouseConnectionTrait};
use crate::core::deserialize::FromRow;
use crate::core::query_builder::{AstPass, QueryFragment};
use crate::core::result::{Error, QueryResult};
use crate::core::row::ClickHouseRow as ClickHouseRowTrait;

// =============================================================================
// SQL Escaping Utilities
// =============================================================================

/// Escape an identifier for use in SQL (table names, column names).
/// Wraps in backticks and escapes any backticks within.
#[inline]
fn escape_identifier(s: &str) -> String {
    if s.contains('`') {
        format!("`{}`", s.replace('`', "``"))
    } else {
        format!("`{}`", s)
    }
}

// Re-export clickhouse-rs types for convenience
pub use clickhouse_rs::{Block as NativeBlock, row, types};

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
        let sql = build_sql(query);
        self.execute_raw(&sql).await
    }

    /// Build SQL from a query fragment without executing.
    pub fn build_query<Q>(&self, query: &Q) -> String
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
        let sql = build_sql(&query);
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

#[async_trait]
impl AsyncConnection for NativeConnection {
    type Backend = ClickHouse;

    async fn establish(url: &str) -> QueryResult<Self> {
        Self::establish(url).await
    }

    async fn execute(&mut self, sql: &str) -> QueryResult<()> {
        self.execute_raw(sql).await
    }

    async fn load<T, U>(&mut self, query: T) -> QueryResult<Vec<U>>
    where
        T: QueryFragment<Self::Backend> + Send,
        U: FromRow + Send,
    {
        let sql = build_sql(&query);
        Err(Error::QueryError(format!(
            "Use conn.query(q).await and process Block directly for: {}",
            sql
        )))
    }

    async fn execute_query<T>(&mut self, query: T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + Send,
    {
        let sql = build_sql(&query);
        self.execute_raw(&sql).await?;
        Ok(0)
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
/// # Panics
///
/// Panics if the query fragment fails to produce valid SQL. This should only
/// occur if there's a bug in the query builder implementation, as all valid
/// query fragments should produce valid SQL.
pub fn build_sql<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> String {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)
        .expect("QueryFragment::walk_ast failed - this indicates a bug in the query builder");
    builder.finish()
}

// =============================================================================
// Query Execution Extensions
// =============================================================================

/// Extension trait for query fragments to get SQL.
pub trait ToSql: QueryFragment<ClickHouse> {
    /// Convert to SQL string.
    fn to_sql_string(&self) -> String {
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
    /// Load rows from a raw SQL query using JSON format (for serde types).
    ///
    /// This method queries ClickHouse and converts the Block to JSON for
    /// serde-compatible deserialization.
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
        let block = self.query_raw(sql).await?;
        block_to_vec(&block)
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
        let sql = build_sql(query);
        self.execute_raw(&sql).await
    }

    async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: ClickHouseRowTrait,
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        let sql = build_sql(&query);
        self.load_json(&sql).await
    }

    fn build_sql<Q>(&self, query: &Q) -> String
    where
        Q: QueryFragment<ClickHouse>,
    {
        build_sql(query)
    }

    fn database(&self) -> &str {
        &self.database
    }
}

/// Lazily-built column index for a block.
///
/// This struct caches column metadata once per block, avoiding repeated
/// allocations and lookups when processing many rows.
struct BlockColumnIndex {
    /// Column names (shared across all rows via Rc).
    names: std::rc::Rc<[String]>,
    /// Column types for extraction.
    types: Vec<clickhouse_rs::types::SqlType>,
}

impl BlockColumnIndex {
    /// Build the column index from a block (one-time cost per block).
    fn from_block(block: &Block<Complex>) -> Self {
        let cols: Vec<_> = block.columns().iter()
            .map(|col| (col.name().to_string(), col.sql_type()))
            .collect();

        let (names, types): (Vec<_>, Vec<_>) = cols.into_iter().unzip();

        Self {
            names: names.into(),
            types,
        }
    }

    /// Get the number of columns.
    #[inline]
    fn len(&self) -> usize {
        self.names.len()
    }
}

/// Convert a native Block to a Vec of deserializable rows.
fn block_to_vec<T: DeserializeOwned>(
    block: &Block<Complex>,
) -> QueryResult<Vec<T>> {
    let row_count = block.row_count();

    // Pre-allocate results vector
    let mut results = Vec::with_capacity(row_count);

    // Build column index once for the entire block (lazy initialization)
    let index = BlockColumnIndex::from_block(block);
    let col_count = index.len();

    for row_idx in 0..row_count {
        let mut map = serde_json::Map::with_capacity(col_count);

        for col_idx in 0..col_count {
            let value = extract_block_value(block, row_idx, col_idx, &index.types[col_idx])?;
            // Clone from Rc is cheap (just ref count bump for the slice)
            map.insert(index.names[col_idx].clone(), value);
        }

        let row: T = serde_json::from_value(serde_json::Value::Object(map))
            .map_err(|e| Error::DeserializationError(e.to_string()))?;
        results.push(row);
    }

    Ok(results)
}

/// Extract a value from a native Block cell.
fn extract_block_value(
    block: &Block<Complex>,
    row: usize,
    col: usize,
    sql_type: &clickhouse_rs::types::SqlType,
) -> QueryResult<serde_json::Value> {
    use clickhouse_rs::types::SqlType;

    Ok(match sql_type {
        SqlType::UInt8 => {
            let v: u8 = block.get(row, col).map_err(ch_err)?;
            serde_json::Value::Number(v.into())
        }
        SqlType::UInt16 => {
            let v: u16 = block.get(row, col).map_err(ch_err)?;
            serde_json::Value::Number(v.into())
        }
        SqlType::UInt32 => {
            let v: u32 = block.get(row, col).map_err(ch_err)?;
            serde_json::Value::Number(v.into())
        }
        SqlType::UInt64 => {
            let v: u64 = block.get(row, col).map_err(ch_err)?;
            serde_json::Value::Number(v.into())
        }
        SqlType::Int8 => {
            let v: i8 = block.get(row, col).map_err(ch_err)?;
            serde_json::Value::Number(v.into())
        }
        SqlType::Int16 => {
            let v: i16 = block.get(row, col).map_err(ch_err)?;
            serde_json::Value::Number(v.into())
        }
        SqlType::Int32 => {
            let v: i32 = block.get(row, col).map_err(ch_err)?;
            serde_json::Value::Number(v.into())
        }
        SqlType::Int64 => {
            let v: i64 = block.get(row, col).map_err(ch_err)?;
            serde_json::Value::Number(v.into())
        }
        SqlType::Float32 => {
            let v: f32 = block.get(row, col).map_err(ch_err)?;
            serde_json::Number::from_f64(v as f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        SqlType::Float64 => {
            let v: f64 = block.get(row, col).map_err(ch_err)?;
            serde_json::Number::from_f64(v)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        SqlType::String | SqlType::FixedString(_) => {
            let v: String = block.get(row, col).map_err(ch_err)?;
            serde_json::Value::String(v)
        }
        SqlType::Date => {
            let days: u16 = block.get(row, col).map_err(ch_err)?;
            let date = days_to_date_string(days as i32);
            serde_json::Value::String(date)
        }
        SqlType::DateTime(_) => {
            let secs: u32 = block.get(row, col).map_err(ch_err)?;
            let datetime = secs_to_datetime_string(secs as i64);
            serde_json::Value::String(datetime)
        }
        SqlType::Nullable(inner) => {
            match inner {
                SqlType::String | SqlType::FixedString(_) => {
                    block.get::<Option<String>, _>(row, col)
                        .map_err(ch_err)?
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null)
                }
                SqlType::UInt64 => {
                    block.get::<Option<u64>, _>(row, col)
                        .map_err(ch_err)?
                        .map(|v| serde_json::Value::Number(v.into()))
                        .unwrap_or(serde_json::Value::Null)
                }
                SqlType::Int64 => {
                    block.get::<Option<i64>, _>(row, col)
                        .map_err(ch_err)?
                        .map(|v| serde_json::Value::Number(v.into()))
                        .unwrap_or(serde_json::Value::Null)
                }
                _ => {
                    block.get::<Option<String>, _>(row, col)
                        .unwrap_or(None)
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null)
                }
            }
        }
        _ => {
            let v: String = block.get(row, col).unwrap_or_default();
            serde_json::Value::String(v)
        }
    })
}

/// Convert clickhouse-rs errors to our error type.
fn ch_err(e: clickhouse_rs::errors::Error) -> Error {
    Error::QueryError(e.to_string())
}

/// Convert days since epoch to ISO date string.
/// Uses stack-based formatting to avoid heap allocations.
fn days_to_date_string(days: i32) -> String {
    const DAYS_IN_400_YEARS: i32 = 146097;

    let days = days + 719468;

    let era = if days >= 0 { days } else { days - 146096 } / DAYS_IN_400_YEARS;
    let doe = days - era * DAYS_IN_400_YEARS;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    // Pre-allocate exact size: "YYYY-MM-DD" = 10 chars
    let mut result = String::with_capacity(10);
    write_padded_i32(&mut result, y, 4);
    result.push('-');
    write_padded_i32(&mut result, m, 2);
    result.push('-');
    write_padded_i32(&mut result, d, 2);
    result
}

/// Convert seconds since epoch to ISO datetime string.
/// Uses stack-based formatting to avoid heap allocations.
fn secs_to_datetime_string(secs: i64) -> String {
    let days = (secs / 86400) as i32;
    let day_secs = (secs % 86400) as u32;
    let hours = day_secs / 3600;
    let mins = (day_secs % 3600) / 60;
    let secs_val = day_secs % 60;

    // Pre-allocate exact size: "YYYY-MM-DDTHH:MM:SSZ" = 20 chars
    let mut result = String::with_capacity(20);

    // Date part
    const DAYS_IN_400_YEARS: i32 = 146097;
    let days_adj = days + 719468;
    let era = if days_adj >= 0 { days_adj } else { days_adj - 146096 } / DAYS_IN_400_YEARS;
    let doe = days_adj - era * DAYS_IN_400_YEARS;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    write_padded_i32(&mut result, y, 4);
    result.push('-');
    write_padded_i32(&mut result, m, 2);
    result.push('-');
    write_padded_i32(&mut result, d, 2);
    result.push('T');
    write_padded_u32(&mut result, hours, 2);
    result.push(':');
    write_padded_u32(&mut result, mins, 2);
    result.push(':');
    write_padded_u32(&mut result, secs_val, 2);
    result.push('Z');
    result
}

/// Write a zero-padded i32 to a string.
#[inline]
fn write_padded_i32(s: &mut String, value: i32, width: usize) {
    let mut buf = itoa::Buffer::new();
    let formatted = buf.format(value);
    for _ in 0..(width.saturating_sub(formatted.len())) {
        s.push('0');
    }
    s.push_str(formatted);
}

/// Write a zero-padded u32 to a string.
#[inline]
fn write_padded_u32(s: &mut String, value: u32, width: usize) {
    let mut buf = itoa::Buffer::new();
    let formatted = buf.format(value);
    for _ in 0..(width.saturating_sub(formatted.len())) {
        s.push('0');
    }
    s.push_str(formatted);
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
        let result = build_sql(&query);
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
