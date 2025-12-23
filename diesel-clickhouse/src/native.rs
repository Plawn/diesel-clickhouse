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

use std::borrow::Cow;
use std::time::Duration;

use async_trait::async_trait;
use clickhouse_rs::{Pool, ClientHandle, Block, types::Complex};
use futures::StreamExt;

use crate::core::backend::{BindableValue, BindCollector, ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::core::connection::ClickHouseConnection as ClickHouseConnectionTrait;
use crate::core::escape::escape_identifier;
use crate::core::query_builder::{AstPass, QueryFragment};
use crate::core::result::{Error, QueryResult};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

// Re-export clickhouse-rs types for convenience
pub use clickhouse_rs::{Block as NativeBlock, row, types};

/// Type alias for the complex block type used by FromNativeBlock
pub type ComplexBlock = Block<Complex>;

/// Type alias for the simple block type (used in streaming)
pub type SimpleBlock = Block<clickhouse_rs::types::Simple>;

// =============================================================================
// Compression Mode
// =============================================================================

/// Compression mode for native protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeCompression {
    /// No compression (default).
    #[default]
    None,
    /// LZ4 compression.
    Lz4,
}

// =============================================================================
// Client Builder
// =============================================================================

/// Builder for configuring a ClickHouse Native connection.
///
/// All connection parameters (host, port, database, user, password) are required.
/// Optional settings include compression, TLS, timeouts, and pool configuration.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::native::NativeClientBuilder;
/// use diesel_clickhouse::native::NativeCompression;
/// use std::time::Duration;
///
/// let conn = NativeClientBuilder::new()
///     .host("localhost")
///     .port(9000)
///     .database("analytics")
///     .user("default")
///     .password("")
///     .compression(NativeCompression::Lz4)
///     .pool_max(20)
///     .query_timeout(Duration::from_secs(180))
///     .build()
///     .await?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct NativeClientBuilder {
    host: Option<String>,
    port: Option<u16>,
    database: Option<String>,
    user: Option<String>,
    password: Option<String>,
    compression: NativeCompression,
    secure: bool,
    skip_verify: bool,
    connection_timeout: Option<Duration>,
    ping_timeout: Option<Duration>,
    query_timeout: Option<Duration>,
    pool_min: Option<usize>,
    pool_max: Option<usize>,
}

impl NativeClientBuilder {
    /// Create a new Native client builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the host (required).
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Set the port (required).
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the database (required).
    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    /// Set the user (required).
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Set the password (required).
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Set compression mode (optional, default: None).
    pub fn compression(mut self, compression: NativeCompression) -> Self {
        self.compression = compression;
        self
    }

    /// Enable TLS (optional, default: false).
    ///
    /// Requires the `native-tls-native` feature.
    pub fn secure(mut self, enabled: bool) -> Self {
        self.secure = enabled;
        self
    }

    /// Skip TLS certificate verification (optional, default: false).
    ///
    /// Warning: This is insecure and should only be used for testing.
    pub fn skip_verify(mut self, enabled: bool) -> Self {
        self.skip_verify = enabled;
        self
    }

