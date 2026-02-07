//! # Unified Connection Interface
//!
//! The `Connection` enum provides a unified API for both HTTP and Native backends.
//! The backend is selected automatically based on the URL scheme.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//! use diesel_clickhouse::Connection;
//!
//! // 1. Define table schema
//! diesel_clickhouse::table! {
//!     users (id, created_at) {
//!         id -> UInt64,
//!         name -> CHString,
//!         active -> Bool,
//!         created_at -> DateTime,
//!     }
//! }
//!
//! // 2. Define row types with unified #[derive(Row)]
//! #[derive(Debug, Row)]
//! struct User {
//!     id: u64,
//!     name: String,
//!     active: bool,
//! }
//!
//! #[derive(Row, Insertable)]
//! #[diesel_clickhouse(table = users)]
//! struct NewUser {
//!     id: u64,
//!     name: String,
//!     active: bool,
//! }
//!
//! // 3. Use the connection - same API for both HTTP and Native!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // HTTP or Native - your choice!
//!     let conn = Connection::establish("http://localhost:8123/default").await?;
//!     // Or: Connection::establish("tcp://localhost:9000/default").await?;
//!
//!     // DDL
//!     conn.execute("CREATE TABLE IF NOT EXISTS users ...").await?;
//!
//!     // INSERT (HTTP backend - uses optimized RowBinary format)
//!     let users = vec![
//!         NewUser { id: 1, name: "Alice".into(), active: true },
//!         NewUser { id: 2, name: "Bob".into(), active: false },
//!     ];
//!     conn.insert_rows("users", &users).await?;
//!
//!     // INSERT (Native backend - uses optimized Block format)
//!     // conn.insert_native("users", &users).await?;
//!
//!     // QUERY - unified with #[derive(Row)]
//!     let users: Vec<User> = conn.load(
//!         users::table.filter(users::active.eq(true))
//!     ).await?;
//!
//!     // STREAMING - memory-efficient for large datasets
//!     conn.stream_for_each(
//!         users::table.filter(users::active.eq(true)),
//!         |user: User| {
//!             println!("User: {}", user.name);
//!             Ok(())
//!         }
//!     ).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## URL Schemes
//!
//! | Scheme | Backend | Default Port | Example |
//! |--------|---------|--------------|---------|
//! | `http://` | HTTP | 8123 | `http://localhost:8123/mydb` |
//! | `https://` | HTTP | 8443 | `https://ch.example.com/mydb` |
//! | `tcp://` | Native | 9000 | `tcp://localhost:9000/mydb` |
//!
//! ## Backend-Specific Access
//!
//! For advanced use cases, access the underlying connection:
//!
//! ```rust,ignore
//! // HTTP: Access clickhouse crate's Client directly
//! if let Some(http_conn) = conn.as_http() {
//!     let mut inserter = http_conn.client().inserter::<NewUser>("users")?;
//!     inserter.write(&user)?;
//!     inserter.end().await?;
//! }
//!
//! // Native: Access clickhouse-rs Block API
//! if let Some(native_conn) = conn.as_native() {
//!     let block = native_conn.query_raw("SELECT * FROM users").await?;
//! }
//! ```
//!
//! ## Streaming
//!
//! For large result sets, use streaming to process rows without loading everything
//! into memory. Both backends support true network streaming:
//!
//! - **HTTP**: Row-by-row streaming via cursor - O(1) memory
//! - **Native**: Block-by-block streaming via background task - O(block_size) memory
//!
//! ### Using `stream()` (returns a RowStream)
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//!
//! // Stream results row by row - true streaming for both backends!
//! let mut stream = conn.stream::<User, _>(
//!     users::table.filter(users::active.eq(true))
//! ).await?;
//!
//! while let Some(user) = stream.next().await? {
//!     println!("User: {} - {}", user.id, user.name);
//! }
//! ```
//!
//! ### Using `stream_for_each()` (callback-based)
//!
//! ```rust,ignore
//! // Process each row with a callback - true streaming for both backends
//! conn.stream_for_each(
//!     users::table.filter(users::active.eq(true)),
//!     |user: User| {
//!         println!("Processing: {}", user.name);
//!         Ok(())
//!     }
//! ).await?;
//!
//! // Async callback version
//! conn.stream_for_each_async(
//!     users::table.select((users::id, users::name)),
//!     |user: User| async move {
//!         process_user_async(user).await;
//!         Ok(())
//!     }
//! ).await?;
//! ```
//!
//! ### Streaming Comparison
//!
//! | Method | HTTP Backend | Native Backend | Memory |
//! |--------|--------------|----------------|--------|
//! | `load()` | All in memory | All in memory | O(n) |
//! | `stream()` | True streaming | True streaming | O(1) / O(block_size) |
//! | `stream_for_each()` | True streaming | True streaming | O(1) / O(block_size) |
//!
//! ## Backend API Differences
//!
//! While the unified `Connection` API covers most use cases, some methods are
//! backend-specific due to protocol differences:
//!
//! ### HTTP-Only Methods
//!
//! | Method | Description |
//! |--------|-------------|
//! | `load_compiled()` | Execute pre-compiled query with bindings |
//! | `load_arrow()` | Load results as Apache Arrow batches |
//! | `stream_arrow()` | Stream results as Arrow batches |
//! | `load_zero_copy()` | Zero-copy Arrow processing |
//! | `inserter()` | Get clickhouse crate's native inserter |
//!
//! ### Native-Only Methods
//!
//! | Method | Description |
//! |--------|-------------|
//! | `insert_native()` | Insert using optimized Block format |
//! | `query_raw()` | Get raw Block for manual processing |
//!
//! ### Compression Support
//!
//! Both backends use the unified `Compression` enum, but support differs:
//!
//! | Mode | HTTP | Native |
//! |------|------|--------|
//! | `None` | ✓ | ✓ |
//! | `Lz4` | ✓ | ✓ |
//! | `Lz4Hc` | → Lz4 | → Lz4 |
//! | `Zstd` | → None | → None |
//!
//! Unsupported modes fall back to the closest supported alternative.

