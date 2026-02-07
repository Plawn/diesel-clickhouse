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

mod builder;
mod sql;

use async_trait::async_trait;
use clickhouse::Client;
use serde::Serialize;

use crate::core::backend::ClickHouse;
use crate::core::connection::ClickHouseConnection as ClickHouseConnectionTrait;
use crate::core::query_builder::QueryFragment;
use crate::core::result::{Error, QueryResult};

// Re-export submodule items
pub use builder::HttpClientBuilder;
pub use sql::{build_sql, build_sql_native, BindableValue, NativeCompiledQuery, CompiledQuery, CompiledQueryExt, compile_query};

// Re-export clickhouse Row for convenience (for users who need direct clickhouse crate access)
pub use clickhouse::Row as NativeClickHouseRow;

// Re-export Compression from core for unified API
pub use crate::core::connection::Compression;

// =============================================================================
// Connection
// =============================================================================

/// A connection to ClickHouse via HTTP.
#[derive(Clone)]
pub struct ClickHouseConnection {
    client: Client,
    /// The base client without compression applied, used to rebuild `client`
    /// when switching compression modes (e.g., from Lz4 back to None).
    base_client: Client,
    database: String,
    compression: Compression,
}

impl ClickHouseConnection {
    /// Create a connection from an existing Client.
    pub fn from_client(client: Client, database: impl Into<String>) -> Self {
        Self {
            base_client: client.clone(),
            client,
            database: database.into(),
            compression: Compression::None,
        }
    }

    /// Create a connection from an existing Client with compression setting.
    ///
    /// The provided `client` should be the base client (without compression).
    /// Compression is applied on top of it based on the `compression` parameter.
    pub(crate) fn from_client_with_compression(
        client: Client,
        database: impl Into<String>,
        compression: Compression,
    ) -> Self {
        let active_client = match compression {
            Compression::Lz4 | Compression::Lz4Hc => {
                client.clone().with_compression(clickhouse::Compression::Lz4)
            }
            Compression::None | Compression::Zstd => client.clone(),
        };
        Self {
            base_client: client,
            client: active_client,
            database: database.into(),
            compression,
        }
    }

    /// Enable compression for this connection.
    ///
    /// Compression is beneficial for large INSERT operations (>1KB payload).
    /// For small queries, the compression overhead may outweigh the benefits.
    ///
    /// # Supported modes
    ///
    /// - `Compression::None` - No compression
    /// - `Compression::Lz4` - LZ4 compression (recommended)
    /// - `Compression::Lz4Hc` - Falls back to LZ4 (Lz4Hc is deprecated in clickhouse crate)
    /// - `Compression::Zstd` - Not supported, falls back to None
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
        // Rebuild client from base_client to correctly handle all transitions,
        // including switching from Lz4 back to None.
        match compression {
            Compression::Lz4 | Compression::Lz4Hc => {
                self.client = self.base_client.clone().with_compression(clickhouse::Compression::Lz4);
            }
            Compression::None | Compression::Zstd => {
                // Zstd not supported by clickhouse crate, use no compression
                self.client = self.base_client.clone();
            }
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
            .map_err(Error::query_from)
    }

    /// Execute a query fragment (UPDATE, DELETE, etc).
    ///
    /// Uses native parameter binding for query plan caching.
    pub async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(query)?;
        self.execute_native(&compiled).await
    }

    /// Execute a NativeCompiledQuery with native parameter binding.
    pub async fn execute_native(&self, compiled: &NativeCompiledQuery) -> QueryResult<()> {
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .execute()
            .await
            .map_err(Error::query_from)
    }

    /// Build SQL from a query fragment without executing.
    pub fn build_query<Q>(&self, query: &Q) -> QueryResult<String>
    where
        Q: QueryFragment<ClickHouse>,
    {
        build_sql(query)
    }
}

// =============================================================================
// Query Execution Extensions
// =============================================================================