    /// Set connection timeout (optional).
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = Some(timeout);
        self
    }

    /// Set ping timeout (optional).
    pub fn ping_timeout(mut self, timeout: Duration) -> Self {
        self.ping_timeout = Some(timeout);
        self
    }

    /// Set query timeout (optional).
    pub fn query_timeout(mut self, timeout: Duration) -> Self {
        self.query_timeout = Some(timeout);
        self
    }

    /// Set minimum pool size (optional).
    pub fn pool_min(mut self, min: usize) -> Self {
        self.pool_min = Some(min);
        self
    }

    /// Set maximum pool size (optional).
    pub fn pool_max(mut self, max: usize) -> Self {
        self.pool_max = Some(max);
        self
    }

    /// Build and establish the connection.
    ///
    /// Returns a unified `Connection` that can be used with all interfaces.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Required fields (host, port, database, user, password) are not set
    /// - Connection to the server fails
    pub async fn build(self) -> QueryResult<crate::Connection> {
        let host = self.host.ok_or_else(||
            Error::ConnectionError(Cow::Borrowed("host is required")))?;
        let port = self.port.ok_or_else(||
            Error::ConnectionError(Cow::Borrowed("port is required")))?;
        let database = self.database.ok_or_else(||
            Error::ConnectionError(Cow::Borrowed("database is required")))?;
        let user = self.user.ok_or_else(||
            Error::ConnectionError(Cow::Borrowed("user is required")))?;
        let password = self.password.ok_or_else(||
            Error::ConnectionError(Cow::Borrowed("password is required")))?;

        // URL-encode user, password, and database to handle special characters
        // Only encode characters that have special meaning in URLs
        const URL_SPECIAL: &AsciiSet = &CONTROLS
            .add(b':')  // separates user:password and host:port
            .add(b'@')  // separates userinfo from host
            .add(b'/')  // path separator
            .add(b'?')  // query separator
            .add(b'#'); // fragment separator

        let encoded_user = utf8_percent_encode(&user, URL_SPECIAL).to_string();
        let encoded_password = utf8_percent_encode(&password, URL_SPECIAL).to_string();
        let encoded_database = utf8_percent_encode(&database, URL_SPECIAL).to_string();

        // Build URL with query parameters
        let mut url = format!(
            "tcp://{}:{}@{}:{}/{}",
            encoded_user, encoded_password, host, port, encoded_database
        );
        let mut params = Vec::with_capacity(8);

        if self.secure {
            params.push("secure=true".to_string());
        }
        if self.skip_verify {
            params.push("skip_verify=true".to_string());
        }
        if self.compression == NativeCompression::Lz4 {
            params.push("compression=lz4".to_string());
        }
        // Add timeouts with defaults (clickhouse-rs requires these)
        let connection_timeout = self.connection_timeout.unwrap_or(Duration::from_secs(5));
        params.push(format!("connection_timeout={}ms", connection_timeout.as_millis()));

        let ping_timeout = self.ping_timeout.unwrap_or(Duration::from_secs(3));
        params.push(format!("ping_timeout={}ms", ping_timeout.as_millis()));

        let query_timeout = self.query_timeout.unwrap_or(Duration::from_secs(180));
        params.push(format!("query_timeout={}s", query_timeout.as_secs()));
        if let Some(min) = self.pool_min {
            params.push(format!("pool_min={}", min));
        }
        if let Some(max) = self.pool_max {
            params.push(format!("pool_max={}", max));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let pool = Pool::new(url);

        // Test connection
        let mut client = pool
            .get_handle()
            .await
            .map_err(|e| Error::ConnectionError(Cow::Owned(format!("Failed to connect: {}", e))))?;

        client
            .query("SELECT 1")
            .fetch_all()
            .await
            .map_err(|e| Error::ConnectionError(Cow::Owned(format!("Connection test failed: {}", e))))?;

        let server_addr = format!("{}:{}", host, port);

        let native_conn = NativeConnection {
            pool,
            database,
            server_addr,
            user,
            password,
        };

        Ok(crate::Connection::Native(native_conn))
    }
}

// =============================================================================
// Direct Block Deserialization (optimized, no JSON intermediate)
// =============================================================================

/// Trait for types that can be deserialized directly from a Native Block row.
///
/// This trait is automatically implemented by `#[derive(Row)]` and provides
/// optimized deserialization without JSON intermediate conversion.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::{Row, native::FromNativeBlock};
///
/// #[derive(Debug, Row)]
/// struct User {
///     id: u64,
///     name: String,
/// }
///
/// // FromNativeBlock is auto-implemented, allowing direct Block deserialization
/// ```
pub trait FromNativeBlock: Sized {
    /// Deserialize a row from a Native Block at the given index.
    fn from_block_row(
        block: &ComplexBlock,
        row_idx: usize,
    ) -> QueryResult<Self>;
}

/// Trait for deserializing from any block type (Simple or Complex).
/// This is automatically implemented for types that implement FromNativeBlock
/// and have field types that implement BlockValue for all K.
pub trait FromAnyBlock: Sized {
    /// Deserialize a row from a block of any type at the given index.
    fn from_any_block<K: clickhouse_rs::types::ColumnType>(
        block: &Block<K>,
        row_idx: usize,
    ) -> QueryResult<Self>;
}

