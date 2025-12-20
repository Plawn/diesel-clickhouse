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
//! use clickhouse::Row;
//! use serde::{Serialize, Deserialize};
//!
//! // Define your row type with clickhouse's Row derive
//! #[derive(Row, Serialize, Deserialize)]
//! struct MyRow {
//!     id: u64,
//!     name: String,
//! }
//!
//! // Use the connection
//! let conn = ClickHouseConnection::establish("http://localhost:8123/mydb").await?;
//! let rows: Vec<MyRow> = conn.fetch("SELECT id, name FROM my_table").await?;
//! ```

use async_trait::async_trait;
use clickhouse::Client;
use serde::{Deserialize, Serialize};

use crate::core::backend::{ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::core::connection::AsyncConnection;
use crate::core::deserialize::FromRow;
use crate::core::query_builder::{AstPass, QueryFragment};
use crate::core::result::{Error, QueryResult};

// Re-export clickhouse Row for convenience
pub use clickhouse::Row as ClickHouseRow;

/// A connection to ClickHouse via HTTP.
#[derive(Clone)]
pub struct ClickHouseConnection {
    client: Client,
    database: String,
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

        let database = parsed.path().trim_start_matches('/').to_string();
        let database = if database.is_empty() {
            "default".to_string()
        } else {
            database
        };

        let base_url = format!(
            "{}://{}{}",
            parsed.scheme(),
            parsed.host_str().unwrap_or("localhost"),
            parsed.port().map(|p| format!(":{}", p)).unwrap_or_default()
        );

        let client = Client::default()
            .with_url(&base_url)
            .with_database(&database);

        // Test connection
        client
            .query("SELECT 1")
            .execute()
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        Ok(Self { client, database })
    }

    /// Create a connection from an existing Client.
    pub fn from_client(client: Client, database: impl Into<String>) -> Self {
        Self {
            client,
            database: database.into(),
        }
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
        let sql = build_sql(&query);
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
        let sql = build_sql(&query);
        self.execute_raw(&sql).await?;
        Ok(0) // ClickHouse doesn't easily return affected rows
    }
}

// =============================================================================
// SQL Building
// =============================================================================

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
    /// Execute the query.
    async fn execute(self, conn: &ClickHouseConnection) -> QueryResult<()> {
        conn.execute_statement(&self).await
    }
}

impl<T: QueryFragment<ClickHouse> + Send + Sync> ExecuteMut for T {}

// =============================================================================
// Fluent Query Execution Macros
// =============================================================================

/// Load all results from a query.
///
/// This macro provides a convenient way to execute a SELECT query and load all results.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::{load, prelude::*};
///
/// let users: Vec<User> = load!(
///     users::table.filter(users::active.eq(true)),
///     &conn
/// )?;
/// ```
#[macro_export]
macro_rules! load {
    ($query:expr, $conn:expr) => {{
        let sql = $crate::http::ToSql::to_sql_string(&$query);
        $conn
            .client()
            .query(&sql)
            .fetch_all()
            .await
            .map_err(|e| $crate::Error::QueryError(e.to_string()))
    }};
}

/// Load the first result from a query.
///
/// # Example
///
/// ```rust,ignore
/// let user: User = first!(
///     users::table.filter(users::id.eq(1)),
///     &conn
/// )?;
/// ```
#[macro_export]
macro_rules! first {
    ($query:expr, $conn:expr) => {{
        let sql = $crate::http::ToSql::to_sql_string(&$query);
        $conn
            .client()
            .query(&sql)
            .fetch_one()
            .await
            .map_err(|e| $crate::Error::QueryError(e.to_string()))
    }};
}

/// Load an optional result from a query.
///
/// # Example
///
/// ```rust,ignore
/// let user: Option<User> = get_optional!(
///     users::table.filter(users::id.eq(1)),
///     &conn
/// )?;
/// ```
#[macro_export]
macro_rules! get_optional {
    ($query:expr, $conn:expr) => {{
        let sql = $crate::http::ToSql::to_sql_string(&$query);
        $conn
            .client()
            .query(&sql)
            .fetch_optional()
            .await
            .map_err(|e| $crate::Error::QueryError(e.to_string()))
    }};
}

/// Execute a query without returning results.
///
/// Useful for UPDATE/DELETE statements.
///
/// # Example
///
/// ```rust,ignore
/// execute!(
///     diesel_clickhouse::update(users::table)
///         .filter(users::id.eq(1))
///         .set(users::name.eq("New Name")),
///     &conn
/// )?;
/// ```
#[macro_export]
macro_rules! execute {
    ($query:expr, $conn:expr) => {{
        let sql = $crate::http::ToSql::to_sql_string(&$query);
        $conn.execute_raw(&sql).await
    }};
}

/// Insert rows into a table.
///
/// # Example
///
/// ```rust,ignore
/// // Using the table from schema with explicit row type (required)
/// let count = insert!(NewUser => users::table, &new_users, &conn)?;
/// ```
#[macro_export]
macro_rules! insert {
    // Version with explicit row type and table from schema
    ($row_type:ty => $table:expr, $rows:expr, $conn:expr) => {{
        async {
            use $crate::Table;
            let rows: &[$row_type] = $rows;
            if rows.is_empty() {
                return Ok::<_, $crate::Error>(0usize);
            }

            // Helper function to get table name from type
            fn get_table_name<T: Table>(_: &T) -> &'static str {
                T::table_name()
            }
            let table_name = get_table_name(&$table);

            let mut insert = $conn
                .client()
                .insert::<$row_type>(table_name)
                .await
                .map_err(|e| $crate::Error::QueryError(e.to_string()))?;

            for row in rows {
                insert
                    .write(row)
                    .await
                    .map_err(|e| $crate::Error::QueryError(e.to_string()))?;
            }

            insert
                .end()
                .await
                .map_err(|e| $crate::Error::QueryError(e.to_string()))?;

            Ok(rows.len())
        }
        .await
    }};
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
        let result = build_sql(&query);
        assert_eq!(result, "SELECT * FROM test_table");

        let query = SelectStatement::new(TestTable)
            .filter(sql_literal::<diesel_clickhouse_types::Bool>("id > 10"))
            .limit(100);
        let result = build_sql(&query);
        assert_eq!(result, "SELECT * FROM test_table WHERE id > 10 LIMIT 100");
    }
}
