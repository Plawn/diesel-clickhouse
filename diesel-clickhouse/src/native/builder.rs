//! Native connection builder.

use std::borrow::Cow;
use std::time::Duration;

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

use crate::common::{ConnectionParams, ConnectionBuilder};
use crate::core::result::{Error, QueryResult};

use super::{NativeCompression, NativeConnection};

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
    /// Common connection parameters (host, port, database, user, password).
    params: ConnectionParams,
    /// Compression mode.
    compression: NativeCompression,
    /// Enable TLS.
    secure: bool,
    /// Skip TLS certificate verification.
    skip_verify: bool,
    /// Connection timeout.
    connection_timeout: Option<Duration>,
    /// Ping timeout.
    ping_timeout: Option<Duration>,
    /// Query timeout.
    query_timeout: Option<Duration>,
    /// Minimum pool size.
    pool_min: Option<usize>,
    /// Maximum pool size.
    pool_max: Option<usize>,
}

impl ConnectionBuilder for NativeClientBuilder {
    fn params_mut(&mut self) -> &mut ConnectionParams {
        &mut self.params
    }
}

impl NativeClientBuilder {
    /// Create a new Native client builder.
    pub fn new() -> Self {
        Self::default()
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
        use clickhouse_rs::Pool;

        let validated = self.params.validate()?;

        // URL-encode user, password, and database to handle special characters
        // Only encode characters that have special meaning in URLs
        const URL_SPECIAL: &AsciiSet = &CONTROLS
            .add(b':')  // separates user:password and host:port
            .add(b'@')  // separates userinfo from host
            .add(b'/')  // path separator
            .add(b'?')  // query separator
            .add(b'#'); // fragment separator

        let encoded_user = utf8_percent_encode(&validated.user, URL_SPECIAL).to_string();
        let encoded_password = utf8_percent_encode(&validated.password, URL_SPECIAL).to_string();
        let encoded_database = utf8_percent_encode(&validated.database, URL_SPECIAL).to_string();

        // Build URL with query parameters
        let mut url = format!(
            "tcp://{}:{}@{}:{}/{}",
            encoded_user, encoded_password, validated.host, validated.port, encoded_database
        );
        let mut url_params = Vec::with_capacity(8);

        if self.secure {
            url_params.push("secure=true".to_string());
        }
        if self.skip_verify {
            url_params.push("skip_verify=true".to_string());
        }
        if self.compression == NativeCompression::Lz4 {
            url_params.push("compression=lz4".to_string());
        }
        // Add timeouts with defaults (clickhouse-rs requires these)
        let connection_timeout = self.connection_timeout.unwrap_or(Duration::from_secs(5));
        url_params.push(format!("connection_timeout={}ms", connection_timeout.as_millis()));

        let ping_timeout = self.ping_timeout.unwrap_or(Duration::from_secs(3));
        url_params.push(format!("ping_timeout={}ms", ping_timeout.as_millis()));

        let query_timeout = self.query_timeout.unwrap_or(Duration::from_secs(180));
        url_params.push(format!("query_timeout={}s", query_timeout.as_secs()));
        if let Some(min) = self.pool_min {
            url_params.push(format!("pool_min={}", min));
        }
        if let Some(max) = self.pool_max {
            url_params.push(format!("pool_max={}", max));
        }

        if !url_params.is_empty() {
            url.push('?');
            url.push_str(&url_params.join("&"));
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

        // Enable JSON-as-string mode for ClickHouse 24.10+ JSON type support
        // Note: This setting is session-scoped. New connections from the pool
        // will need to have this setting applied via enable_json_support().
        #[cfg(feature = "json")]
        {
            client
                .execute("SET output_format_native_write_json_as_string = 1")
                .await
                .map_err(|e| Error::ConnectionError(Cow::Owned(format!("Failed to enable JSON support: {}", e))))?;
        }

        let server_addr = validated.server_addr();
        let native_conn = NativeConnection::from_pool(pool, validated.database, server_addr);

        Ok(crate::Connection::Native(native_conn))
    }
}