/// Helper trait for extracting typed values from a Block column.
///
/// This is used by the `#[derive(Row)]` macro to extract individual field values.
/// Generic over the column type K to support both Complex and Simple blocks.
pub trait BlockValue<K: clickhouse_rs::types::ColumnType = Complex>: Sized {
    /// Get a value from the block at the given row and column name.
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self>;
}

// Macro to implement BlockValue for primitive types with generic K
macro_rules! impl_block_value {
    ($ty:ty, $name:literal) => {
        impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for $ty {
            fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
                block.get(row_idx, column)
                    .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get {} column '{}': {}", $name, column, e))))
            }
        }
    };
}

// Implement BlockValue for common types (generic over K)
impl_block_value!(u8, "u8");
impl_block_value!(u16, "u16");
impl_block_value!(u32, "u32");
impl_block_value!(u64, "u64");
impl_block_value!(i8, "i8");
impl_block_value!(i16, "i16");
impl_block_value!(i32, "i32");
impl_block_value!(i64, "i64");
impl_block_value!(f32, "f32");
impl_block_value!(f64, "f64");
impl_block_value!(bool, "bool");

impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for String {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        let s: &str = block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get String column '{}': {}", column, e))))?;
        Ok(s.to_string())
    }
}

#[cfg(feature = "chrono")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for chrono::DateTime<chrono_tz::Tz> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get DateTime column '{}': {}", column, e))))
    }
}

#[cfg(feature = "chrono")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for chrono::DateTime<chrono::FixedOffset> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        // Get as DateTime<Tz> and convert to FixedOffset
        let dt: chrono::DateTime<chrono_tz::Tz> = block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get DateTime column '{}': {}", column, e))))?;
        Ok(dt.fixed_offset())
    }
}

#[cfg(feature = "chrono")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for chrono::DateTime<chrono::Utc> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        // Get as DateTime<Tz> and convert to Utc
        let dt: chrono::DateTime<chrono_tz::Tz> = block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get DateTime column '{}': {}", column, e))))?;
        Ok(dt.with_timezone(&chrono::Utc))
    }
}

impl<K: clickhouse_rs::types::ColumnType, T: BlockValue<K>> BlockValue<K> for Option<T> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        // Try to get the value, return None if it's NULL
        match T::get_value(block, row_idx, column) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None), // Assume error means NULL for Nullable columns
        }
    }
}

impl<K: clickhouse_rs::types::ColumnType, T> BlockValue<K> for Vec<T>
where
    for<'a> Vec<T>: clickhouse_rs::types::FromSql<'a>,
{
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get Vec column '{}': {}", column, e))))
    }
}

/// Convert a native Block to a Vec using FromNativeBlock trait (optimized).
pub fn block_to_vec_optimized<T: FromNativeBlock>(block: &ComplexBlock) -> QueryResult<Vec<T>> {
    let row_count = block.row_count();
    let mut results = Vec::with_capacity(row_count);

    for row_idx in 0..row_count {
        results.push(T::from_block_row(block, row_idx)?);
    }

    Ok(results)
}

// =============================================================================
// ToNativeBlock - Optimized INSERT via Block API
// =============================================================================

/// Trait for types that can be converted to a native Block for INSERT.
///
/// This trait enables optimized binary INSERT operations using ClickHouse's
/// native Block format instead of generating SQL VALUES text. This provides
/// better performance for bulk inserts.
///
/// This trait is automatically implemented by the `#[row]` attribute macro
/// for types that also derive `Insertable`.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::prelude::*;
/// use diesel_clickhouse::native::ToNativeBlock;
///
/// #[row]
/// #[derive(Debug, Clone, Insertable)]
/// #[diesel_clickhouse(table = users)]
/// struct NewUser {
///     id: u64,
///     name: String,
///     active: bool,
/// }
///
/// // Optimized insert via Block API
/// let users = vec![
///     NewUser { id: 1, name: "Alice".into(), active: true },
///     NewUser { id: 2, name: "Bob".into(), active: false },
/// ];
/// conn.insert_native("users", &users).await?;
/// ```
pub trait ToNativeBlock: Sized {
    /// Column names for this row type.
    fn column_names() -> &'static [&'static str];

    /// Convert a slice of rows to a native Block for efficient INSERT.
    fn rows_to_block(rows: &[Self]) -> QueryResult<Block>;
}

