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

#[cfg(feature = "native")]
use serde::de::DeserializeOwned;

use crate::core::backend::ClickHouse;
use crate::core::query_builder::QueryFragment;
use crate::core::result::{Error, QueryResult};

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

    /// Establish a connection based on the URL scheme.
    ///
    /// # URL Formats
    ///
    /// - HTTP: `http://[user:pass@]host[:port]/database`
    /// - HTTPS: `https://[user:pass@]host[:port]/database`
    /// - Native: `tcp://[user:pass@]host[:port]/database[?options]`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // HTTP connection
    /// let conn = Connection::establish("http://localhost:8123/default").await?;
    ///
    /// // Native connection
    /// let conn = Connection::establish("tcp://localhost:9000/default").await?;
    ///
    /// // Native with TLS
    /// let conn = Connection::establish("tcp://localhost:9440/default?secure=true").await?;
    /// ```
    pub async fn establish(url: &str) -> QueryResult<Self> {
        if url.starts_with("http://") || url.starts_with("https://") {
            #[cfg(feature = "http")]
            {
                let conn = crate::http::ClickHouseConnection::new(url).await?;
                Ok(Connection::Http(conn))
            }
            #[cfg(not(feature = "http"))]
            {
                Err(Error::ConnectionError(
                    "HTTP backend not enabled. Add feature 'http' to Cargo.toml".to_string(),
                ))
            }
        } else if url.starts_with("tcp://") {
            #[cfg(feature = "native")]
            {
                let conn = crate::native::NativeConnection::establish(url).await?;
                Ok(Connection::Native(conn))
            }
            #[cfg(not(feature = "native"))]
            {
                Err(Error::ConnectionError(
                    "Native backend not enabled. Add feature 'native' to Cargo.toml".to_string(),
                ))
            }
        } else {
            Err(Error::ConnectionError(format!(
                "Unknown URL scheme. Use 'http://', 'https://', or 'tcp://'. Got: {}",
                url
            )))
        }
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
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn load_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        self.load(query).await?.into_iter().next().ok_or(Error::NotFound)
    }

    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn load_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        self.load(query).await?.into_iter().next().ok_or(Error::NotFound)
    }

    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn load_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        self.load(query).await?.into_iter().next().ok_or(Error::NotFound)
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
    #[cfg(all(feature = "http", not(feature = "native")))]
    pub async fn load_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        Ok(self.load(query).await?.into_iter().next())
    }

    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn load_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        Ok(self.load(query).await?.into_iter().next())
    }

    #[cfg(all(feature = "http", feature = "native"))]
    pub async fn load_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        Ok(self.load(query).await?.into_iter().next())
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
    // Unified Fetch Methods (HTTP)
    // =========================================================================

    /// Fetch all rows from a query.
    ///
    /// For HTTP backend, row type must derive `clickhouse::Row`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use clickhouse::Row;
    /// use serde::Deserialize;
    ///
    /// #[derive(Debug, Row, Deserialize)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// let users: Vec<User> = conn.fetch_all(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    #[cfg(feature = "http")]
    pub async fn fetch_all<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let sql = self.build_sql(query)?;
        self.fetch_all_raw(&sql).await
    }

    /// Fetch all rows from a raw SQL query.
    #[cfg(feature = "http")]
    pub async fn fetch_all_raw<T>(&self, sql: &str) -> QueryResult<Vec<T>>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
    {
        match self {
            Connection::Http(conn) => {
                conn.client()
                    .query(sql)
                    .fetch_all()
                    .await
                    .map_err(|e| Error::QueryError(e.to_string()))
            }
            #[cfg(feature = "native")]
            Connection::Native(_) => {
                Err(Error::QueryError(
                    "Native backend requires serde::Deserialize. Use fetch_all_native() instead.".to_string()
                ))
            }
        }
    }

    /// Fetch exactly one row from a query.
    #[cfg(feature = "http")]
    pub async fn fetch_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let results: Vec<T> = self.fetch_all(query).await?;
        results.into_iter().next().ok_or(Error::NotFound)
    }

    /// Fetch zero or one row from a query.
    #[cfg(feature = "http")]
    pub async fn fetch_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let results: Vec<T> = self.fetch_all(query).await?;
        Ok(results.into_iter().next())
    }

    // =========================================================================
    // Unified Fetch Methods (Native only)
    // =========================================================================

    /// Fetch all rows from a query (native backend).
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn fetch_all<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: DeserializeOwned,
        Q: QueryFragment<ClickHouse>,
    {
        let sql = self.build_sql(query)?;
        self.fetch_all_raw(&sql).await
    }

    /// Fetch all rows from a raw SQL query (native backend).
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn fetch_all_raw<T>(&self, sql: &str) -> QueryResult<Vec<T>>
    where
        T: DeserializeOwned,
    {
        match self {
            Connection::Native(conn) => {
                let block = conn.query_raw(sql).await?;
                block_to_vec(&block)
            }
        }
    }

    /// Fetch exactly one row (native backend).
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn fetch_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: DeserializeOwned,
        Q: QueryFragment<ClickHouse>,
    {
        let results: Vec<T> = self.fetch_all(query).await?;
        results.into_iter().next().ok_or(Error::NotFound)
    }

    /// Fetch zero or one row (native backend).
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub async fn fetch_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: DeserializeOwned,
        Q: QueryFragment<ClickHouse>,
    {
        let results: Vec<T> = self.fetch_all(query).await?;
        Ok(results.into_iter().next())
    }

    // =========================================================================
    // Zero-Copy API
    // =========================================================================

    /// Load rows using zero-copy parsing with a callback (HTTP backend only).
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
    /// let count = conn.load_zero_copy(
    ///     "SELECT id, name, score FROM users",
    ///     &["id", "name", "score"],
    ///     |row| {
    ///         let id: u64 = row.get_u64("id")?;
    ///         let name: &str = row.get_str("name")?;  // Borrowed!
    ///         let score: f64 = row.get_f64("score")?;
    ///         println!("{}: {} ({})", id, name, score);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    ///
    /// # Backend Support
    ///
    /// - **HTTP**: Full support with true zero-copy parsing
    /// - **Native**: Not supported (returns error). Use `load()` instead.
    #[cfg(feature = "http")]
    pub async fn load_zero_copy<F>(
        &self,
        sql: &str,
        columns: &[&str],
        callback: F,
    ) -> QueryResult<usize>
    where
        F: for<'a, 'b> FnMut(crate::zero_copy::ZeroCopyRow<'a, 'b>) -> QueryResult<()>,
    {
        match self {
            Connection::Http(conn) => conn.load_zero_copy(sql, columns, callback).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                "Zero-copy parsing is only supported on HTTP backend. Use load() instead.".to_string()
            )),
        }
    }

    /// Load rows using zero-copy streaming parsing with a callback (HTTP backend only).
    ///
    /// Unlike `load_zero_copy`, this method processes rows as chunks arrive
    /// from the network, which can reduce peak memory usage for very large result sets.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let count = conn.load_zero_copy_streaming(
    ///     "SELECT * FROM huge_table",
    ///     &["id", "data"],
    ///     |row| {
    ///         // Process each row as it arrives
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    #[cfg(feature = "http")]
    pub async fn load_zero_copy_streaming<F>(
        &self,
        sql: &str,
        columns: &[&str],
        callback: F,
    ) -> QueryResult<usize>
    where
        F: for<'a, 'b> FnMut(crate::zero_copy::ZeroCopyRow<'a, 'b>) -> QueryResult<()>,
    {
        match self {
            Connection::Http(conn) => conn.load_zero_copy_streaming(sql, columns, callback).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                "Zero-copy streaming is only supported on HTTP backend.".to_string()
            )),
        }
    }

    /// Load rows from a query fragment using zero-copy parsing (HTTP backend only).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let count = conn.load_zero_copy_query(
    ///     users::table.filter(users::active.eq(true)),
    ///     &["id", "name"],
    ///     |row| {
    ///         let name = row.get_str("name")?;
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    #[cfg(feature = "http")]
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
        match self {
            Connection::Http(conn) => conn.load_zero_copy_query(query, columns, callback).await,
            #[cfg(feature = "native")]
            Connection::Native(_) => Err(Error::QueryError(
                "Zero-copy parsing is only supported on HTTP backend.".to_string()
            )),
        }
    }

    // =========================================================================
    // Native-specific fetch (when both features enabled)
    // =========================================================================

    /// Fetch all rows using native backend with serde deserialization.
    ///
    /// Use this when you have both HTTP and Native features enabled
    /// and want to fetch from the native backend.
    #[cfg(feature = "native")]
    pub async fn fetch_all_native<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: DeserializeOwned,
        Q: QueryFragment<ClickHouse>,
    {
        let sql = self.build_sql(query)?;
        match self {
            #[cfg(feature = "http")]
            Connection::Http(_) => {
                Err(Error::QueryError(
                    "This is an HTTP connection. Use fetch_all() with clickhouse::Row instead.".to_string()
                ))
            }
            Connection::Native(conn) => {
                let block = conn.query_raw(&sql).await?;
                block_to_vec(&block)
            }
        }
    }
}