// Re-export ToSqlString from core
pub use crate::core::sql_builder::ToSqlString;

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
        let compiled = build_sql_native(&query)?;
        Ok(compiled.bind_to(self.client.query(&compiled.sql)))
    }

    /// Stream rows from a query using a cursor.
    ///
    /// This method returns a `RowCursor` that allows you to process results
    /// row by row without loading everything into memory. Ideal for large result sets.
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
    /// // Stream results row by row
    /// let mut cursor = conn.stream::<User, _>(
    ///     users::table.filter(users::active.eq(true))
    /// )?;
    ///
    /// while let Some(user) = cursor.next().await? {
    ///     println!("User: {} - {}", user.id, user.name);
    /// }
    /// ```
    ///
    /// # Note
    ///
    /// The row type `T` must implement `clickhouse::Row` (via `#[derive(Row)]`).
    /// Cursors may return errors after producing some rows. Use
    /// `client.with_option("wait_end_of_query", "1")` for server-side buffering
    /// if you need to ensure all rows succeed before processing.
    pub fn stream<T, Q>(&self, query: Q) -> QueryResult<clickhouse::query::RowCursor<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        compiled.bind_to(self.client.query(&compiled.sql))
            .fetch::<T>()
            .map_err(Error::query_from)
    }

    /// Stream rows from a query with native parameter binding.
    ///
    /// Like `stream()`, but uses native parameter binding for better
    /// query plan caching on the server.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let compiled = build_sql_native(&users::table.filter(users::id.eq(42)))?;
    /// let mut cursor = conn.stream_native::<User>(&compiled)?;
    ///
    /// while let Some(user) = cursor.next().await? {
    ///     println!("User: {}", user.name);
    /// }
    /// ```
    pub fn stream_native<T>(&self, compiled: &NativeCompiledQuery) -> QueryResult<clickhouse::query::RowCursor<T>>
    where
        T: clickhouse::Row,
    {
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch::<T>()
            .map_err(Error::query_from)
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

    // =========================================================================
    // Native Parameter Binding
    // =========================================================================

    /// Create a bound query with native parameter binding.
    ///
    /// This uses the clickhouse crate's native `.bind()` mechanism which
    /// properly serializes parameters without manual string escaping.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = conn.bound_query("SELECT * FROM users WHERE id = ? AND name = ?")
    ///     .bind(42u64)
    ///     .bind("alice")
    ///     .fetch_all()
    ///     .await?;
    /// ```
    pub fn bound_query(&self, sql: &str) -> clickhouse::query::Query {
        self.client.query(sql)
    }

    /// Execute a query with bound parameters (no results).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.execute_bound("ALTER TABLE users DELETE WHERE id = ?", &[&42u64]).await?;
    /// ```
    pub async fn execute_bound<T: Serialize + Send + Sync>(
        &self,
        sql: &str,
        params: &[T],
    ) -> QueryResult<()> {
        let mut query = self.client.query(sql);
        for param in params {
            query = query.bind(param);
        }
        query.execute().await.map_err(Error::query_from)
    }

    /// Load rows with bound parameters using native Row deserialization.
    ///
    /// This is the recommended way to execute parameterized queries as it uses
    /// the clickhouse crate's native binding and deserialization.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(Debug, clickhouse::Row, Deserialize)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// let users: Vec<User> = conn.load_bound(
    ///     "SELECT id, name FROM users WHERE active = ?",
    ///     |q| q.bind(true),
    /// ).await?;
    /// ```
    pub async fn load_bound<T, F>(&self, sql: &str, bind_fn: F) -> QueryResult<Vec<T>>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
        F: FnOnce(clickhouse::query::Query) -> clickhouse::query::Query,
    {
        let query = bind_fn(self.client.query(sql));
        query
            .fetch_all()
            .await
            .map_err(Error::query_from)
    }

    /// Fetch a single row with bound parameters.
    ///
    /// Returns an error if no rows are returned.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = conn.load_one_bound(
    ///     "SELECT id, name FROM users WHERE id = ?",
    ///     |q| q.bind(42u64),
    /// ).await?;
    /// ```
    pub async fn load_one_bound<T, F>(&self, sql: &str, bind_fn: F) -> QueryResult<T>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
        F: FnOnce(clickhouse::query::Query) -> clickhouse::query::Query,
    {
        let query = bind_fn(self.client.query(sql));
        query
            .fetch_one()
            .await
            .map_err(Error::query_from)
    }

    /// Fetch an optional row with bound parameters.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = conn.load_optional_bound(
    ///     "SELECT id, name FROM users WHERE id = ?",
    ///     |q| q.bind(42u64),
    /// ).await?;
    /// ```
    pub async fn load_optional_bound<T, F>(&self, sql: &str, bind_fn: F) -> QueryResult<Option<T>>
    where
        T: clickhouse::RowOwned + clickhouse::RowRead + Send,
        F: FnOnce(clickhouse::query::Query) -> clickhouse::query::Query,
    {
        let query = bind_fn(self.client.query(sql));
        query
            .fetch_optional()
            .await
            .map_err(Error::query_from)
    }
}

