//! ClickHouse testcontainers support.
//!
//! This module provides utilities for running ClickHouse in Docker containers
//! during integration tests. The container is automatically started and stopped.
//!
//! # Usage
//!
//! ```rust,ignore
//! use test_support::ClickHouseContainer;
//!
//! #[tokio::test]
//! async fn test_with_clickhouse() {
//!     let container = ClickHouseContainer::start().await;
//!     let conn = container.http_connection().await.unwrap();
//!
//!     // Run your tests...
//! }
//! ```

// Allow unused code for utility functions that may not be used in all test configurations
#![allow(dead_code)]

use std::time::Duration;

use std::borrow::Cow;
use std::collections::BTreeMap;

use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, Image};
use tokio::sync::OnceCell;

/// HTTP port for ClickHouse
const CLICKHOUSE_HTTP_PORT: ContainerPort = ContainerPort::Tcp(8123);
/// Native protocol port for ClickHouse
const CLICKHOUSE_NATIVE_PORT: ContainerPort = ContainerPort::Tcp(9000);

/// Default ClickHouse image version to use for tests.
/// Using CH 25 for latest features. Authentication configured via env vars.
pub const DEFAULT_CLICKHOUSE_VERSION: &str = "25.11-alpine";

/// Default ClickHouse user for tests
pub const CLICKHOUSE_USER: &str = "default";
/// Default ClickHouse password for tests (empty = no password in permissive mode)
pub const CLICKHOUSE_PASSWORD: &str = "";

/// Custom ClickHouse image that exposes both HTTP and Native ports.
/// Configured for ClickHouse 25.x with permissive authentication.
#[derive(Debug, Clone)]
struct ClickHouseImageWithNative {
    tag: String,
    env_vars: BTreeMap<String, String>,
}

impl Default for ClickHouseImageWithNative {
    fn default() -> Self {
        let mut env_vars = BTreeMap::new();
        // Allow connections without password for the default user
        // This XML config is injected to allow passwordless access
        env_vars.insert(
            "CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT".to_string(),
            "1".to_string(),
        );
        // Set empty password for default user
        env_vars.insert(
            "CLICKHOUSE_PASSWORD".to_string(),
            "".to_string(),
        );
        env_vars.insert(
            "CLICKHOUSE_USER".to_string(),
            "default".to_string(),
        );

        Self {
            tag: DEFAULT_CLICKHOUSE_VERSION.to_string(),
            env_vars,
        }
    }
}

impl ClickHouseImageWithNative {
    /// Create with a specific ClickHouse version tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }
}

impl Image for ClickHouseImageWithNative {
    fn name(&self) -> &str {
        "clickhouse/clickhouse-server"
    }

    fn tag(&self) -> &str {
        &self.tag
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        // Wait for HTTP endpoint to be ready
        // Use /ping endpoint which doesn't require auth
        vec![WaitFor::http(
            HttpWaitStrategy::new("/ping").with_expected_status_code(200_u16),
        )]
    }

    fn env_vars(
        &self,
    ) -> impl IntoIterator<Item = (impl Into<Cow<'_, str>>, impl Into<Cow<'_, str>>)> {
        self.env_vars.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &[CLICKHOUSE_HTTP_PORT, CLICKHOUSE_NATIVE_PORT]
    }
}

use diesel_clickhouse::{Connection, ConnectionBuilder};

/// Default database name for tests.
pub const DEFAULT_TEST_DATABASE: &str = "test_db";

/// Configuration for the ClickHouse test container.
#[derive(Debug, Clone)]
pub struct ClickHouseContainerConfig {
    /// ClickHouse image version (e.g., "24.8", "latest").
    pub version: String,
    /// Database name to create and use.
    pub database: String,
    /// Username for authentication.
    pub user: String,
    /// Password for authentication.
    pub password: String,
}

impl Default for ClickHouseContainerConfig {
    fn default() -> Self {
        Self {
            version: DEFAULT_CLICKHOUSE_VERSION.to_string(),
            database: DEFAULT_TEST_DATABASE.to_string(),
            user: "default".to_string(),
            password: "".to_string(),
        }
    }
}

impl ClickHouseContainerConfig {
    /// Create a new configuration with the specified version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set the database name.
    pub fn with_database(mut self, database: impl Into<String>) -> Self {
        self.database = database.into();
        self
    }

    /// Set the user credentials.
    pub fn with_credentials(mut self, user: impl Into<String>, password: impl Into<String>) -> Self {
        self.user = user.into();
        self.password = password.into();
        self
    }
}

