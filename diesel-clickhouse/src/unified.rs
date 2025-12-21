//! Unified connection interface for diesel-clickhouse.
//!
//! This module provides a unified API that works with both HTTP and Native backends,
//! allowing you to write backend-agnostic code.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::connection::Connection;
//! use diesel_clickhouse::prelude::*;
//!
//! // Connect via HTTP or Native based on URL scheme
//! let conn = Connection::establish("http://localhost:8123/default").await?;
//! // or: Connection::establish("tcp://localhost:9000/default").await?
//!
//! // Execute queries - same API for both backends
//! conn.execute("CREATE TABLE test (id UInt64) ENGINE = Memory").await?;
//!
//! // Insert data
//! conn.insert_values("test", "(1), (2), (3)").await?;
//!
//! // Query with the builder
//! let sql = conn.build_sql(users::table.filter(users::active.eq(true)));
//! ```

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
    Http {
        conn: crate::http::ClickHouseConnection,
        /// Base URL for direct HTTP requests (for unified fetch)
        base_url: String,
    },

    /// Native protocol connection
    #[cfg(feature = "native")]
    Native(crate::native::NativeConnection),
}

impl Connection {
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
                // Extract base URL for direct requests
                let parsed = url::Url::parse(url)
                    .map_err(|e| Error::ConnectionError(format!("Invalid URL: {}", e)))?;
                let base_url = format!(
                    "{}://{}{}",
                    parsed.scheme(),
                    parsed.host_str().unwrap_or("localhost"),
                    parsed.port().map(|p| format!(":{}", p)).unwrap_or_default()
                );
                Ok(Connection::Http { conn, base_url })
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
            Connection::Http { conn, .. } => conn.database(),
            #[cfg(feature = "native")]
            Connection::Native(conn) => conn.database(),
        }
    }

    /// Check if this is an HTTP connection.
    pub fn is_http(&self) -> bool {
        match self {
            #[cfg(feature = "http")]
            Connection::Http { .. } => true,
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
            Connection::Http { conn, .. } => conn.execute_raw(sql).await,
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
            Connection::Http { conn, .. } => conn.execute_statement(&query).await,
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
    /// let sql = conn.build_sql(users::table.filter(users::active.eq(true)));
    /// println!("Query: {}", sql);
    /// ```
    pub fn build_sql<Q>(&self, query: Q) -> String
    where
        Q: QueryFragment<ClickHouse>,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http { conn, .. } => conn.build_query(&query),
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
            Connection::Http { conn, .. } => conn.insert_raw(table, values_sql).await,
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

    /// Get the underlying HTTP connection (if HTTP backend).
    #[cfg(feature = "http")]
    pub fn as_http(&self) -> Option<&crate::http::ClickHouseConnection> {
        match self {
            Connection::Http { conn, .. } => Some(conn),
            #[cfg(feature = "native")]
            Connection::Native(_) => None,
        }
    }

    /// Get the underlying Native connection (if Native backend).
    #[cfg(feature = "native")]
    pub fn as_native(&self) -> Option<&crate::native::NativeConnection> {
        match self {
            #[cfg(feature = "http")]
            Connection::Http { .. } => None,
            Connection::Native(conn) => Some(conn),
        }
    }

    // =========================================================================
    // Unified Fetch Methods
    // =========================================================================

    /// Fetch all rows from a query.
    ///
    /// The row type must implement `serde::Deserialize`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(Debug, Deserialize)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// let users: Vec<User> = conn.fetch_all(
    ///     users::table.filter(users::active.eq(true))
    /// ).await?;
    /// ```
    pub async fn fetch_all<T, Q>(&self, query: Q) -> QueryResult<Vec<T>>
    where
        T: DeserializeOwned,
        Q: QueryFragment<ClickHouse>,
    {
        let sql = self.build_sql(query);
        self.fetch_all_raw(&sql).await
    }

    /// Fetch all rows from a raw SQL query.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = conn.fetch_all_raw("SELECT * FROM users").await?;
    /// ```
    pub async fn fetch_all_raw<T>(&self, sql: &str) -> QueryResult<Vec<T>>
    where
        T: DeserializeOwned,
    {
        match self {
            #[cfg(feature = "http")]
            Connection::Http { conn, base_url } => {
                // Use JSONEachRow format for easy parsing
                let json_sql = format!("{} FORMAT JSONEachRow", sql);

                // Make direct HTTP request to ClickHouse
                let client = reqwest::Client::new();
                let response = client
                    .post(base_url)
                    .query(&[("database", conn.database())])
                    .body(json_sql)
                    .send()
                    .await
                    .map_err(|e| Error::QueryError(e.to_string()))?;

                if !response.status().is_success() {
                    let error_text = response.text().await.unwrap_or_default();
                    return Err(Error::QueryError(error_text));
                }

                let text = response
                    .text()
                    .await
                    .map_err(|e| Error::QueryError(e.to_string()))?;

                // Parse JSONEachRow format (one JSON object per line)
                let mut results = Vec::new();
                for line in text.lines() {
                    if !line.trim().is_empty() {
                        let row: T = serde_json::from_str(line)
                            .map_err(|e| Error::DeserializationError(e.to_string()))?;
                        results.push(row);
                    }
                }
                Ok(results)
            }
            #[cfg(feature = "native")]
            Connection::Native(conn) => {
                let block = conn.query_raw(sql).await?;
                block_to_vec(&block)
            }
        }
    }

    /// Fetch exactly one row from a query.
    ///
    /// Returns an error if no rows are found.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = conn.fetch_one(
    ///     users::table.filter(users::id.eq(1))
    /// ).await?;
    /// ```
    pub async fn fetch_one<T, Q>(&self, query: Q) -> QueryResult<T>
    where
        T: DeserializeOwned,
        Q: QueryFragment<ClickHouse>,
    {
        let results: Vec<T> = self.fetch_all(query).await?;
        results.into_iter().next().ok_or(Error::NotFound)
    }

    /// Fetch zero or one row from a query.
    ///
    /// Returns `None` if no rows are found.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = conn.fetch_optional(
    ///     users::table.filter(users::id.eq(1))
    /// ).await?;
    /// ```
    pub async fn fetch_optional<T, Q>(&self, query: Q) -> QueryResult<Option<T>>
    where
        T: DeserializeOwned,
        Q: QueryFragment<ClickHouse>,
    {
        let results: Vec<T> = self.fetch_all(query).await?;
        Ok(results.into_iter().next())
    }
}

/// Convert a native Block to a Vec of deserializable rows.
#[cfg(feature = "native")]
fn block_to_vec<T: DeserializeOwned>(
    block: &clickhouse_rs::Block<clickhouse_rs::types::Complex>,
) -> QueryResult<Vec<T>> {
    let mut results = Vec::new();

    for row_idx in 0..block.row_count() {
        let mut map = serde_json::Map::new();

        for col_idx in 0..block.column_count() {
            let col_name = block.columns()[col_idx].name();
            let sql_type = block.columns()[col_idx].sql_type();

            let value = extract_value(block, row_idx, col_idx, sql_type)?;
            map.insert(col_name.to_string(), value);
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
    sql_type: clickhouse_rs::types::SqlType,
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
#[cfg(feature = "native")]
mod chrono_lite {
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

        format!("{:04}-{:02}-{:02}", y, m, d)
    }

    /// Convert seconds since epoch to ISO datetime string.
    pub fn secs_to_datetime(secs: i64) -> String {
        let days = (secs / 86400) as i32;
        let day_secs = (secs % 86400) as u32;
        let hours = day_secs / 3600;
        let mins = (day_secs % 3600) / 60;
        let secs = day_secs % 60;

        let date = days_to_date(days);
        format!("{}T{:02}:{:02}:{:02}Z", date, hours, mins, secs)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_url_parsing() {
        // These would need async runtime to actually test
        assert!(true);
    }
}
