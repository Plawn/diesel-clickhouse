//! Async connection traits for ClickHouse.
//!
//! This module provides the core connection abstraction for diesel-clickhouse:
//!
//! - [`ClickHouseConnection`] - The main unified connection trait that works with both backends
//!
//! # Usage
//!
//! The recommended way to use connections is through the [`ClickHouseConnection`] trait
//! and the unified `Connection` type:
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
//! async fn get_users(conn: &Connection) -> QueryResult<Vec<User>> {
//!     conn.load(users::table.filter(users::active.eq(true))).await
//! }
//! ```

use crate::backend::ClickHouse;
use crate::query_builder::QueryFragment;
use crate::result::QueryResult;

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
// Connection Settings
// =============================================================================

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

/// Compression algorithm for ClickHouse connections.
///
/// This is the unified compression enum used by both HTTP and Native backends.
/// Note that not all compression modes are supported by all backends:
///
/// | Mode | HTTP Backend | Native Backend |
/// |------|--------------|----------------|
/// | `None` | ✓ | ✓ |
/// | `Lz4` | ✓ | ✓ |
/// | `Lz4Hc` | ✓ (deprecated in clickhouse crate) | ✗ (falls back to Lz4) |
/// | `Zstd` | ✗ (not supported) | ✗ (not supported) |
///
/// When an unsupported compression mode is used, backends will either fall back
/// to the closest supported mode or return an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    /// No compression.
    #[default]
    None,
    /// LZ4 compression (recommended for most use cases).
    Lz4,
    /// LZ4 high compression (higher ratio, slower).
    /// Note: Deprecated in HTTP backend, falls back to Lz4 in Native backend.
    Lz4Hc,
    /// Zstandard compression.
    /// Note: Not currently supported by either backend.
    Zstd,
}
