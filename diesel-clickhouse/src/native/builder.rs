//! Native connection builder.

use std::borrow::Cow;
use std::time::Duration;

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

use crate::common::{ConnectionParams, ConnectionBuilder};
use crate::core::result::{Error, QueryResult};

use super::{Compression, NativeConnection};

/// Builder for configuring a ClickHouse Native connection.
///
/// All connection parameters (host, port, database, user, password) are required.
/// Optional settings include compression, TLS, timeouts, and pool configuration.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::native::NativeClientBuilder;
/// use diesel_clickhouse::native::Compression;
/// use std::time::Duration;
///
/// let conn = NativeClientBuilder::new()
///     .host("localhost")
///     .port(9000)
///     .database("analytics")
///     .user("default")
///     .password("")
///     .compression(Compression::Lz4)
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
    compression: Compression,
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
    ///
    /// # Supported modes
    ///
    /// - `Compression::None` - No compression
    /// - `Compression::Lz4` - LZ4 compression
    /// - `Compression::Lz4Hc` - Falls back to LZ4 (not supported by clickhouse-rs)
    /// - `Compression::Zstd` - Falls back to None (not supported by clickhouse-rs)
    pub fn compression(mut self, compression: Compression) -> Self {
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
        use clickhouse_rs::Pool;

        use std::fmt::Write;

        let validated = self.params.validate_ref()?;

        // URL-encode user, password, and database to handle special characters
        // Only encode characters that have special meaning in URLs
        const URL_SPECIAL: &AsciiSet = &CONTROLS
            .add(b':')  // separates user:password and host:port
            .add(b'@')  // separates userinfo from host
            .add(b'/')  // path separator
            .add(b'?')  // query separator
            .add(b'#'); // fragment separator

        // Build URL directly using Display trait to avoid intermediate String allocations.
        // Pre-allocate for typical URL length (~200 chars).
        let mut url = String::with_capacity(256);
        write!(
            url,
            "tcp://{}:{}@{}:{}/{}",
            utf8_percent_encode(&validated.user, URL_SPECIAL),
            utf8_percent_encode(&validated.password, URL_SPECIAL),
            validated.host,
            validated.port,
            utf8_percent_encode(&validated.database, URL_SPECIAL)
        ).ok();

        // Build query parameters directly into URL string
        let connection_timeout = self.connection_timeout.unwrap_or(Duration::from_secs(5));
        let ping_timeout = self.ping_timeout.unwrap_or(Duration::from_secs(3));
        let query_timeout = self.query_timeout.unwrap_or(Duration::from_secs(180));

        // Always have at least the timeout params, so start with '?'
        write!(
            url,
            "?connection_timeout={}ms&ping_timeout={}ms&query_timeout={}s",
            connection_timeout.as_millis(),
            ping_timeout.as_millis(),
            query_timeout.as_secs()
        ).ok();

        if self.secure {
            url.push_str("&secure=true");
        }
        if self.skip_verify {
            url.push_str("&skip_verify=true");
        }
        // Apply compression setting
        // Note: Lz4Hc falls back to Lz4, Zstd is not supported by clickhouse-rs
        match self.compression {
            Compression::Lz4 | Compression::Lz4Hc => {
                url.push_str("&compression=lz4");
            }
            Compression::None | Compression::Zstd => {
                // Zstd not supported by clickhouse-rs, use no compression
            }
        }
        if let Some(min) = self.pool_min {
            write!(url, "&pool_min={}", min).ok();
        }
        if let Some(max) = self.pool_max {
            write!(url, "&pool_max={}", max).ok();
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

        let server_addr = validated.server_addr();

        let native_conn = NativeConnection::from_pool(pool, validated.database, server_addr);
        Ok(crate::Connection::Native(native_conn))
    }
}