// =============================================================================
// Apache Arrow API
// =============================================================================

#[cfg(feature = "arrow")]
impl ClickHouseConnection {
    /// Load query results as Apache Arrow RecordBatches.
    ///
    /// This method uses ClickHouse's ArrowStream format for true zero-copy
    /// columnar data access. Arrow is the most efficient format for analytical
    /// workloads and enables seamless interoperability with tools like Polars,
    /// DataFusion, and DuckDB.
    ///
    /// # Zero-Copy Architecture
    ///
    /// This implementation uses [`ZeroCopyArrowDecoder`] which:
    /// - Converts `bytes::Bytes` chunks to Arrow buffers without copying
    /// - Parses batches incrementally as data arrives
    /// - Minimizes memory usage during streaming
    ///
    /// # Arguments
    ///
    /// * `sql` - The SQL query to execute
    ///
    /// # Returns
    ///
    /// An `ArrowResult` containing the schema and record batches.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    /// use diesel_clickhouse::arrow::array::{Int64Array, StringArray};
    ///
    /// let result = conn.load_arrow("SELECT id, name FROM users").await?;
    ///
    /// println!("Schema: {:?}", result.schema());
    /// println!("Total rows: {}", result.num_rows());
    ///
    /// for batch in result {
    ///     let ids = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
    ///     let names = batch.column(1).as_any().downcast_ref::<StringArray>().unwrap();
    ///
    ///     for i in 0..batch.num_rows() {
    ///         println!("User {}: {}", ids.value(i), names.value(i));
    ///     }
    /// }
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - **Zero-copy**: Data is accessed directly without parsing
    /// - **Columnar**: Efficient for queries accessing few columns
    /// - **SIMD-ready**: Arrow's layout enables vectorized operations
    /// - **Recommended for**: Large analytical queries, data pipelines
    pub async fn load_arrow(&self, sql: &str) -> QueryResult<crate::arrow::ArrowResult> {
        self.load_arrow_with_bindings(self.client.query(sql)).await
    }

    /// Internal method to load Arrow with optional bindings applied to the query.
    ///
    /// Uses zero-copy streaming: each chunk from the HTTP response is converted
    /// to an Arrow buffer without copying, and batches are decoded incrementally.
    async fn load_arrow_with_bindings(
        &self,
        query: clickhouse::query::Query,
    ) -> QueryResult<crate::arrow::ArrowResult> {
        use crate::arrow::ZeroCopyArrowDecoder;

        // Execute query with ArrowStream format
        let mut cursor = query
            .fetch_bytes("ArrowStream")
            .map_err(Error::query_from)?;

        // Zero-copy streaming decode
        let mut decoder = ZeroCopyArrowDecoder::new();
        // Pre-allocate for typical Arrow streams (usually 1-8 batches)
        let mut all_batches = Vec::with_capacity(8);

        loop {
            match cursor.next().await {
                Ok(Some(chunk)) => {
                    // Zero-copy: Bytes -> Buffer conversion
                    let batches = decoder.decode_chunk(chunk)?;
                    all_batches.extend(batches);
                }
                Ok(None) => break,
                Err(e) => return Err(Error::query_from(e)),
            }
        }

        decoder.finish()?;

        // Build result
        if all_batches.is_empty() {
            return Err(Error::DeserializationError(std::borrow::Cow::Borrowed(
                "Arrow stream contained no batches",
            )));
        }

        let schema = all_batches[0].schema();
        Ok(crate::arrow::ArrowResult::new(schema, all_batches))
    }

    /// Load query results as Arrow with a callback for each batch.
    ///
    /// This method processes batches as they are parsed, which can be more
    /// memory-efficient for very large result sets.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::arrow::array::Int64Array;
    ///
    /// let total = conn.load_arrow_callback(
    ///     "SELECT id FROM huge_table",
    ///     |batch| {
    ///         let ids = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
    ///         println!("Processing {} rows", ids.len());
    ///         Ok(())
    ///     }
    /// ).await?;
    /// println!("Processed {} total rows", total);
    /// ```
    pub async fn load_arrow_callback<F>(
        &self,
        sql: &str,
        callback: F,
    ) -> QueryResult<usize>
    where
        F: FnMut(::arrow::array::RecordBatch) -> QueryResult<()> + Send + 'static,
    {
        self.load_arrow_callback_with_bindings(self.client.query(sql), callback).await
    }