/// Convert a native Block to a Vec of deserializable rows.
#[cfg(feature = "native")]
fn block_to_vec<T: DeserializeOwned>(
    block: &clickhouse_rs::Block<clickhouse_rs::types::Complex>,
) -> QueryResult<Vec<T>> {
    let row_count = block.row_count();
    let col_count = block.column_count();

    // Pre-allocate results vector
    let mut results = Vec::with_capacity(row_count);

    // Cache column metadata outside the row loop to avoid repeated allocations
    let columns: Vec<_> = block.columns().iter()
        .map(|col| (col.name().to_string(), col.sql_type()))
        .collect();

    for row_idx in 0..row_count {
        let mut map = serde_json::Map::with_capacity(col_count);

        for (col_idx, (col_name, sql_type)) in columns.iter().enumerate() {
            let value = extract_value(block, row_idx, col_idx, sql_type)?;
            map.insert(col_name.clone(), value);
        }

        let row: T = serde_json::from_value(serde_json::Value::Object(map))
            .map_err(|e| Error::DeserializationError(e.to_string()))?;
        results.push(row);
    }

    Ok(results)
}

/// Helper to convert clickhouse-rs errors.
#[cfg(feature = "native")]
fn ch_err(e: clickhouse_rs::errors::Error) -> Error {
    Error::QueryError(e.to_string())
}

