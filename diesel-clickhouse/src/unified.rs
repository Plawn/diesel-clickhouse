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
            Connection::Http(conn) => conn.load_binary(query).await,
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
            Connection::Native(conn) => conn.load_optimized(query).await,
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
            Connection::Http(conn) => conn.load_binary(query).await,
            Connection::Native(conn) => conn.load_optimized(query).await,
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

    // =========================================================================
    // Optimized RowBinary Loading (HTTP only) - kept for explicit access
    // =========================================================================

    /// Load rows using RowBinary format (2-3x faster than JSON).
    ///
    /// This method uses ClickHouse's native RowBinary format which is
    /// significantly faster than JSONEachRow. Only available for HTTP backend.
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
    /// // Fast RowBinary loading (HTTP only)
    /// let users: Vec<User> = conn.load_binary(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    ///
    /// # Performance
    ///
    /// - 2-3x faster parsing than JSON
    /// - Lower memory allocations
    /// - Native type handling
    ///
    /// # Backend Support
    ///
    /// - **HTTP**: Full support with RowBinary format
    /// - **Native**: Not supported (native backend uses its own binary protocol)
    #[cfg(feature = "http")]
    pub async fn load_binary<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        match self {
            Connection::Http(conn) => conn.load_binary(query).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                "load_binary() is only supported on HTTP backend. Native backend uses its own binary protocol via load().".to_string()
            )),
        }
    }

    /// Load a single row using RowBinary format.
    ///
    /// Returns an error if no rows are found.
    #[cfg(feature = "http")]
    pub async fn load_binary_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        match self {
            Connection::Http(conn) => conn.load_binary_one(query).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                "load_binary_one() is only supported on HTTP backend.".to_string()
            )),
        }
    }

    /// Load an optional row using RowBinary format.
    ///
    /// Returns `None` if no rows are found.
    #[cfg(feature = "http")]
    pub async fn load_binary_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        match self {
            Connection::Http(conn) => conn.load_binary_optional(query).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                "load_binary_optional() is only supported on HTTP backend.".to_string()
            )),
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
    // Optimized Native Loading (direct Block deserialization, no JSON)
    // =========================================================================

    /// Load rows using optimized direct Block deserialization (Native backend only).
    ///
    /// This method deserializes rows directly from the native Block without
    /// JSON intermediate conversion, providing 2-3x better performance than
    /// the standard `load()` method.
    ///
    /// Types must implement `FromNativeBlock`, which is automatically generated
    /// by `#[derive(Row)]`.
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
    /// // Optimized: direct Block → struct deserialization (Native backend)
    /// let users: Vec<User> = conn.load_optimized(users::table.select_all()).await?;
    /// ```
    ///
    /// # Backend Support
    ///
    /// - **Native**: Full support with direct Block deserialization
    /// - **HTTP**: Not supported (use `load_binary()` for HTTP optimization)
    #[cfg(feature = "native")]
    pub async fn load_optimized<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(_) => Err(Error::QueryError(
                "load_optimized() is only supported on Native backend. Use load_binary() for HTTP optimization.".to_string()
            )),
            Connection::Native(conn) => conn.load_optimized(query).await,
        }
    }

    /// Load a single row using optimized Native deserialization.
    ///
    /// Returns an error if no rows are found.
    #[cfg(feature = "native")]
    pub async fn load_optimized_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(_) => Err(Error::QueryError(
                "load_optimized_one() is only supported on Native backend.".to_string()
            )),
            Connection::Native(conn) => conn.load_optimized_one(query).await,
        }
    }

    /// Load an optional row using optimized Native deserialization.
    ///
    /// Returns `None` if no rows are found.
    #[cfg(feature = "native")]
    pub async fn load_optimized_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(_) => Err(Error::QueryError(
                "load_optimized_optional() is only supported on Native backend.".to_string()
            )),
            Connection::Native(conn) => conn.load_optimized_optional(query).await,
        }
    }

    // =========================================================================
    // Streaming Methods
    // =========================================================================

    /// Stream rows from a query.
    ///
    /// Returns a `RowStream` that allows you to process results row by row.
    /// Works with both HTTP and Native backends.
    ///
    /// - **HTTP**: True streaming (rows fetched incrementally from server)
    /// - **Native**: Block-based (all rows loaded, then iterated)
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
    #[cfg(feature = "http")]
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
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                "For Native backend, use stream_native() with #[derive(Row)] types.".to_string()
            )),
        }
    }

    /// Stream rows from a query (Native backend).
    ///
    /// For Native backend, the row type must implement `FromNativeBlock`.
    /// Note: Native backend loads all rows first, then iterates.
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
    /// let mut stream = conn.stream_native::<User, _>(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    ///
    /// while let Some(user) = stream.next().await? {
    ///     println!("User: {}", user.name);
    /// }
    /// ```
    #[cfg(feature = "native")]
    pub async fn stream_native<T, Q>(&self, query: Q) -> QueryResult<crate::stream::RowStream<T>>
    where
        T: crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        // Load all rows using optimized binary deserialization, then iterate
        let rows: Vec<T> = self.load_optimized(query).await?;
        Ok(crate::stream::RowStream::from(rows))
    }

    // =========================================================================
    // Fetch Methods (aliases for load methods)
    // =========================================================================

    impl_load_methods! {
        /// Fetch all rows from a query (alias for `load()`).
        delegate fn fetch_all(&self, query) -> Vec<T> {
            self.load(query).await
        }
    }

    impl_load_methods! {
        raw fn fetch_all_raw(&self, sql: &str) -> Vec<T> {
            http(conn) => conn.load_binary_raw(sql).await,
            native(conn) => conn.load_optimized_raw(sql).await,
        }
    }

    impl_load_methods! {
        /// Fetch exactly one row from a query (alias for `load_one()`).
        delegate fn fetch_one(&self, query) -> T {
            self.load_one(query).await
        }
    }

    impl_load_methods! {
        /// Fetch zero or one row from a query (alias for `load_optional()`).
        delegate fn fetch_optional(&self, query) -> Option<T> {
            self.load_optional(query).await
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
    /// - **Native + native-arrow**: Full support with true zero-copy streaming
    #[cfg(feature = "arrow")]
    pub async fn load_zero_copy<F>(&self, sql: &str, callback: F) -> QueryResult<usize>
    where
        F: for<'a> FnMut(crate::arrow::ArrowRow<'a>) -> QueryResult<()>,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http(conn) => conn.load_zero_copy(sql, callback).await,
            #[cfg(all(feature = "native", feature = "native-arrow"))]
            Connection::Native(conn) => conn.load_zero_copy(sql, callback).await,
            #[cfg(all(feature = "native", not(feature = "native-arrow")))]
            Connection::Native(_) => Err(Error::QueryError(
                "Zero-copy parsing requires 'native-arrow' feature for Native backend.".to_string()
            )),
        }
    }

    /// Load rows from a query fragment using zero-copy parsing.
    ///
    /// Works with both HTTP and Native backends when appropriate features are enabled.
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
            #[cfg(all(feature = "native", feature = "native-arrow"))]
            Connection::Native(conn) => conn.load_zero_copy_query(query, callback).await,
            #[cfg(all(feature = "native", not(feature = "native-arrow")))]
            Connection::Native(_) => Err(Error::QueryError(
                "Zero-copy parsing requires 'native-arrow' feature for Native backend.".to_string()
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
                "Arrow format is only supported on HTTP backend.".to_string()
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
        F: FnMut(::arrow::array::RecordBatch) -> QueryResult<()>,
    {
        match self {
            Connection::Http(conn) => conn.load_arrow_callback(sql, callback).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                "Arrow format is only supported on HTTP backend.".to_string()
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
                "Arrow format is only supported on HTTP backend. Use load_zero_copy() for native backend.".to_string()
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
