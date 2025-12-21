//! Async connection traits for ClickHouse.
//!
//! This module provides the core connection abstraction for diesel-clickhouse:
//!
//! - [`ClickHouseConnection`] - The main unified connection trait that works with both backends
//! - [`AsyncConnection`] - Lower-level async connection trait (for internal use)
//!
//! # Usage
//!
//! The recommended way to use connections is through the [`ClickHouseConnection`] trait:
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//!
//! #[derive(Debug, Row)]
//! struct User {
//!     id: u64,
//!     name: String,
//! }
//!
//! async fn get_users(conn: &impl ClickHouseConnection) -> QueryResult<Vec<User>> {
//!     conn.load(users::table.filter(users::active.eq(true))).await
//! }
//! ```

use crate::backend::{Backend, ClickHouse};
use crate::deserialize::FromRow;
use crate::query_builder::QueryFragment;
use crate::result::QueryResult;
use crate::row::ClickHouseRow;

// =============================================================================
// Unified ClickHouse Connection Trait
// =============================================================================

/// Unified connection trait for ClickHouse that works with both HTTP and Native backends.
///
/// This is the main connection trait you should use in your application code.
/// It provides a consistent API regardless of which backend is being used.
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
///     active: bool,
/// }
///
/// async fn query_users(conn: &impl ClickHouseConnection) -> QueryResult<Vec<User>> {
///     // This works with both HTTP and Native connections
///     conn.load(
///         users::table
///             .filter(users::active.eq(true))
///             .order_by(users::name.asc())
///             .limit(100)
///     ).await
/// }
///
/// async fn insert_user(conn: &impl ClickHouseConnection, user: &NewUser) -> QueryResult<()> {
///     conn.insert(insert_into(users::table).values(user)).await
/// }
/// ```
///
/// # Backend Differences
///
/// While this trait provides a unified API, there are some behavioral differences:
///
/// | Feature | HTTP Backend | Native Backend |
/// |---------|--------------|----------------|
/// | Connection | HTTP/HTTPS | TCP binary protocol |
/// | Default Port | 8123 | 9000 (9440 for TLS) |
/// | Serialization | serde + clickhouse Row | serde only |
/// | Streaming | Via inserter | Via blocks |
#[async_trait::async_trait]
pub trait ClickHouseConnection: Send + Sync {
    /// Establish a connection from a URL.
    ///
    /// The URL format determines the backend:
    /// - `http://` or `https://` - HTTP backend
    /// - `tcp://` - Native backend
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // HTTP connection
    /// let conn = Connection::establish("http://localhost:8123/default").await?;
    ///
    /// // Native connection
    /// let conn = Connection::establish("tcp://localhost:9000/default").await?;
    /// ```
    async fn establish(url: &str) -> QueryResult<Self>
    where
        Self: Sized;

    /// Execute a raw SQL statement (no results).
    ///
    /// Use this for DDL statements (CREATE, ALTER, DROP) and other non-query operations.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.execute_raw("CREATE TABLE users (id UInt64, name String) ENGINE = MergeTree() ORDER BY id").await?;
    /// ```
    async fn execute_raw(&self, sql: &str) -> QueryResult<()>;

    /// Execute a query fragment (INSERT, UPDATE, DELETE).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.execute_statement(&update(users::table).set(users::active.eq(false)).filter(users::id.eq(1))).await?;
    /// ```
    async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse> + Send + Sync;

    /// Load rows from a query.
    ///
    /// This is the primary method for fetching data. The row type must implement
    /// `ClickHouseRow` (which is automatically satisfied by `#[derive(Row)]`).
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
    /// let users: Vec<User> = conn.load(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: ClickHouseRow,
        Q: QueryFragment<ClickHouse> + Send + Sync;

    /// Load a single row from a query.
    ///
    /// Returns an error if no rows are found.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = conn.load_one(
    ///     users::table.filter(users::id.eq(42))
    /// ).await?;
    /// ```
    async fn load_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: ClickHouseRow,
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        self.load(query).await?.into_iter().next().ok_or(crate::result::Error::NotFound)
    }

    /// Load an optional single row from a query.
    ///
    /// Returns `None` if no rows are found.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = conn.load_optional(
    ///     users::table.filter(users::id.eq(42))
    /// ).await?;
    /// ```
    async fn load_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: ClickHouseRow,
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        Ok(self.load(query).await?.into_iter().next())
    }

    /// Insert data using a query fragment.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.insert(insert_into(users::table).values(&new_user)).await?;
    /// ```
    async fn insert<Q>(&self, query: Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        self.execute_statement(&query).await
    }

    /// Build SQL from a query fragment without executing.
    ///
    /// Useful for debugging or logging queries.
    ///
    /// Returns an error if the query fragment fails to produce valid SQL.
    fn build_sql<Q>(&self, query: &Q) -> QueryResult<String>
    where
        Q: QueryFragment<ClickHouse>;

    /// Get the database name.
    fn database(&self) -> &str;

    /// Ping the connection to verify it's alive.
    async fn ping(&self) -> QueryResult<()> {
        self.execute_raw("SELECT 1").await
    }
}

// =============================================================================
// Legacy AsyncConnection Trait (for internal use)
// =============================================================================