    /// Internal method to load Arrow with callback, with optional bindings.
    ///
    /// Uses streaming parsing - data is processed as it arrives from the network
    /// without buffering the entire result set in memory.
    async fn load_arrow_callback_with_bindings<F>(
        &self,
        query: clickhouse::query::Query,
        mut callback: F,
    ) -> QueryResult<usize>
    where
        F: FnMut(::arrow::array::RecordBatch) -> QueryResult<()> + Send + 'static,
    {
        use futures::StreamExt;

        let mut stream = std::pin::pin!(self.stream_arrow_with_bindings(query));
        let mut count = 0;

        while let Some(batch_result) = stream.next().await {
            let batch = batch_result?;
            count += batch.num_rows();
            callback(batch)?;
        }

        Ok(count)
    }

    /// Load query results from a QueryFragment as Arrow.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = conn.load_arrow_query(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    pub async fn load_arrow_query<Q>(&self, query: Q) -> QueryResult<crate::arrow::ArrowResult>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        self.load_arrow_with_bindings(compiled.bind_to(self.client.query(&compiled.sql))).await
    }

    /// Stream Arrow RecordBatches from a SQL query.
    ///
    /// Returns an async stream that yields batches as they are parsed,
    /// without buffering the entire result set in memory.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use futures::StreamExt;
    ///
    /// let mut stream = conn.stream_arrow("SELECT * FROM huge_table");
    /// while let Some(batch_result) = stream.next().await {
    ///     let batch = batch_result?;
    ///     println!("Got batch with {} rows", batch.num_rows());
    /// }
    /// ```
    pub fn stream_arrow(
        &self,
        sql: &str,
    ) -> impl futures::Stream<Item = QueryResult<::arrow::array::RecordBatch>> + Send + 'static {
        self.stream_arrow_with_bindings(self.client.query(sql))
    }

    /// Stream Arrow RecordBatches from a QueryFragment.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use futures::StreamExt;
    ///
    /// let mut stream = conn.stream_arrow_query(
    ///     users::table.filter(users::active.eq(true))
    /// )?;
    /// while let Some(batch_result) = stream.next().await {
    ///     let batch = batch_result?;
    ///     println!("Got batch with {} rows", batch.num_rows());
    /// }
    /// ```
    pub fn stream_arrow_query<Q>(
        &self,
        query: Q,
    ) -> QueryResult<impl futures::Stream<Item = QueryResult<::arrow::array::RecordBatch>> + Send + 'static>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        Ok(self.stream_arrow_with_bindings(compiled.bind_to(self.client.query(&compiled.sql))))
    }

    /// Internal method to stream Arrow batches with bindings.
    ///
    /// # Zero-Copy Implementation
    ///
    /// This method uses [`ZeroCopyArrowDecoder`] to process chunks directly
    /// from the HTTP response without intermediate copies:
    ///
    /// 1. `BytesCursor::next()` returns `bytes::Bytes` (reference-counted)
    /// 2. `ZeroCopyArrowDecoder::decode_chunk()` converts to Arrow `Buffer` (zero-copy)
    /// 3. `StreamDecoder` parses batches directly from the buffer
    /// 4. Batches are yielded to the caller with data still in original buffer
    fn stream_arrow_with_bindings(
        &self,
        query: clickhouse::query::Query,
    ) -> impl futures::Stream<Item = QueryResult<::arrow::array::RecordBatch>> + Send + 'static {
        async_stream::stream! {
            use crate::arrow::ZeroCopyArrowDecoder;

            // Execute query with ArrowStream format
            let mut cursor = match query.fetch_bytes("ArrowStream") {
                Ok(c) => c,
                Err(e) => {
                    yield Err(Error::query_from(e));
                    return;
                }
            };

            let mut decoder = ZeroCopyArrowDecoder::new();

            // Stream chunks and decode batches
            loop {
                match cursor.next().await {
                    Ok(Some(chunk)) => {
                        // Zero-copy decode: Bytes -> Buffer -> RecordBatch
                        match decoder.decode_chunk(chunk) {
                            Ok(batches) => {
                                for batch in batches {
                                    yield Ok(batch);
                                }
                            }
                            Err(e) => {
                                yield Err(e);
                                return;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        yield Err(Error::query_from(e));
                        return;
                    }
                }
            }

            // Finalize decoder
            if let Err(e) = decoder.finish() {
                yield Err(e);
            }
        }
    }

    // =========================================================================
    // Zero-Copy Row API (Arrow-backed)
    // =========================================================================

    /// Load rows using zero-copy parsing with a callback.
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
    ///     "SELECT id, name, score FROM users WHERE active = 1",
    ///     |row| {
    ///         let id: u64 = row.get_u64("id")?;
    ///         let name: &str = row.get_str("name")?;  // Zero-copy borrow!
    ///         let score: f64 = row.get_f64("score")?;
    ///
    ///         println!("User {}: {} (score: {})", id, name, score);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// println!("Processed {} rows", count);
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Uses Arrow format (binary, no parsing overhead)
    /// - True zero-copy: data is accessed directly from Arrow buffers
    /// - SIMD-friendly memory layout
    /// - Ideal for large result sets with row-by-row processing
    pub async fn load_zero_copy<F>(&self, sql: &str, mut callback: F) -> QueryResult<usize>
    where
        F: for<'a> FnMut(crate::arrow::ArrowRow<'a>) -> QueryResult<()>,
    {
        let result = self.load_arrow(sql).await?;
        let column_indices = crate::arrow::build_column_index(result.schema());

        let mut total_count = 0;
        for batch in result.batches() {
            let count = crate::arrow::for_each_row(batch, &column_indices, &mut callback)?;
            total_count += count;
        }

        Ok(total_count)
    }

    /// Load rows from a query fragment using zero-copy parsing.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let count = conn.load_zero_copy_query(
    ///     users::table.filter(users::active.eq(true)).select((users::id, users::name)),
    ///     |row| {
    ///         let id = row.get_u64("id")?;
    ///         let name = row.get_str("name")?;
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn load_zero_copy_query<Q, F>(&self, query: Q, mut callback: F) -> QueryResult<usize>
    where
        Q: QueryFragment<ClickHouse>,
        F: for<'a> FnMut(crate::arrow::ArrowRow<'a>) -> QueryResult<()>,
    {
        let compiled = build_sql_native(&query)?;
        let result = self.load_arrow_with_bindings(compiled.bind_to(self.client.query(&compiled.sql))).await?;
        let column_indices = crate::arrow::build_column_index(result.schema());

        let mut total_count = 0;
        for batch in result.batches() {
            let count = crate::arrow::for_each_row(batch, &column_indices, &mut callback)?;
            total_count += count;
        }

        Ok(total_count)
    }
}