/// Trait for types that can be added as a column to a Block.
///
/// This is implemented for common Rust types that map to ClickHouse types.
pub trait IntoBlockColumn {
    /// The type of the column data vector.
    type ColumnData;

    /// Add this value to a column data vector.
    fn push_to_column(value: &Self, column: &mut Self::ColumnData);

    /// Create an empty column data vector.
    fn new_column() -> Self::ColumnData;

    /// Add the column to a block.
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block;
}

/// Trait for types that can be added as a column to a Block by taking ownership.
///
/// This is an optimization to avoid cloning for types like `String` and `Vec<T>`.
/// Use this trait when you have owned values and want to avoid unnecessary allocations.
///
/// # Example
///
/// ```rust,ignore
/// // With IntoBlockColumn (requires clone):
/// IntoBlockColumn::push_to_column(&my_string, &mut column);
///
/// // With IntoBlockColumnOwned (no clone, takes ownership):
/// IntoBlockColumnOwned::push_to_column_owned(my_string, &mut column);
/// ```
pub trait IntoBlockColumnOwned: IntoBlockColumn + Sized {
    /// Add this value to a column data vector, taking ownership.
    ///
    /// This avoids cloning for types like `String` and `Vec<T>`.
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData);
}

// Implement IntoBlockColumn for primitive types
impl IntoBlockColumn for u8 {
    type ColumnData = Vec<u8>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for u16 {
    type ColumnData = Vec<u16>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for u32 {
    type ColumnData = Vec<u32>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for u64 {
    type ColumnData = Vec<u64>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for i8 {
    type ColumnData = Vec<i8>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for i16 {
    type ColumnData = Vec<i16>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for i32 {
    type ColumnData = Vec<i32>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for i64 {
    type ColumnData = Vec<i64>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for f32 {
    type ColumnData = Vec<f32>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for f64 {
    type ColumnData = Vec<f64>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for bool {
    type ColumnData = Vec<u8>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(if *value { 1 } else { 0 });
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumn for String {
    type ColumnData = Vec<String>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(value.clone());
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumnOwned for String {
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        column.push(value);
    }
}

impl IntoBlockColumn for &str {
    type ColumnData = Vec<String>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push((*value).to_string());
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

// Vec<T> for Array columns
impl<T: Clone> IntoBlockColumn for Vec<T>
where
    Vec<Vec<T>>: clickhouse_rs::types::column::ColumnFrom,
{
    type ColumnData = Vec<Vec<T>>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(value.clone());
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl<T: Clone> IntoBlockColumnOwned for Vec<T>
where
    Vec<Vec<T>>: clickhouse_rs::types::column::ColumnFrom,
{
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        column.push(value);
    }
}

// DateTime types - convert to the format clickhouse-rs expects
#[cfg(feature = "chrono")]
impl IntoBlockColumn for chrono::DateTime<chrono_tz::Tz> {
    type ColumnData = Vec<chrono::DateTime<chrono_tz::Tz>>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumn for chrono::DateTime<chrono::FixedOffset> {
    // clickhouse-rs expects DateTime<Tz>, so we convert
    type ColumnData = Vec<chrono::DateTime<chrono_tz::Tz>>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        // Convert FixedOffset to UTC then to Tz
        use chrono::TimeZone;
        let utc = value.with_timezone(&chrono::Utc);
        let tz_dt = chrono_tz::UTC.from_utc_datetime(&utc.naive_utc());
        column.push(tz_dt);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumn for chrono::DateTime<chrono::Utc> {
    type ColumnData = Vec<chrono::DateTime<chrono_tz::Tz>>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        use chrono::TimeZone;
        let tz_dt = chrono_tz::UTC.from_utc_datetime(&value.naive_utc());
        column.push(tz_dt);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumn for chrono::NaiveDateTime {
    type ColumnData = Vec<chrono::DateTime<chrono_tz::Tz>>;

    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        use chrono::TimeZone;
        let tz_dt = chrono_tz::UTC.from_utc_datetime(value);
        column.push(tz_dt);
    }

    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

// =============================================================================
// Native Connection
// =============================================================================

/// A connection to ClickHouse via native binary protocol.
///
/// This provides better performance than HTTP by using ClickHouse's
/// native binary protocol over TCP.
///
/// # Creating a Connection
///
/// Use [`NativeClientBuilder`] to create a connection:
///
/// ```rust,ignore
/// use diesel_clickhouse::native::NativeClientBuilder;
///
/// let conn = NativeClientBuilder::new()
///     .host("localhost")
///     .port(9000)
///     .database("default")
///     .user("default")
///     .password("")
///     .build()
///     .await?;
/// ```
///
/// ## Ports
///
/// - **Port 9000**: Plain TCP (default)
/// - **Port 9440**: TLS-encrypted TCP (use `.secure(true)`)
#[derive(Clone)]
pub struct NativeConnection {
    pool: Pool,
    database: String,
    /// Server address (host:port) for Arrow connection
    server_addr: String,
    /// Username for authentication
    user: String,
    /// Password for authentication
    password: String,
}

impl NativeConnection {
    /// Create a connection from an existing pool.
    pub fn from_pool(
        pool: Pool,
        database: impl Into<String>,
        server_addr: impl Into<String>,
        user: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            pool,
            database: database.into(),
            server_addr: server_addr.into(),
            user: user.into(),
            password: password.into(),
        }
    }

    /// Get the server address (host:port).
    pub fn server_addr(&self) -> &str {
        &self.server_addr
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
            .map_err(|e| Error::ConnectionError(Cow::Owned(format!("Failed to get handle: {}", e))))
    }

    /// Execute a raw SQL query (no results).
    pub async fn execute_raw(&self, sql: &str) -> QueryResult<()> {
        let mut client = self.get_handle().await?;
        client
            .execute(sql)
            .await
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))?;
        Ok(())
    }

    /// Execute a query fragment (UPDATE, DELETE, etc).
    pub async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let sql = build_sql_interpolated(query)?;
        self.execute_raw(&sql).await
    }

    /// Build SQL from a query fragment without executing.
    pub fn build_query<Q>(&self, query: &Q) -> QueryResult<String>
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
            .map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))
    }

    /// Execute a query fragment and return the result block.
    pub async fn query<Q>(&self, query: Q) -> QueryResult<Block<Complex>>
    where
        Q: QueryFragment<ClickHouse>,
    {
        let sql = build_sql_interpolated(&query)?;
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
            .map_err(|e| Error::QueryError(Cow::Owned(format!("Insert failed: {}", e))))?;
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

    /// Insert rows using the optimized native Block API.
    ///
    /// This method provides the best performance for bulk inserts by using
    /// ClickHouse's native binary Block format instead of generating SQL VALUES.
    ///
    /// The row type must implement `ToNativeBlock`, which is automatically
    /// generated by the `#[row]` attribute for types that also derive `Insertable`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::prelude::*;
    /// use diesel_clickhouse::native::ToNativeBlock;
    ///
    /// #[row]
    /// #[derive(Debug, Clone, Insertable)]
    /// #[diesel_clickhouse(table = users)]
    /// struct NewUser {
    ///     id: u64,
    ///     name: String,
    ///     active: bool,
    /// }
    ///
    /// let users = vec![
    ///     NewUser { id: 1, name: "Alice".into(), active: true },
    ///     NewUser { id: 2, name: "Bob".into(), active: false },
    /// ];
    ///
    /// // Optimized insert via Block API
    /// conn.insert_native("users", &users).await?;
    /// ```
    pub async fn insert_native<T: ToNativeBlock>(&self, table: &str, rows: &[T]) -> QueryResult<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let block = T::rows_to_block(rows)?;
        self.insert(table, block).await
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Build SQL from a QueryFragment.
///
/// Returns an error if the query fragment fails to produce valid SQL.
pub fn build_sql<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<String> {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)?;
    Ok(builder.finish())
}

/// Build SQL from a QueryFragment with bind values interpolated inline.
///
/// This is required for the native backend since clickhouse-rs doesn't support
/// bind parameters like the HTTP backend does. All `?` placeholders are replaced
/// with their actual SQL literal values.
pub fn build_sql_interpolated<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<String> {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)?;

    let sql = builder.finish();
    let bindings = collector.bindable_values();

    interpolate_bindings(&sql, bindings)
}

/// Replace `?` placeholders in SQL with actual bind values.
fn interpolate_bindings(sql: &str, bindings: &[BindableValue]) -> QueryResult<String> {
    let mut result = String::with_capacity(sql.len() + bindings.len() * 10);
    let mut binding_idx = 0;

    for ch in sql.chars() {
        if ch == '?' {
            if binding_idx >= bindings.len() {
                return Err(Error::QueryError(Cow::Owned(format!(
                    "Not enough bind values: expected at least {}, got {}",
                    binding_idx + 1,
                    bindings.len()
                ))));
            }
            result.push_str(&bindings[binding_idx].sql_literal());
            binding_idx += 1;
        } else {
            result.push(ch);
        }
    }

    if binding_idx != bindings.len() {
        return Err(Error::QueryError(Cow::Owned(format!(
            "Too many bind values: expected {}, got {}",
            binding_idx,
            bindings.len()
        ))));
    }

    Ok(result)
}

// =============================================================================
// Query Execution Extensions
// =============================================================================

/// Extension trait for query fragments to get SQL.
pub trait ToSql: QueryFragment<ClickHouse> {
    /// Convert to SQL string.
    ///
    /// Returns an error if the query fragment fails to produce valid SQL.
    fn to_sql_string(&self) -> QueryResult<String> {
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
    // =========================================================================
    // Loading (direct Block deserialization)
    // =========================================================================

    /// Load rows using direct Block deserialization.
    ///
    /// This method deserializes rows directly from the native Block.
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
    /// let users: Vec<User> = conn.load(users::table.select_all()).await?;
    /// ```
    pub async fn load<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        let sql = build_sql_interpolated(&query)?;
        self.load_raw(&sql).await
    }

    /// Load rows from raw SQL.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = conn.load_raw("SELECT id, name FROM users").await?;
    /// ```
    pub async fn load_raw<T: FromNativeBlock + Send>(&self, sql: &str) -> QueryResult<Vec<T>> {
        let block = self.query_raw(sql).await?;
        block_to_vec_optimized(&block)
    }

    // =========================================================================
    // Streaming Methods
    // =========================================================================

    /// Stream rows from a query with a callback.
    ///
    /// This method uses true network streaming - blocks are fetched incrementally
    /// from the server and processed one at a time. Memory usage is O(block_size)
    /// instead of O(total_rows).
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
    pub async fn stream_for_each<T, Q, F>(&self, query: Q, callback: F) -> QueryResult<()>
    where
        T: FromAnyBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> QueryResult<()>,
    {
        let sql = build_sql_interpolated(&query)?;
        self.stream_for_each_raw(&sql, callback).await
    }

    /// Stream rows from raw SQL with a callback.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.stream_for_each_raw(
    ///     "SELECT id, name FROM users",
    ///     |user: User| {
    ///         println!("User: {}", user.name);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn stream_for_each_raw<T, F>(&self, sql: &str, mut callback: F) -> QueryResult<()>
    where
        T: FromAnyBlock + Send,
        F: FnMut(T) -> QueryResult<()>,
    {
        let mut client = self.get_handle().await?;
        let mut block_stream = client.query(sql).stream_blocks();

        while let Some(block_result) = block_stream.next().await {
            let block = block_result.map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))?;
            for row_idx in 0..block.row_count() {
                let item = T::from_any_block(&block, row_idx)?;
                callback(item)?;
            }
        }
        Ok(())
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
    pub async fn stream_for_each_async<T, Q, F, Fut>(&self, query: Q, callback: F) -> QueryResult<()>
    where
        T: FromAnyBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = QueryResult<()>>,
    {
        let sql = build_sql_interpolated(&query)?;
        self.stream_for_each_async_raw(&sql, callback).await
    }

    /// Stream rows from raw SQL with an async callback.
    pub async fn stream_for_each_async_raw<T, F, Fut>(&self, sql: &str, mut callback: F) -> QueryResult<()>
    where
        T: FromAnyBlock + Send,
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = QueryResult<()>>,
    {
        let mut client = self.get_handle().await?;
        let mut block_stream = client.query(sql).stream_blocks();

        while let Some(block_result) = block_stream.next().await {
            let block = block_result.map_err(|e| Error::QueryError(Cow::Owned(e.to_string())))?;
            for row_idx in 0..block.row_count() {
                let item = T::from_any_block(&block, row_idx)?;
                callback(item).await?;
            }
        }
        Ok(())
    }

    /// Stream rows from a query, returning an async iterator.
    ///
    /// This method provides true network streaming - a background task reads blocks
    /// from the server and sends rows through a channel. Memory usage is O(block_size)
    /// instead of O(total_rows).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut stream = conn.stream::<User, _>(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    ///
    /// while let Some(user) = stream.next().await? {
    ///     println!("User: {}", user.name);
    /// }
    /// ```
    pub fn stream<T, Q>(&self, query: Q) -> QueryResult<crate::stream::NativeBlockStream<T>>
    where
        T: FromAnyBlock + Send + 'static,
        Q: QueryFragment<ClickHouse> + Send,
    {
        let sql = build_sql_interpolated(&query)?;
        Ok(self.stream_raw(&sql))
    }

    /// Stream rows from raw SQL, returning an async iterator.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut stream = conn.stream_raw::<User>("SELECT id, name FROM users");
    ///
    /// while let Some(user) = stream.next().await? {
    ///     println!("User: {}", user.name);
    /// }
    /// ```
    pub fn stream_raw<T>(&self, sql: &str) -> crate::stream::NativeBlockStream<T>
    where
        T: FromAnyBlock + Send + 'static,
    {
        // Channel buffer size - balances memory usage vs throughput
        // Using 1024 rows buffer which is reasonable for most use cases
        let (tx, rx) = tokio::sync::mpsc::channel(1024);
        let pool = self.pool.clone();
        let sql = sql.to_string();

        // Spawn background task to read blocks and send rows
        tokio::spawn(async move {
            let client_result = pool.get_handle().await;
            let mut client = match client_result {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(Error::ConnectionError(Cow::Owned(
                        format!("Failed to get connection handle: {}", e)
                    )))).await;
                    return;
                }
            };

            let mut block_stream = client.query(&sql).stream_blocks();

            while let Some(block_result) = block_stream.next().await {
                match block_result {
                    Ok(block) => {
                        for row_idx in 0..block.row_count() {
                            let item_result = T::from_any_block(&block, row_idx);
                            if tx.send(item_result).await.is_err() {
                                // Receiver dropped, stop streaming
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(Error::QueryError(Cow::Owned(e.to_string())))).await;
                        return;
                    }
                }
            }
        });

        crate::stream::NativeBlockStream::new(rx)
    }

    /// Load a single row.
    ///
    /// Returns an error if no rows are returned.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = conn.load_one(users::table.filter(users::id.eq(1))).await?;
    /// ```
    pub async fn load_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        let mut results = self.load(query).await?;
        results.pop().ok_or(Error::NotFound)
    }

    /// Load an optional single row.
    ///
    /// Returns `None` if no rows are returned.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = conn.load_optional(
    ///     users::table.filter(users::id.eq(1))
    /// ).await?;
    /// ```
    pub async fn load_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: FromNativeBlock + Send,
        Q: QueryFragment<ClickHouse> + Send,
    {
        let mut results = self.load(query).await?;
        Ok(results.pop())
    }
}

#[async_trait]
impl ClickHouseConnectionTrait for NativeConnection {
    async fn execute_raw(&self, sql: &str) -> QueryResult<()> {
        NativeConnection::execute_raw(self, sql).await
    }

    async fn execute_statement<Q>(&self, query: &Q) -> QueryResult<()>
    where
        Q: QueryFragment<ClickHouse> + Send + Sync,
    {
        let sql = build_sql_interpolated(query)?;
        self.execute_raw(&sql).await
    }

    fn build_sql<Q>(&self, query: &Q) -> QueryResult<String>
    where
        Q: QueryFragment<ClickHouse>,
    {
        build_sql_interpolated(query)
    }

    fn database(&self) -> &str {
        &self.database
    }
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
        let result = build_sql(&query).expect("failed to build SQL");
        assert_eq!(result, "SELECT * FROM test_table");
    }
}