use std::borrow::Cow;

use crate::core::backend::ClickHouse;
use crate::core::query_builder::{QueryFragment, QueryOutputType};
use crate::core::deserialize::Queryable;
use crate::core::result::{Error, QueryResult};

// =============================================================================
// Helper Macro for Connection Delegation
// =============================================================================

/// Macro to delegate method calls to the underlying connection variant.
///
/// This eliminates repetitive match statements when both HTTP and Native
/// backends use the same method signature.
///
/// # Usage
///
/// ```ignore
/// // Sync method delegation
/// with_connection!(self, |conn| conn.method())
///
/// // Async method delegation
/// with_connection!(self, |conn| conn.method().await)
/// ```
macro_rules! with_connection {
    ($self:expr, |$conn:ident| $expr:expr) => {
        match $self {
            #[cfg(feature = "http")]
            Connection::Http($conn) => $expr,
            #[cfg(feature = "native")]
            Connection::Native($conn) => $expr,
        }
    };
}

/// Macro for HTTP-only methods that return an error for Native backend.
#[cfg(feature = "arrow")]
macro_rules! http_only {
    ($self:expr, |$conn:ident| $expr:expr, $error_msg:expr) => {
        match $self {
            #[cfg(feature = "http")]
            Connection::Http($conn) => $expr,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(Cow::Borrowed($error_msg))),
        }
    };
}

/// A unified connection that works with both HTTP and Native backends.
///
/// The connection type is determined by the URL scheme:
/// - `http://` or `https://` → HTTP backend
/// - `tcp://` → Native backend
#[derive(Clone)]
pub enum Connection {
    /// HTTP backend connection
    #[cfg(feature = "http")]
    Http(crate::http::ClickHouseConnection),

    /// Native protocol connection
    #[cfg(feature = "native")]
    Native(crate::native::NativeConnection),
}

impl Connection {
    /// Create a new HTTP connection builder.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = Connection::http()
    ///     .host("localhost")
    ///     .port(8123)
    ///     .database("mydb")
    ///     .user("default")
    ///     .password("")
    ///     .build()
    ///     .await?;
    /// ```
    #[cfg(feature = "http")]
    pub fn http() -> crate::http::HttpClientBuilder {
        crate::http::HttpClientBuilder::new()
    }

    /// Create a new Native connection builder.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = Connection::native()
    ///     .host("localhost")
    ///     .port(9000)
    ///     .database("mydb")
    ///     .user("default")
    ///     .password("")
    ///     .pool_max(20)
    ///     .build()
    ///     .await?;
    /// ```
    #[cfg(feature = "native")]
    pub fn native() -> crate::native::NativeClientBuilder {
        crate::native::NativeClientBuilder::new()
    }