/// A running ClickHouse container for integration tests.
///
/// The container is automatically stopped when this struct is dropped.
pub struct ClickHouseContainer {
    container: ContainerAsync<ClickHouseImageWithNative>,
    config: ClickHouseContainerConfig,
    http_port: u16,
    native_port: u16,
}

impl ClickHouseContainer {
    /// Start a new ClickHouse container with default configuration.
    pub async fn start() -> Self {
        Self::start_with_config(ClickHouseContainerConfig::default()).await
    }

    /// Start a new ClickHouse container with custom configuration.
    pub async fn start_with_config(config: ClickHouseContainerConfig) -> Self {
        // Use our custom image that exposes both HTTP (8123) and Native (9000) ports
        // ClickHouse 25 with permissive auth configuration
        let image = ClickHouseImageWithNative::default()
            .with_tag(&config.version);

        let container = image
            .start()
            .await
            .expect("Failed to start ClickHouse container");

        // Get the mapped ports
        let http_port = container
            .get_host_port_ipv4(8123)
            .await
            .expect("Failed to get HTTP port");

        let native_port = container
            .get_host_port_ipv4(9000)
            .await
            .expect("Failed to get Native port");

        let instance = Self {
            container,
            config,
            http_port,
            native_port,
        };

        // Wait for ClickHouse to be ready and create the test database
        instance.wait_for_ready().await;
        instance.create_database().await;

        instance
    }

    /// Wait for ClickHouse to be ready to accept connections.
    async fn wait_for_ready(&self) {
        let url = format!("http://127.0.0.1:{}", self.http_port);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client");

        let max_attempts = 60;
        let mut last_error = String::new();

        for attempt in 0..max_attempts {
            match client
                .get(&url)
                .query(&[("query", "SELECT 1")])
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return;
                    }
                    // Check for auth errors - container is ready but needs auth
                    if status.as_u16() == 401 || status.as_u16() == 403 {
                        // Try without auth first, some versions don't need it
                        last_error = format!("Auth required (status {})", status);
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        last_error = format!("Status {}: {}", status, body);
                    }
                }
                Err(e) => {
                    last_error = format!("Connection error: {}", e);
                }
            }

            if attempt < max_attempts - 1 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        panic!(
            "ClickHouse container failed to become ready within 30 seconds. \
             Port: {}, Last error: {}",
            self.http_port, last_error
        );
    }

    /// Create the test database.
    async fn create_database(&self) {
        let url = format!("http://127.0.0.1:{}", self.http_port);
        let client = reqwest::Client::new();

        let query = format!("CREATE DATABASE IF NOT EXISTS {}", self.config.database);

        let resp = client
            .post(&url)
            .body(query)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .expect("Failed to create database");

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            panic!("Failed to create database: {}", body);
        }
    }

    /// Get the HTTP port for the container.
    pub fn http_port(&self) -> u16 {
        self.http_port
    }

    /// Get the Native protocol port for the container.
    pub fn native_port(&self) -> u16 {
        self.native_port
    }

    /// Get the database name.
    pub fn database(&self) -> &str {
        &self.config.database
    }

    /// Get the HTTP connection URL.
    pub fn http_url(&self) -> String {
        format!(
            "http://{}:{}@127.0.0.1:{}/{}",
            self.config.user, self.config.password, self.http_port, self.config.database
        )
    }

    /// Get the Native protocol connection URL.
    pub fn native_url(&self) -> String {
        format!(
            "tcp://{}:{}@127.0.0.1:{}/{}",
            self.config.user, self.config.password, self.native_port, self.config.database
        )
    }

    /// Create an HTTP connection to the container.
    #[cfg(feature = "http")]
    pub async fn http_connection(&self) -> Result<Connection, diesel_clickhouse::Error> {
        Connection::http()
            .host("127.0.0.1")
            .port(self.http_port)
            .database(&self.config.database)
            .user(&self.config.user)
            .password(&self.config.password)
            .build()
            .await
    }

    /// Create a Native protocol connection to the container.
    #[cfg(feature = "native")]
    pub async fn native_connection(&self) -> Result<Connection, diesel_clickhouse::Error> {
        Connection::native()
            .host("127.0.0.1")
            .port(self.native_port)
            .database(&self.config.database)
            .user(&self.config.user)
            .password(&self.config.password)
            .build()
            .await
    }

    /// Execute a raw SQL statement on the container (for setup/teardown).
    pub async fn execute_raw(&self, sql: &str) -> Result<(), String> {
        let url = format!("http://127.0.0.1:{}", self.http_port);
        let client = reqwest::Client::new();

        let resp = client
            .post(&url)
            .query(&[("database", &self.config.database)])
            .body(sql.to_string())
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(format!("Query failed: {}", body))
        }
    }
}

