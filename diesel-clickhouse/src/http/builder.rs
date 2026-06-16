//! HTTP client builder.

use clickhouse::Client;

use crate::common::{ConnectionParams, ConnectionBuilder};
use crate::core::result::{Error, QueryResult};

use super::{Compression, ClickHouseConnection};

/// Builder for configuring a ClickHouse HTTP connection.
///
/// All connection parameters (host, port, database, user, password) are required.
/// Optional settings include compression and ClickHouse query options.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::http::HttpClientBuilder;
/// use diesel_clickhouse::http::Compression;
///
/// let conn = HttpClientBuilder::new()
///     .host("localhost")
///     .port(8123)
///     .database("analytics")
///     .user("default")
///     .password("")
///     .compression(Compression::Lz4)
///     .option("max_execution_time", "60")
///     .build()
///     .await?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct HttpClientBuilder {
    /// Common connection parameters (host, port, database, user, password).
    params: ConnectionParams,
    /// Use HTTPS instead of HTTP.
    https: bool,
    /// Compression mode.
    compression: Compression,
    /// ClickHouse query options.
    options: Vec<(String, String)>,
}

impl ConnectionBuilder for HttpClientBuilder {
    fn params_mut(&mut self) -> &mut ConnectionParams {
        &mut self.params
    }
}

impl HttpClientBuilder {
    /// Create a new HTTP client builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use HTTPS instead of HTTP (optional, default: false).
    pub fn https(mut self, enabled: bool) -> Self {
        self.https = enabled;
        self
    }

    /// Set compression mode (optional, default: None).
    pub fn compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    /// Set a ClickHouse query setting (optional).
    ///
    /// Common options:
    /// - `wait_end_of_query` - Wait for query to complete before streaming
    /// - `max_execution_time` - Maximum query execution time in seconds
    /// - `max_query_size` - Maximum query size in bytes
    /// - `max_result_rows` - Maximum number of result rows
    /// - `max_result_bytes` - Maximum result size in bytes
    pub fn option(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.options.push((key.into(), value.into()));
        self
    }

    /// Build and establish the connection (consuming version).
    ///
    /// Returns a unified `Connection` that can be used with all interfaces.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Required fields (host, port, database, user, password) are not set
    /// - Connection to the server fails
    pub async fn build(self) -> QueryResult<crate::Connection> {
        self.build_ref().await
    }

    /// Build and establish the connection (borrowing version).
    ///
    /// This is more efficient for connection pooling as it only clones the
    /// required fields (host, database, user, password) rather than the entire builder.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Required fields (host, port, database, user, password) are not set
    /// - Connection to the server fails
    pub async fn build_ref(&self) -> QueryResult<crate::Connection> {
        let validated = self.params.validate_ref()?;

        let scheme = if self.https { "https" } else { "http" };

        let url = format!("{}://{}:{}", scheme, validated.host, validated.port);
        let mut base_client = Client::default()
            .with_url(&url)
            .with_database(&validated.database)
            .with_user(&validated.user)
            .with_password(&validated.password);

        for (key, value) in &self.options {
            base_client = base_client.with_setting(key, value);
        }

        // Enable JSON-as-string mode for ClickHouse 24.10+ JSON type support
        // This ensures stable serialization format (TypeId instability workaround)
        #[cfg(feature = "json")]
        {
            base_client = base_client
                .with_setting("output_format_binary_write_json_as_string", "1")
                .with_setting("input_format_binary_read_json_as_string", "1");
        }

        // Test connection (using base client; compression not needed for SELECT 1)
        base_client.query("SELECT 1").execute().await
            .map_err(Error::connection_from)?;

        // from_client_with_compression takes the base client and applies compression
        let http_conn = ClickHouseConnection::from_client_with_compression(
            base_client,
            validated.database,
            self.compression,
        );

        Ok(crate::Connection::Http(http_conn))
    }
}