    /// Get the database name.
    pub fn database(&self) -> &str {
        with_connection!(self, |conn| conn.database())
    }

    /// Check if this is an HTTP connection.
    pub fn is_http(&self) -> bool {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(_) => true,
            #[cfg(feature = "native")]
            Connection::Native(_) => false,
        }
    }

    /// Check if this is a Native connection.
    pub fn is_native(&self) -> bool {
        !self.is_http()
    }

    /// Execute a raw SQL statement (no results).
    ///
    /// Use this for DDL statements (CREATE, ALTER, DROP) and mutations.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.execute("CREATE TABLE users (id UInt64) ENGINE = Memory").await?;
    /// conn.execute("ALTER TABLE users ADD COLUMN name String").await?;
    /// ```
    pub async fn execute(&self, sql: &str) -> QueryResult<()> {
        with_connection!(self, |conn| conn.execute_raw(sql).await)
    }

    /// Execute a query fragment.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.execute_query(
    ///     update(users::table)
    ///         .set(users::name.eq("New Name"))
    ///         .filter(users::id.eq(1))
    /// ).await?;
    /// ```
    pub async fn execute_query<Q>(&self, query: Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse>,
    {
        with_connection!(self, |conn| conn.execute_statement(&query).await)
    }

    /// Build SQL from a query fragment without executing.
    ///
    /// Useful for debugging or logging queries.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let sql = conn.build_sql(users::table.filter(users::active.eq(true)))?;
    /// println!("Query: {}", sql);
    /// ```
    pub fn build_sql<Q>(&self, query: Q) -> QueryResult<String>
    where
        Q: QueryFragment<ClickHouse>,
    {
        with_connection!(self, |conn| conn.build_query(&query))
    }

    // =========================================================================
    // Unified Load Method - Optimized binary deserialization
    // =========================================================================

    /// Load rows from a query with compile-time type verification.
    ///
    /// This method ensures at compile time that the struct type matches the
    /// query's output columns. Use `#[typed_row(table = xxx)]` to generate
    /// the required `Queryable` implementation.
    ///
    /// - **HTTP**: Uses RowBinary format (2-3x faster than JSON)
    /// - **Native**: Uses direct Block deserialization (no JSON intermediate)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    ///
    /// #[typed_row(table = users)]
    /// #[derive(Debug)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// // Compile-time verified: User matches users::table columns
    /// let users: Vec<User> = conn.load(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    ///
    /// // Compile error if User doesn't match the query!
    /// ```
    pub async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: Queryable<Q::SqlType> + crate::UnifiedRow,
        Q: QueryFragment<ClickHouse> + QueryOutputType + Send,
    {
        with_connection!(self, |conn| conn.load(query).await)
    }

    /// Load rows without compile-time type verification.
    ///
    /// **Deprecated**: Use `load()` with `#[typed_row(table = xxx)]` for type safety.
    ///
    /// This method is provided for backward compatibility during migration.
    /// The row type must be marked with `#[row]` attribute.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[row]
    /// struct User { id: u64, name: String }
    ///
    /// // No compile-time verification - runtime errors possible
    /// let users: Vec<User> = conn.load_unchecked(users::table).await?;
    /// ```
    #[deprecated(since = "0.2.0", note = "Use load() with #[typed_row(table = xxx)] for compile-time type safety")]
    pub async fn load_unchecked<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: crate::UnifiedRow,
        Q: QueryFragment<ClickHouse> + Send,
    {
        with_connection!(self, |conn| conn.load(query).await)
    }

    /// Load a single row from a query with compile-time type verification.
    ///
    /// Returns an error if no rows are found.
    ///
    /// This dispatches to backend-specific efficient implementations:
    /// - **HTTP**: Uses `fetch_one()` (server-side LIMIT)
    /// - **Native**: Inserts `LIMIT 1` into the SQL
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = conn.load_one(
    ///     users::table.filter(users::id.eq(42))
    /// ).await?;
    /// ```
    pub async fn load_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: Queryable<Q::SqlType> + crate::UnifiedRow,
        Q: QueryFragment<ClickHouse> + QueryOutputType + Send,
    {
        with_connection!(self, |conn| conn.load_one(query).await)
    }

    /// Load an optional single row from a query with compile-time type verification.
    ///
    /// Returns `None` if no rows are found.
    ///
    /// This dispatches to backend-specific efficient implementations:
    /// - **HTTP**: Uses `fetch_optional()` (server-side LIMIT)
    /// - **Native**: Inserts `LIMIT 1` into the SQL
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = conn.load_optional(
    ///     users::table.filter(users::id.eq(42))
    /// ).await?;
    /// ```
    pub async fn load_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: Queryable<Q::SqlType> + crate::UnifiedRow,
        Q: QueryFragment<ClickHouse> + QueryOutputType + Send,
    {
        with_connection!(self, |conn| conn.load_optional(query).await)
    }

    /// Get the underlying HTTP connection (if HTTP backend).
    #[cfg(feature = "http")]
    pub fn as_http(&self) -> Option<&crate::http::ClickHouseConnection> {
        match self {
            Connection::Http(conn) => Some(conn),
            #[cfg(feature = "native")]
            Connection::Native(_) => None,
        }
    }

    /// Get the underlying Native connection (if Native backend).
    #[cfg(feature = "native")]
    pub fn as_native(&self) -> Option<&crate::native::NativeConnection> {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(_) => None,
            Connection::Native(conn) => Some(conn),
        }
    }

    // =========================================================================
    // Streaming Methods
    // =========================================================================

    /// Stream rows from a query with compile-time type verification.
    ///
    /// Returns a `RowStream` that allows you to process results row by row.
    /// Works with both HTTP and Native backends with true streaming.
    ///
    /// - **HTTP**: True streaming (rows fetched incrementally from server) - O(1) memory
    /// - **Native**: True streaming via background task - O(block_size) memory
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    ///
    /// #[typed_row(table = users)]
    /// #[derive(Debug)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// // Compile-time verified streaming
    /// let mut stream = conn.stream::<User, _>(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    ///
    /// while let Some(user) = stream.next().await? {
    ///     println!("User: {} - {}", user.id, user.name);
    /// }
    /// ```
    pub async fn stream<T, Q>(&self, query: Q) -> QueryResult<crate::stream::RowStream<T>>
    where
        T: Queryable<Q::SqlType> + crate::StreamableRow,
        Q: QueryFragment<ClickHouse> + QueryOutputType + Send,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => {
                let cursor = conn.stream(query)?;
                Ok(crate::stream::RowStream::Http(cursor))
            }
            #[cfg(feature = "native")]
            Connection::Native(conn) => {
                let stream = conn.stream(query)?;
                Ok(crate::stream::RowStream::from(stream))
            }
        }
    }

    /// Stream rows without compile-time type verification.
    ///
    /// **Deprecated**: Use `stream()` with `#[typed_row(table = xxx)]` for type safety.
    #[deprecated(since = "0.2.0", note = "Use stream() with #[typed_row(table = xxx)] for compile-time type safety")]
    pub async fn stream_unchecked<T, Q>(&self, query: Q) -> QueryResult<crate::stream::RowStream<T>>
    where
        T: crate::StreamableRow,
        Q: QueryFragment<ClickHouse> + Send,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => {
                let cursor = conn.stream(query)?;
                Ok(crate::stream::RowStream::Http(cursor))
            }
            #[cfg(feature = "native")]
            Connection::Native(conn) => {
                let stream = conn.stream(query)?;
                Ok(crate::stream::RowStream::from(stream))
            }
        }
    }

    // =========================================================================
    // Streaming with Callback (true streaming for both backends)
    // =========================================================================

    /// Stream rows from a query with a callback.
    ///
    /// This method provides true streaming for both backends:
    /// - **HTTP**: Rows fetched incrementally via cursor
    /// - **Native**: Blocks fetched incrementally, rows processed per block
    ///
    /// Memory usage is O(block_size) instead of O(total_rows).
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
    pub async fn stream_for_each<T, Q, F>(&self, query: Q, mut callback: F) -> QueryResult<()>
    where
        T: crate::CallbackStreamableRow,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> QueryResult<()>,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => {
                let mut cursor: clickhouse::query::RowCursor<T> = conn.stream(query)?;
                while let Some(row) = cursor.next().await.map_err(Error::query_from)? {
                    callback(row)?;
                }
                Ok(())
            }
            #[cfg(feature = "native")]
            Connection::Native(conn) => conn.stream_for_each(query, callback).await,
        }
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
    pub async fn stream_for_each_async<T, Q, F, Fut>(&self, query: Q, mut callback: F) -> QueryResult<()>
    where
        T: crate::CallbackStreamableRow,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = QueryResult<()>>,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => {
                let mut cursor: clickhouse::query::RowCursor<T> = conn.stream(query)?;
                while let Some(row) = cursor.next().await.map_err(Error::query_from)? {
                    callback(row).await?;
                }
                Ok(())
            }
            #[cfg(feature = "native")]
            Connection::Native(conn) => conn.stream_for_each_async(query, callback).await,
        }
    }

    /// Load all rows from a raw SQL string.
    pub async fn load_raw<T>(&self, sql: &str) -> QueryResult<Vec<T>>
    where
        T: crate::UnifiedRow,
    {
        with_connection!(self, |conn| conn.load_raw(sql).await)
    }

    // =========================================================================
    // Zero-Copy Row API (Arrow-backed)
    // =========================================================================

    /// Load rows using zero-copy parsing with a callback (HTTP backend + Arrow feature).
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
    ///     "SELECT id, name, score FROM users",
    ///     |row| {
    ///         let id: u64 = row.get_u64("id")?;
    ///         let name: &str = row.get_str("name")?;  // Zero-copy borrow!
    ///         let score: f64 = row.get_f64("score")?;
    ///         println!("{}: {} ({})", id, name, score);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    ///
    /// # Backend Support
    ///
    /// - **HTTP + Arrow**: Full support with true zero-copy
    /// - **Native**: Not supported (returns error)
    #[cfg(feature = "arrow")]
    pub async fn load_zero_copy<F>(&self, sql: &str, callback: F) -> QueryResult<usize>
    where
        F: for<'a> FnMut(crate::arrow::ArrowRow<'a>) -> QueryResult<()>,
    {
        http_only!(
            self,
            |conn| conn.load_zero_copy(sql, callback).await,
            "Zero-copy parsing is not supported for Native backend. Use HTTP backend instead."
        )
    }

    /// Load rows from a query fragment using zero-copy parsing.
    ///
    /// Works with HTTP backend when Arrow feature is enabled.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let count = conn.load_zero_copy_query(
    ///     users::table.filter(users::active.eq(true)),
    ///     |row| {
    ///         let name = row.get_str("name")?;
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    #[cfg(feature = "arrow")]
    pub async fn load_zero_copy_query<Q, F>(&self, query: Q, callback: F) -> QueryResult<usize>
    where
        Q: QueryFragment<ClickHouse>,
        F: for<'a> FnMut(crate::arrow::ArrowRow<'a>) -> QueryResult<()>,
    {
        http_only!(
            self,
            |conn| conn.load_zero_copy_query(query, callback).await,
            "Zero-copy parsing is not supported for Native backend. Use HTTP backend instead."
        )
    }

    // =========================================================================
    // Apache Arrow API
    // =========================================================================

    /// Load query results as Apache Arrow RecordBatches (HTTP backend only).
    ///
    /// This method uses ClickHouse's ArrowStream format for true zero-copy
    /// columnar data access. Arrow is the most efficient format for analytical
    /// workloads and enables seamless interoperability with tools like Polars,
    /// DataFusion, and DuckDB.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    /// use diesel_clickhouse::arrow::array::{Int64Array, StringArray};
    ///
    /// let result = conn.load_arrow("SELECT id, name FROM users").await?;
    ///
    /// for batch in result {
    ///     let ids = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
    ///     for i in 0..batch.num_rows() {
    ///         println!("ID: {}", ids.value(i));
    ///     }
    /// }
    /// ```
    ///
    /// # Backend Support
    ///
    /// - **HTTP**: Full support with ArrowStream format
    /// - **Native**: Not supported (returns error)
    #[cfg(feature = "arrow")]
    pub async fn load_arrow(&self, sql: &str) -> QueryResult<crate::arrow::ArrowResult> {
        http_only!(
            self,
            |conn| conn.load_arrow(sql).await,
            "Arrow format is only supported on HTTP backend."
        )
    }

    /// Load query results as Arrow with a callback for each batch (HTTP backend only).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let total = conn.load_arrow_callback(
    ///     "SELECT * FROM huge_table",
    ///     |batch| {
    ///         println!("Processing {} rows", batch.num_rows());
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    #[cfg(feature = "arrow")]
    pub async fn load_arrow_callback<F>(
        &self,
        sql: &str,
        callback: F,
    ) -> QueryResult<usize>
    where
        F: FnMut(::arrow::array::RecordBatch) -> QueryResult<()> + Send + 'static,
    {
        http_only!(
            self,
            |conn| conn.load_arrow_callback(sql, callback).await,
            "Arrow format is only supported on HTTP backend."
        )
    }

    /// Load query results from a QueryFragment as Arrow (HTTP backend only).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = conn.load_arrow_query(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    #[cfg(feature = "arrow")]
    pub async fn load_arrow_query<Q>(&self, query: Q) -> QueryResult<crate::arrow::ArrowResult>
    where
        Q: QueryFragment<ClickHouse>,
    {
        http_only!(
            self,
            |conn| conn.load_arrow_query(query).await,
            "Arrow format is only supported on HTTP backend."
        )
    }
}