/// Extract a value from a native Block cell.
#[cfg(feature = "native")]
fn extract_value(
    block: &clickhouse_rs::Block<clickhouse_rs::types::Complex>,
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
            // Date is stored as days since epoch, get as u16 and format
            let days: u16 = block.get(row, col).map_err(ch_err)?;
            // Convert to ISO date string (days since 1970-01-01)
            let date = chrono_lite::days_to_date(days as i32);
            serde_json::Value::String(date)
        }
        SqlType::DateTime(_) => {
            // DateTime is stored as seconds since epoch
            let secs: u32 = block.get(row, col).map_err(ch_err)?;
            let datetime = chrono_lite::secs_to_datetime(secs as i64);
            serde_json::Value::String(datetime)
        }
        SqlType::Nullable(inner) => {
            // For Nullable, try to get the inner value. If it fails (NULL), return null.
            // Note: inner is &'static SqlType
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
                    // Fallback: try as optional string
                    block.get::<Option<String>, _>(row, col)
                        .unwrap_or(None)
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null)
                }
            }
        }
        _ => {
            // Fallback: try as string
            let v: String = block.get(row, col).unwrap_or_default();
            serde_json::Value::String(v)
        }
    })
}

/// Simple date/time formatting without chrono dependency.
/// Uses stack-based formatting to avoid heap allocations.
#[cfg(feature = "native")]
mod chrono_lite {
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

    /// Convert days since epoch to ISO date string.
    pub fn days_to_date(days: i32) -> String {
        const DAYS_IN_400_YEARS: i32 = 146097;

        let days = days + 719468; // Adjust for epoch difference

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
    pub fn secs_to_datetime(secs: i64) -> String {
        let days = (secs / 86400) as i32;
        let day_secs = (secs % 86400) as u32;
        let hours = day_secs / 3600;
        let mins = (day_secs % 3600) / 60;
        let secs_val = day_secs % 60;

        // Pre-allocate exact size: "YYYY-MM-DDTHH:MM:SSZ" = 20 chars
        let mut result = String::with_capacity(20);

        // Date part inline
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