/// Async connection trait for ClickHouse (legacy, for internal use).
///
/// Prefer using [`ClickHouseConnection`] in new code.
#[async_trait::async_trait]
pub trait AsyncConnection: Send + Sized {
    /// The backend type for this connection.
    type Backend: Backend;

    /// Establish a new connection.
    async fn establish(url: &str) -> QueryResult<Self>;

    /// Execute a raw SQL query.
    async fn execute(&mut self, sql: &str) -> QueryResult<()>;

    /// Execute a query and load results.
    async fn load<T, U>(&mut self, query: T) -> QueryResult<Vec<U>>
    where
        T: QueryFragment<Self::Backend> + Send,
        U: FromRow + Send;

    /// Execute a query and return affected row count.
    async fn execute_query<T>(&mut self, query: T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + Send;

    /// Begin a batch insert operation.
    fn batch_insert<T>(&mut self, table_name: &str) -> BatchInserter<'_, Self>
    where
        Self: Sized,
    {
        BatchInserter::new(table_name)
    }

    /// Ping the connection to verify it's alive.
    async fn ping(&mut self) -> QueryResult<()> {
        self.execute("SELECT 1").await
    }
}

/// A batch insert helper.
pub struct BatchInserter<'a, C> {
    table_name: String,
    _conn: std::marker::PhantomData<&'a C>,
}

impl<'a, C: AsyncConnection> BatchInserter<'a, C> {
    /// Create a new batch inserter.
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            _conn: std::marker::PhantomData,
        }
    }

    /// Get the table name.
    pub fn table_name(&self) -> &str {
        &self.table_name
    }
}

/// Settings for a connection.
#[derive(Debug, Clone)]
pub struct ConnectionSettings {
    /// Maximum number of retries for failed queries.
    pub max_retries: u32,
    /// Timeout for queries in seconds.
    pub query_timeout_secs: u64,
    /// Whether to enable query logging.
    pub log_queries: bool,
    /// Whether to use async insert mode.
    pub async_insert: bool,
    /// Whether to wait for async insert completion.
    pub wait_for_async_insert: bool,
    /// Compression algorithm.
    pub compression: Compression,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            max_retries: 3,
            query_timeout_secs: 300,
            log_queries: false,
            async_insert: false,
            wait_for_async_insert: true,
            compression: Compression::Lz4,
        }
    }
}

/// Compression algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// No compression.
    None,
    /// LZ4 compression (recommended).
    Lz4,
    /// LZ4 high compression.
    Lz4Hc,
    /// Zstandard compression.
    Zstd,
}

/// A pool of connections (planned feature).
#[allow(dead_code)]
struct ConnectionPool<C: AsyncConnection> {
    _conn: std::marker::PhantomData<C>,
}

/// Transaction handle placeholder.
///
/// # ⚠️ ClickHouse Transaction Limitations
///
/// **ClickHouse does NOT support traditional ACID transactions** like PostgreSQL/MySQL.
/// This struct exists for API compatibility but provides **no actual transactional guarantees**.
///
/// ## What ClickHouse Offers Instead
///
/// - **Atomicity per INSERT**: Each INSERT statement is atomic
/// - **ReplacingMergeTree**: For deduplication (use FINAL to get latest)
/// - **CollapsingMergeTree**: For implementing delete/update patterns
/// - **Lightweight deletes**: `ALTER TABLE DELETE` (async background operation)
///
/// ## Behavior of This Struct
///
/// - `commit()`: No-op, always succeeds
/// - `rollback()`: No-op, does nothing (data already written is NOT rolled back)
/// - `drop()`: No-op
///
/// ## Recommendations
///
/// For transactional semantics in ClickHouse:
/// 1. Design your schema to be append-only
/// 2. Use ReplacingMergeTree + FINAL for latest state
/// 3. Use batch inserts (atomic per batch)
/// 4. Consider external coordination (e.g., Redis locks) for complex workflows
#[deprecated(
    since = "0.2.0",
    note = "ClickHouse does not support transactions. This struct is a no-op placeholder. \
            Use batch inserts for atomicity or ReplacingMergeTree for deduplication."
)]
pub struct Transaction<'a, C: AsyncConnection> {
    conn: &'a mut C,
    committed: bool,
}

#[allow(deprecated)]
impl<'a, C: AsyncConnection> Transaction<'a, C> {
    /// Create a new transaction.
    ///
    /// # Note
    /// This does NOT start an actual database transaction.
    pub fn new(conn: &'a mut C) -> Self {
        Self {
            conn,
            committed: false,
        }
    }

    /// Commit the transaction.
    ///
    /// # Note
    /// This is a **no-op**. ClickHouse does not support transactions.
    /// Data written before calling this is already persisted.
    pub async fn commit(mut self) -> QueryResult<()> {
        self.committed = true;
        Ok(())
    }

    /// Rollback the transaction.
    ///
    /// # Note
    /// This is a **no-op**. ClickHouse does not support rollback.
    /// Data written is NOT reverted.
    pub async fn rollback(self) -> QueryResult<()> {
        // ClickHouse has no rollback capability
        Ok(())
    }

    /// Get the inner connection.
    pub fn conn(&mut self) -> &mut C {
        self.conn
    }
}

#[allow(deprecated)]
impl<'a, C: AsyncConnection> Drop for Transaction<'a, C> {
    fn drop(&mut self) {
        // No-op: ClickHouse has no transaction rollback
    }
}