// =============================================================================
// Migration Support
// =============================================================================

#[cfg(feature = "migrations")]
mod migration_impl {
    use super::*;
    use async_trait::async_trait;
    use diesel_clickhouse_migrations::{MigrationConnection, MigrationError, Result as MigrationResult};

    /// Convert any error to a MigrationError::SqlError.
    #[inline]
    fn to_migration_err<E: std::fmt::Display>(e: E) -> MigrationError {
        MigrationError::SqlError {
            migration: String::new(),
            message: e.to_string(),
        }
    }

    /// Convert any error to a MigrationError::SqlError with a custom message prefix.
    #[inline]
    fn to_migration_err_with_context<E: std::fmt::Display>(context: &str, e: E) -> MigrationError {
        MigrationError::SqlError {
            migration: String::new(),
            message: format!("{}: {}", context, e),
        }
    }

    #[async_trait]
    impl MigrationConnection for Connection {
        async fn execute(&mut self, sql: &str) -> MigrationResult<()> {
            Connection::execute(self, sql).await.map_err(to_migration_err)
        }

        async fn query_exists(&mut self, sql: &str) -> MigrationResult<bool> {
            match self {
                #[cfg(feature = "http")]
                Connection::Http(conn) => {
                    let result: Option<u8> = conn.client().query(sql)
                        .fetch_optional().await
                        .map_err(to_migration_err)?;
                    Ok(result.is_some())
                }
                #[cfg(feature = "native")]
                Connection::Native(conn) => {
                    let block = conn.query_raw(sql).await.map_err(to_migration_err)?;
                    Ok(block.row_count() > 0)
                }
            }
        }

