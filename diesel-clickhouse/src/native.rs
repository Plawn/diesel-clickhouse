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
//! use diesel_clickhouse::native::NativeConnection;
//!
//! // Plain TCP connection (port 9000)
//! let conn = NativeConnection::establish("tcp://localhost:9000/default").await?;
//!
//! // With authentication
//! let conn = NativeConnection::establish("tcp://user:pass@localhost:9000/mydb").await?;
//!
//! // With TLS (requires native-tls-native feature)
//! let conn = NativeConnection::establish(
//!     "tcp://user:pass@localhost:9440/mydb?secure=true"
//! ).await?;
//!
//! // Execute queries
//! conn.execute_raw("CREATE TABLE test (id UInt64) ENGINE = Memory").await?;
//!
//! // Insert data
//! conn.insert_raw("test", vec![1u64, 2, 3]).await?;
//!
//! // Query data
//! let block = conn.query_raw("SELECT * FROM test").await?;
//! ```

use async_trait::async_trait;
use clickhouse_rs::{Pool, ClientHandle, Block, types::Complex};

use crate::core::backend::{ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::core::connection::AsyncConnection;
use crate::core::deserialize::FromRow;
use crate::core::query_builder::{AstPass, QueryFragment};
use crate::core::result::{Error, QueryResult};

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
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.insert_values("users", "(1, 'Alice'), (2, 'Bob')").await?;
    /// ```
    pub async fn insert_values(&self, table: &str, values_sql: &str) -> QueryResult<()> {
        let sql = format!("INSERT INTO {} VALUES {}", table, values_sql);
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
pub fn build_sql<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> String {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass).unwrap();
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