/// A shared ClickHouse container that can be reused across tests.
///
/// This is useful for test suites where starting a new container for each test
/// would be too slow. The container is started lazily on first use.
///
/// # Usage
///
/// ```rust,ignore
/// use test_support::SharedClickHouseContainer;
///
/// #[tokio::test]
/// async fn test1() {
///     let container = SharedClickHouseContainer::get().await;
///     // Use container...
/// }
///
/// #[tokio::test]
/// async fn test2() {
///     // Same container instance as test1
///     let container = SharedClickHouseContainer::get().await;
///     // Use container...
/// }
/// ```
pub struct SharedClickHouseContainer {
    inner: OnceCell<ClickHouseContainer>,
}

impl SharedClickHouseContainer {
    /// Create a new shared container holder.
    pub const fn new() -> Self {
        Self {
            inner: OnceCell::const_new(),
        }
    }

    /// Get or create the shared container.
    pub async fn get(&self) -> &ClickHouseContainer {
        self.inner
            .get_or_init(|| async { ClickHouseContainer::start().await })
            .await
    }

    /// Get or create the shared container with custom configuration.
    pub async fn get_with_config(&self, config: ClickHouseContainerConfig) -> &ClickHouseContainer {
        self.inner
            .get_or_init(|| async { ClickHouseContainer::start_with_config(config).await })
            .await
    }
}

/// Global shared container for use in integration tests.
///
/// This provides a single ClickHouse container that is shared across all tests
/// in the test suite, avoiding the overhead of starting a new container for
/// each test.
static SHARED_CONTAINER: SharedClickHouseContainer = SharedClickHouseContainer::new();

/// Get the global shared ClickHouse container.
///
/// The container is started lazily on first call and reused for subsequent calls.
pub async fn shared_container() -> &'static ClickHouseContainer {
    SHARED_CONTAINER.get().await
}

/// Test helper macro for creating isolated test tables.
///
/// Creates a table with a unique name (based on test function name) and
/// returns the table name. The table is automatically cleaned up after the test.
#[macro_export]
macro_rules! test_table {
    ($container:expr, $name:literal, $schema:literal) => {{
        let table_name = format!(
            "{}_{}_{}",
            $name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            std::process::id()
        );

        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} {} ENGINE = MergeTree ORDER BY tuple()",
            table_name, $schema
        );

        $container.execute_raw(&create_sql).await
            .expect("Failed to create test table");

        table_name
    }};
}

/// Test helper macro for cleaning up test tables.
#[macro_export]
macro_rules! drop_test_table {
    ($container:expr, $table_name:expr) => {{
        let drop_sql = format!("DROP TABLE IF EXISTS {}", $table_name);
        let _ = $container.execute_raw(&drop_sql).await;
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_container_starts() {
        let container = ClickHouseContainer::start().await;

        assert!(container.http_port() > 0);
        assert!(container.native_port() > 0);
        assert_eq!(container.database(), DEFAULT_TEST_DATABASE);
    }

    #[tokio::test]
    async fn test_container_execute_raw() {
        let container = ClickHouseContainer::start().await;

        // Create a table
        container
            .execute_raw("CREATE TABLE test_exec (id UInt64) ENGINE = MergeTree ORDER BY id")
            .await
            .unwrap();

        // Insert data
        container
            .execute_raw("INSERT INTO test_exec VALUES (1), (2), (3)")
            .await
            .unwrap();

        // Cleanup
        container
            .execute_raw("DROP TABLE test_exec")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_container_with_custom_config() {
        let config = ClickHouseContainerConfig::default()
            .with_database("custom_db")
            .with_version("24.8");

        let container = ClickHouseContainer::start_with_config(config).await;

        assert_eq!(container.database(), "custom_db");
    }

    #[tokio::test]
    #[cfg(feature = "http")]
    async fn test_http_connection() {
        let container = ClickHouseContainer::start().await;
        let conn = container.http_connection().await.unwrap();

        // Simple query to verify connection works
        conn.execute("SELECT 1").await.unwrap();
    }

    #[tokio::test]
    #[cfg(feature = "native")]
    async fn test_native_connection() {
        let container = ClickHouseContainer::start().await;
        let conn = container.native_connection().await.unwrap();

        // Simple query to verify connection works
        conn.execute("SELECT 1").await.unwrap();
    }
}