        async fn query_scalar_string(&mut self, sql: &str) -> MigrationResult<Option<String>> {
            match self {
                #[cfg(feature = "http")]
                Connection::Http(conn) => {
                    conn.client().query(sql)
                        .fetch_optional().await
                        .map_err(to_migration_err)
                }
                #[cfg(feature = "native")]
                Connection::Native(conn) => {
                    let block = conn.query_raw(sql).await.map_err(to_migration_err)?;

                    if block.row_count() == 0 {
                        return Ok(None);
                    }

                    let value: String = block.get(0, 0)
                        .map_err(|e| to_migration_err_with_context("Failed to get scalar string", e))?;
                    Ok(Some(value))
                }
            }
        }

        async fn query_versions(&mut self, sql: &str) -> MigrationResult<Vec<String>> {
            match self {
                #[cfg(feature = "http")]
                Connection::Http(conn) => {
                    conn.client().query(sql)
                        .fetch_all().await
                        .map_err(to_migration_err)
                }
                #[cfg(feature = "native")]
                Connection::Native(conn) => {
                    let block = conn.query_raw(sql).await.map_err(to_migration_err)?;

                    let mut versions = Vec::with_capacity(block.row_count());
                    for row_idx in 0..block.row_count() {
                        let version: String = block.get(row_idx, 0)
                            .map_err(|e| to_migration_err_with_context(
                                &format!("Failed to get version at row {}", row_idx), e
                            ))?;
                        versions.push(version);
                    }
                    Ok(versions)
                }
            }
        }

        fn database_name(&self) -> &str {
            self.database()
        }
    }
}