// =============================================================================
// Unified ClickHouseConnection Implementation
// =============================================================================

#[async_trait]
impl ClickHouseConnectionTrait for ClickHouseConnection {
    async fn execute_raw(&self, sql: &str) -> QueryResult<()> {
        ClickHouseConnection::execute_raw(self, sql).await
    }

    async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        // Use native parameter binding
        let compiled = build_sql_native(query)?;
        self.execute_native(&compiled).await
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
// Optimized RowBinary Loading
// =============================================================================

impl ClickHouseConnection {
    /// Load rows using RowBinary format (optimized, 2-3x faster than JSON).
    ///
    /// This method uses the native RowBinary format which is significantly
    /// faster than JSONEachRow. The row type must derive `clickhouse::Row`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    ///
    /// #[derive(Debug, Row)]  // Generates both serde and clickhouse::Row
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// // Fast RowBinary loading
    /// let users: Vec<User> = conn.load_optimized(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    ///
    /// # Performance
    ///
    /// RowBinary format provides:
    /// - 2-3x faster parsing than JSON
    /// - Lower memory allocations
    /// - Native type handling without string conversion
    pub async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        self.load_compiled(&compiled).await
    }

    /// Load rows using RowBinary with a pre-compiled query.
    pub async fn load_compiled<T>(&self, compiled: &NativeCompiledQuery) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
    {
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch_all()
            .await
            .map_err(Error::query_from)
    }

