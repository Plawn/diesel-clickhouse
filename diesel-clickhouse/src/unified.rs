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
//!     // INSERT
//!     let new_user = NewUser { id: 1, name: "Alice".into(), active: true };
//!     conn.insert(insert_into(users::table).values(&new_user)).await?;
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

use std::borrow::Cow;

use crate::core::backend::ClickHouse;
use crate::core::query_builder::QueryFragment;
use crate::core::result::{Error, QueryResult};

// =============================================================================
// Macros to reduce cfg duplication for load methods
// =============================================================================

/// Generates load methods with proper cfg attributes for each backend combination.
/// This macro eliminates the need to write 3 versions of each method manually.
macro_rules! impl_load_methods {
    // Pattern for methods that delegate to self.load()
    (
        $(#[$meta:meta])*
        delegate fn $name:ident(&$self:ident, $query:ident) -> $ret:ty $body:block
    ) => {
        $(#[$meta])*
        #[cfg(all(feature = "http", not(feature = "native")))]
        pub async fn $name<T, Q>(&$self, $query: Q) -> QueryResult<$ret>
        where
            T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
            Q: QueryFragment<ClickHouse>,
        $body

        #[cfg(all(feature = "native", not(feature = "http")))]
        pub async fn $name<T, Q>(&$self, $query: Q) -> QueryResult<$ret>
        where
            T: crate::native::FromNativeBlock + Send,
            Q: QueryFragment<ClickHouse> + Send,
        $body

        #[cfg(all(feature = "http", feature = "native"))]
        pub async fn $name<T, Q>(&$self, $query: Q) -> QueryResult<$ret>
        where
            T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send,
            Q: QueryFragment<ClickHouse> + Send,
        $body
    };

    // Pattern for raw SQL methods that delegate to backend-specific methods
    (
        $(#[$meta:meta])*
        raw fn $name:ident(&$self:ident, $sql:ident: &str) -> $ret:ty {
            http($http_conn:ident) => $http_body:expr,
            native($native_conn:ident) => $native_body:expr $(,)?
        }
    ) => {
        $(#[$meta])*
        #[cfg(all(feature = "http", not(feature = "native")))]
        pub async fn $name<T>(&$self, $sql: &str) -> QueryResult<$ret>
        where
            T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        {
            match $self {
                Connection::Http($http_conn) => $http_body,
            }
        }

        #[cfg(all(feature = "native", not(feature = "http")))]
        pub async fn $name<T>(&$self, $sql: &str) -> QueryResult<$ret>
        where
            T: crate::native::FromNativeBlock + Send,
        {
            match $self {
                Connection::Native($native_conn) => $native_body,
            }
        }

        #[cfg(all(feature = "http", feature = "native"))]
        pub async fn $name<T>(&$self, $sql: &str) -> QueryResult<$ret>
        where
            T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send,
        {
            match $self {
                Connection::Http($http_conn) => $http_body,
                Connection::Native($native_conn) => $native_body,
            }
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
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => conn.database(),
            #[cfg(feature = "native")]
            Connection::Native(conn) => conn.database(),
        }
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
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => conn.execute_raw(sql).await,
            #[cfg(feature = "native")]
            Connection::Native(conn) => conn.execute_raw(sql).await,
        }
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
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => conn.execute_statement(&query).await,
            #[cfg(feature = "native")]
            Connection::Native(conn) => conn.execute_statement(&query).await,
        }
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
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => conn.build_query(&query),
            #[cfg(feature = "native")]
            Connection::Native(conn) => conn.build_query(&query),
        }
    }

    /// Insert data using raw SQL VALUES.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.insert_values("users", "(1, 'Alice'), (2, 'Bob')").await?;
    /// ```
    pub async fn insert_values(&self, table: &str, values_sql: &str) -> QueryResult<()> {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => conn.insert_raw(table, values_sql).await,
            #[cfg(feature = "native")]
            Connection::Native(conn) => conn.insert_values(table, values_sql).await,
        }
    }

    /// Insert using a query fragment (INSERT statement).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.insert(
    ///     insert_into(users::table).values(&new_user)
    /// ).await?;
    ///
    /// // Batch insert
    /// conn.insert(
    ///     insert_into(users::table).values(users.as_slice())
    /// ).await?;
    /// ```
    pub async fn insert<Q>(&self, query: Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse>,
    {
        self.execute_query(query).await
    }

    /// Insert multiple rows efficiently in a single statement.
    ///
    /// This is a convenience method that builds and executes an INSERT
    /// statement for multiple rows in one network round-trip.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    ///
    /// let users = vec![
    ///     NewUser { id: 1, name: "Alice".into() },
    ///     NewUser { id: 2, name: "Bob".into() },
    ///     NewUser { id: 3, name: "Charlie".into() },
    /// ];
    ///
    /// conn.insert_batch(users::table, &users).await?;
    /// ```
    pub async fn insert_batch<T, R>(&self, table: T, rows: &[R]) -> QueryResult<()>
    where
        T: crate::core::query_source::Table,
        R: crate::core::query_builder::Insertable<T>,
    {
        use crate::core::query_builder::insert_into;

        if rows.is_empty() {
            return Ok(());
        }

        // Use the existing insert mechanism with slice
        let stmt = insert_into(table).values(rows);
        self.execute_query(stmt).await
    }

    // =========================================================================
    // Unified Load Method - Optimized binary deserialization
    // =========================================================================

    /// Load rows from a query using optimized binary deserialization.
    ///
    /// This is the recommended way to fetch data. The row type must be marked
    /// with `#[row]` attribute, which generates optimal deserialization for
    /// each backend:
    ///
    /// - **HTTP**: Uses RowBinary format (2-3x faster than JSON)
    /// - **Native**: Uses direct Block deserialization (no JSON intermediate)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    ///
    /// #[row]
    /// #[derive(Debug)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// // Works optimally with both HTTP and Native connections!
    /// let users: Vec<User> = conn.load(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        match self {
            Connection::Http(conn) => conn.load(query).await,
        }
    }

    /// Load rows from a query using optimized binary deserialization.
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        match self {
            Connection::Native(conn) => conn.load(query).await,
        }
    }

    /// Load rows from a query using optimized binary deserialization.
    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        match self {
            Connection::Http(conn) => conn.load(query).await,
            Connection::Native(conn) => conn.load(query).await,
        }
    }

    impl_load_methods! {
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
        delegate fn load_one(&self, query) -> T {
            self.load(query).await?.into_iter().next().ok_or(Error::NotFound)
        }
    }

    impl_load_methods! {
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
        delegate fn load_optional(&self, query) -> Option<T> {
            Ok(self.load(query).await?.into_iter().next())
        }
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

    /// Stream rows from a query.
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
    /// #[derive(Debug, Row)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// // Stream results row by row (works with both backends!)
    /// let mut stream = conn.stream::<User, _>(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    ///
    /// while let Some(user) = stream.next().await? {
    ///     println!("User: {} - {}", user.id, user.name);
    /// }
    /// ```
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn stream<T, Q>(&self, query: Q) -> QueryResult<crate::stream::RowStream<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        match self {
            Connection::Http(conn) => {
                let cursor = conn.stream(query)?;
                Ok(crate::stream::RowStream::Http(cursor))
            }
        }
    }

    /// Stream rows from a query.
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn stream<T, Q>(&self, query: Q) -> QueryResult<crate::stream::RowStream<T>>
    where
        T: crate::native::FromAnyBlock + Send + 'static,
        Q: QueryFragment<ClickHouse> + Send,
    {
        match self {
            Connection::Native(conn) => {
                let stream = conn.stream(query)?;
                Ok(crate::stream::RowStream::from(stream))
            }
        }
    }

    /// Stream rows from a query.
    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn stream<T, Q>(&self, query: Q) -> QueryResult<crate::stream::RowStream<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromAnyBlock + Send + 'static,
        Q: QueryFragment<ClickHouse> + Send,
    {
        match self {
            Connection::Http(conn) => {
                let cursor = conn.stream(query)?;
                Ok(crate::stream::RowStream::Http(cursor))
            }
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
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn stream_for_each<T, Q, F>(&self, query: Q, mut callback: F) -> QueryResult<()>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
        F: FnMut(T) -> QueryResult<()>,
    {
        match self {
            Connection::Http(conn) => {
                let mut cursor: clickhouse::query::RowCursor<T> = conn.stream(query)?;
                while let Some(row) = cursor.next().await.map_err(Error::query_from)? {
                    callback(row)?;
                }
                Ok(())
            }
        }
    }

    /// Stream rows from a query with a callback.
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn stream_for_each<T, Q, F>(&self, query: Q, callback: F) -> QueryResult<()>
    where
        T: crate::native::FromAnyBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> QueryResult<()>,
    {
        match self {
            Connection::Native(conn) => conn.stream_for_each(query, callback).await,
        }
    }

    /// Stream rows from a query with a callback.
    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn stream_for_each<T, Q, F>(&self, query: Q, mut callback: F) -> QueryResult<()>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromAnyBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> QueryResult<()>,
    {
        match self {
            Connection::Http(conn) => {
                let mut cursor: clickhouse::query::RowCursor<T> = conn.stream(query)?;
                while let Some(row) = cursor.next().await.map_err(Error::query_from)? {
                    callback(row)?;
                }
                Ok(())
            }
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
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn stream_for_each_async<T, Q, F, Fut>(&self, query: Q, mut callback: F) -> QueryResult<()>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = QueryResult<()>>,
    {
        match self {
            Connection::Http(conn) => {
                let mut cursor: clickhouse::query::RowCursor<T> = conn.stream(query)?;
                while let Some(row) = cursor.next().await.map_err(Error::query_from)? {
                    callback(row).await?;
                }
                Ok(())
            }
        }
    }

    /// Stream rows from a query with an async callback.
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn stream_for_each_async<T, Q, F, Fut>(&self, query: Q, callback: F) -> QueryResult<()>
    where
        T: crate::native::FromAnyBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = QueryResult<()>>,
    {
        match self {
            Connection::Native(conn) => conn.stream_for_each_async(query, callback).await,
        }
    }

    /// Stream rows from a query with an async callback.
    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn stream_for_each_async<T, Q, F, Fut>(&self, query: Q, mut callback: F) -> QueryResult<()>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromAnyBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = QueryResult<()>>,
    {
        match self {
            Connection::Http(conn) => {
                let mut cursor: clickhouse::query::RowCursor<T> = conn.stream(query)?;
                while let Some(row) = cursor.next().await.map_err(Error::query_from)? {
                    callback(row).await?;
                }
                Ok(())
            }
            Connection::Native(conn) => conn.stream_for_each_async(query, callback).await,
        }
    }

    impl_load_methods! {
        /// Load all rows from a raw SQL string.
        raw fn load_raw(&self, sql: &str) -> Vec<T> {
            http(conn) => conn.load_raw(sql).await,
            native(conn) => conn.load_raw(sql).await,
        }
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
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => conn.load_zero_copy(sql, callback).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                Cow::Borrowed("Zero-copy parsing is not supported for Native backend. Use HTTP backend instead.")
            )),
        }
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
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => conn.load_zero_copy_query(query, callback).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                Cow::Borrowed("Zero-copy parsing is not supported for Native backend. Use HTTP backend instead.")
            )),
        }
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
        match self {
            Connection::Http(conn) => conn.load_arrow(sql).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                Cow::Borrowed("Arrow format is only supported on HTTP backend.")
            )),
        }
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
        match self {
            Connection::Http(conn) => conn.load_arrow_callback(sql, callback).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                Cow::Borrowed("Arrow format is only supported on HTTP backend.")
            )),
        }
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
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => conn.load_arrow_query(query).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                Cow::Borrowed("Arrow format is only supported on HTTP backend. Use load_zero_copy() for native backend.")
            )),
        }
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

    #[async_trait]
    impl MigrationConnection for Connection {
        async fn execute(&mut self, sql: &str) -> MigrationResult<()> {
            Connection::execute(self, sql).await.map_err(|e| MigrationError::SqlError {
                migration: "".to_string(),
                message: e.to_string(),
            })
        }

        async fn query_exists(&mut self, sql: &str) -> MigrationResult<bool> {
            match self {
                #[cfg(feature = "http")]
                Connection::Http(conn) => {
                    let result: Option<u8> = conn.client().query(sql).fetch_optional().await
                        .map_err(|e| MigrationError::SqlError {
                            migration: "".to_string(),
                            message: e.to_string(),
                        })?;
                    Ok(result.is_some())
                }
                #[cfg(feature = "native")]
                Connection::Native(conn) => {
                    let block = conn.query_raw(sql).await.map_err(|e| MigrationError::SqlError {
                        migration: "".to_string(),
                        message: e.to_string(),
                    })?;
                    Ok(block.row_count() > 0)
                }
            }
        }

        async fn query_scalar_string(&mut self, sql: &str) -> MigrationResult<Option<String>> {
            match self {
                #[cfg(feature = "http")]
                Connection::Http(conn) => {
                    let result: Option<String> = conn.client().query(sql).fetch_optional().await
                        .map_err(|e| MigrationError::SqlError {
                            migration: "".to_string(),
                            message: e.to_string(),
                        })?;
                    Ok(result)
                }
                #[cfg(feature = "native")]
                Connection::Native(conn) => {
                    let block = conn.query_raw(sql).await.map_err(|e| MigrationError::SqlError {
                        migration: "".to_string(),
                        message: e.to_string(),
                    })?;

                    if block.row_count() == 0 {
                        return Ok(None);
                    }

                    let value: String = block.get(0, 0).map_err(|e| MigrationError::SqlError {
                        migration: "".to_string(),
                        message: format!("Failed to get scalar string: {}", e),
                    })?;

                    Ok(Some(value))
                }
            }
        }

        async fn query_versions(&mut self, sql: &str) -> MigrationResult<Vec<String>> {
            match self {
                #[cfg(feature = "http")]
                Connection::Http(conn) => {
                    let versions: Vec<String> = conn.client().query(sql).fetch_all().await
                        .map_err(|e| MigrationError::SqlError {
                            migration: "".to_string(),
                            message: e.to_string(),
                        })?;
                    Ok(versions)
                }
                #[cfg(feature = "native")]
                Connection::Native(conn) => {
                    let block = conn.query_raw(sql).await.map_err(|e| MigrationError::SqlError {
                        migration: "".to_string(),
                        message: e.to_string(),
                    })?;

                    let mut versions = Vec::with_capacity(block.row_count());
                    for row_idx in 0..block.row_count() {
                        let version: String = block.get(row_idx, 0).map_err(|e| MigrationError::SqlError {
                            migration: "".to_string(),
                            message: format!("Failed to get version at row {}: {}", row_idx, e),
                        })?;
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