    /// Load a single row using RowBinary format.
    pub async fn load_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch_one()
            .await
            .map_err(Error::query_from)
    }

    /// Load an optional row using RowBinary format.
    pub async fn load_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        let query = compiled.bind_to(self.client.query(&compiled.sql));
        query
            .fetch_optional()
            .await
            .map_err(Error::query_from)
    }

    /// Load rows from raw SQL using RowBinary format.
    pub async fn load_raw<T>(&self, sql: &str) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
    {
        self.client
            .query(sql)
            .fetch_all()
            .await
            .map_err(Error::query_from)
    }

    /// Load rows with a pre-allocated capacity hint.
    ///
    /// Use this when you know approximately how many rows will be returned
    /// (e.g., from a LIMIT clause or estimated count). This avoids Vec
    /// reallocations during loading.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // When you know the approximate result size
    /// let users: Vec<User> = conn.load_with_capacity(
    ///     users::table.filter(users::active.eq(true)).limit(1000),
    ///     1000,
    /// ).await?;
    /// ```
    ///
    /// # Performance
    ///
    /// - `capacity = 0`: Same as `load()`, Vec grows dynamically
    /// - `capacity = expected_rows`: Single allocation, no reallocations
    /// - `capacity > actual_rows`: Slight memory overhead, but no reallocations
    pub async fn load_with_capacity<T, Q>(&self, query: Q, capacity: usize) -> QueryResult<Vec<T>>
    where
        T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
        Q: QueryFragment<ClickHouse>,
    {
        let compiled = build_sql_native(&query)?;
        let mut cursor = compiled
            .bind_to(self.client.query(&compiled.sql))
            .fetch::<T>()
            .map_err(Error::query_from)?;

        let mut results = Vec::with_capacity(capacity);
        while let Some(row) = cursor.next().await.map_err(Error::query_from)? {
            results.push(row);
        }

        Ok(results)
    }
}

// =============================================================================
// Async Insert Support
// =============================================================================

impl ClickHouseConnection {
    /// Insert rows using async insert mode with RowBinary format.
    ///
    /// This method uses ClickHouse's async insert mode which buffers data
    /// server-side for optimal write performance.
    ///
    /// # Type Requirements
    ///
    /// The row type `R` must:
    /// - Derive `#[derive(clickhouse::Row, serde::Serialize)]`
    /// - Have `Value<'a> = R` (typically primitive-only fields)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::async_insert::AsyncInsertConfig;
    ///
    /// let config = AsyncInsertConfig::fire_and_forget();
    /// conn.async_insert_rows::<events::table, _>(&config, &events).await?;
    /// ```
    pub async fn async_insert_rows<T, R>(
        &self,
        config: &crate::async_insert::AsyncInsertConfig,
        rows: &[R],
    ) -> QueryResult<()>
    where
        T: crate::core::query_source::Table,
        R: clickhouse::Row + Serialize + Send + Sync,
        for<'a> R: clickhouse::Row<Value<'a> = R>,
        for<'a> <R as clickhouse::Row>::Value<'a>: Serialize,
    {
        if rows.is_empty() {
            return Ok(());
        }

        let insert = self
            .client()
            .insert::<R>(T::table_name())
            .await
            .map_err(Error::query_from)?;

        let mut insert = config.apply_to_http_insert(insert);

        for row in rows {
            insert.write(row).await.map_err(Error::query_from)?;
        }

        insert.end().await.map_err(Error::query_from)?;
        Ok(())
    }

    /// Force the server to flush its async insert buffer.
    pub async fn flush_async_insert_queue(&self) -> QueryResult<()> {
        self.execute_raw("SYSTEM FLUSH ASYNC INSERT QUEUE").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::expression::sql as sql_literal;
    use crate::core::query_builder::SelectStatement;
    use crate::core::test_utils::RawTable;

    #[test]
    fn test_build_sql() {
        let query = SelectStatement::new(RawTable("test_table"));
        let result = build_sql(&query).expect("failed to build SQL");
        assert_eq!(result, "SELECT * FROM test_table");

        let query = SelectStatement::new(RawTable("test_table"))
            .filter(sql_literal::<diesel_clickhouse_types::Bool>("id > 10"))
            .limit(100);
        let result = build_sql(&query).expect("failed to build SQL");
        // build_sql returns SQL with placeholders, not inlined values
        assert_eq!(result, "SELECT * FROM test_table WHERE id > 10 LIMIT ?");
    }
}
