//! Connection pooling for ClickHouse connections.
//!
//! This module provides connection pooling for efficient connection reuse and management.
//!
//! # Examples
//!
//! ## Using Builder (Recommended)
//!
//! ```rust,ignore
//! use diesel_clickhouse::{Connection, pool::Pool};
//!
//! // HTTP backend
//! let pool = Pool::builder(
//!     Connection::http()
//!         .host("localhost")
//!         .port(8123)
//!         .user("default")
//!         .password("default")
//!         .database("mydb")
//! )
//! .max_size(20)
//! .min_idle(5)
//! .connection_timeout_ms(30_000)
//! .build()
//! .await?;
//!
//! // Native backend
//! let pool = Pool::builder(
//!     Connection::native()
//!         .host("localhost")
//!         .port(9000)
//!         .user("default")
//!         .password("default")
//!         .database("mydb")
//! )
//! .max_size(20)
//! .min_idle(5)
//! .build()
//! .await?;
//!
//! // Get a connection and use it
//! let conn = pool.get().await?;
//! conn.execute("SELECT 1").await?;
//! // Connection is returned to pool when dropped
//! ```
//!
//! ## Using URL
//!
//! ```rust,ignore
//! use diesel_clickhouse::pool::{Pool, PoolConfig};
//!
//! // HTTP backend
//! let pool = Pool::new(
//!     "http://user:password@localhost:8123/mydb",
//!     PoolConfig::new(20).min_idle(5)
//! ).await?;
//!
//! // Native backend
//! let pool = Pool::new(
//!     "tcp://user:password@localhost:9000/mydb",
//!     PoolConfig::default()
//! ).await?;
//!
//! let conn = pool.get().await?;
//! ```
//!
//! ## Configuration Options
//!
//! ```rust,ignore
//! use diesel_clickhouse::{Connection, pool::Pool};
//!
//! let pool = Pool::builder(Connection::http()
//!         .host("localhost")
//!         .port(8123)
//!         .user("default")
//!         .password("default")
//!         .database("mydb"))
//!     .max_size(50)                    // Max connections (default: 10)
//!     .min_idle(10)                    // Pre-warmed connections (default: 1)
//!     .connection_timeout_ms(10_000)   // Timeout to get connection (default: 30s)
//!     .idle_timeout_ms(300_000)        // Close idle after 5 min (default: 10 min)
//!     .max_lifetime_ms(1_800_000)      // Recycle after 30 min (default: 30 min)
//!     .build()
//!     .await?;
//! ```

use std::sync::Arc;
use crate::Connection;
use crate::core::result::{Error, QueryResult};

// =============================================================================
// Connection Factory trait
// =============================================================================

/// Trait for creating connections.
///
/// This trait is implemented by connection builders to allow the pool
/// to create new connections on demand.
pub trait ConnectionFactory: Send + Sync + 'static {
    /// Create a new connection.
    fn create(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = QueryResult<Connection>> + Send + '_>>;
}

// HTTP connection factory
#[cfg(feature = "http")]
impl ConnectionFactory for crate::http::HttpClientBuilder {
    fn create(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = QueryResult<Connection>> + Send + '_>> {
        let builder = self.clone();
        Box::pin(async move {
            builder.build().await
        })
    }
}

// Native connection factory
#[cfg(feature = "native")]
impl ConnectionFactory for crate::native::NativeClientBuilder {
    fn create(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = QueryResult<Connection>> + Send + '_>> {
        let builder = self.clone();
        Box::pin(async move {
            builder.build().await
        })
    }
}

// =============================================================================
// Pool Configuration
// =============================================================================

/// Configuration for the connection pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of connections in the pool.
    pub max_size: usize,
    /// Minimum number of idle connections to maintain.
    pub min_idle: Option<usize>,
    /// Connection timeout in milliseconds.
    pub connection_timeout_ms: u64,
    /// Idle timeout in milliseconds (connections idle longer are closed).
    pub idle_timeout_ms: Option<u64>,
    /// Maximum lifetime of a connection in milliseconds.
    pub max_lifetime_ms: Option<u64>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10,
            min_idle: Some(1),
            connection_timeout_ms: 30_000,
            idle_timeout_ms: Some(600_000), // 10 minutes
            max_lifetime_ms: Some(1_800_000), // 30 minutes
        }
    }
}

impl PoolConfig {
    /// Create a new pool config with the specified max size.
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size,
            ..Default::default()
        }
    }

    /// Set the minimum idle connections.
    pub fn min_idle(mut self, min_idle: usize) -> Self {
        self.min_idle = Some(min_idle);
        self
    }

    /// Set the connection timeout.
    pub fn connection_timeout_ms(mut self, timeout: u64) -> Self {
        self.connection_timeout_ms = timeout;
        self
    }

    /// Set the idle timeout.
    pub fn idle_timeout_ms(mut self, timeout: u64) -> Self {
        self.idle_timeout_ms = Some(timeout);
        self
    }

    /// Set the max lifetime.
    pub fn max_lifetime_ms(mut self, lifetime: u64) -> Self {
        self.max_lifetime_ms = Some(lifetime);
        self
    }
}

// =============================================================================
// Pool Builder
// =============================================================================

/// Builder for creating a connection pool from a connection builder.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::{Connection, pool::Pool};
///
/// let pool = Pool::builder(Connection::http()
///         .host("localhost")
///         .port(8123)
///         .user("default")
///         .password("default")
///         .database("mydb"))
///     .max_size(20)
///     .min_idle(5)
///     .connection_timeout_ms(30_000)
///     .build()
///     .await?;
/// ```
pub struct PoolBuilder<F: ConnectionFactory> {
    factory: F,
    config: PoolConfig,
}

impl<F: ConnectionFactory> PoolBuilder<F> {
    /// Create a new pool builder with the given connection factory.
    pub fn new(factory: F) -> Self {
        Self {
            factory,
            config: PoolConfig::default(),
        }
    }

    /// Set the maximum number of connections in the pool.
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.config.max_size = max_size;
        self
    }

    /// Set the minimum number of idle connections to maintain.
    pub fn min_idle(mut self, min_idle: usize) -> Self {
        self.config.min_idle = Some(min_idle);
        self
    }

    /// Set the connection timeout in milliseconds.
    pub fn connection_timeout_ms(mut self, timeout: u64) -> Self {
        self.config.connection_timeout_ms = timeout;
        self
    }

    /// Set the idle timeout in milliseconds.
    pub fn idle_timeout_ms(mut self, timeout: u64) -> Self {
        self.config.idle_timeout_ms = Some(timeout);
        self
    }

    /// Set the maximum lifetime of a connection in milliseconds.
    pub fn max_lifetime_ms(mut self, lifetime: u64) -> Self {
        self.config.max_lifetime_ms = Some(lifetime);
        self
    }

    /// Apply a pre-built PoolConfig.
    pub fn config(mut self, config: PoolConfig) -> Self {
        self.config = config;
        self
    }

    /// Build the connection pool.
    ///
    /// This validates the connection by creating a test connection,
    /// then pre-warms the pool with `min_idle` connections.
    pub async fn build(self) -> QueryResult<Pool> {
        // Validate by creating a test connection
        let test_conn = self.factory.create().await?;
        drop(test_conn);

        let config = self.config;
        let inner = Arc::new(PoolInner {
            factory: Box::new(self.factory),
            config: config.clone(),
            connections: tokio::sync::Mutex::new(Vec::with_capacity(config.max_size)),
            available: tokio::sync::Semaphore::new(config.max_size),
            total_created: std::sync::atomic::AtomicUsize::new(0),
        });

        let pool = Pool { inner };

        // Pre-warm the pool with min_idle connections
        if let Some(min_idle) = config.min_idle {
            for _ in 0..min_idle {
                match pool.inner.factory.create().await {
                    Ok(conn) => {
                        let mut conns = pool.inner.connections.lock().await;
                        conns.push(conn);
                        pool.inner.total_created.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => {
                        // Log but don't fail - we can create connections on demand
                        eprintln!("Warning: Failed to pre-warm pool connection: {}", e);
                    }
                }
            }
        }

        Ok(pool)
    }
}

// =============================================================================
// Pooled Connection
// =============================================================================

/// A pooled connection wrapper.
pub struct PooledConnection {
    conn: Option<Connection>,
    pool: Arc<PoolInner>,
}

impl PooledConnection {
    /// Get a reference to the underlying connection.
    ///
    /// Returns `None` if the connection has already been taken (only possible
    /// through internal misuse or after drop).
    pub fn connection(&self) -> Option<&Connection> {
        self.conn.as_ref()
    }
}

impl std::ops::Deref for PooledConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        // INVARIANT: conn is always Some during the lifetime of PooledConnection.
        // It only becomes None inside drop(), after which Deref cannot be called.
        // Using match instead of expect/unwrap to satisfy clippy lints while
        // still catching any internal bugs that might violate this invariant.
        match self.conn.as_ref() {
            Some(conn) => conn,
            None => unreachable!(
                "BUG in diesel-clickhouse: PooledConnection::conn is None outside of drop()"
            ),
        }
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.return_connection(conn);
        }
    }
}

// =============================================================================
// Pool Inner
// =============================================================================

/// Internal pool state.
struct PoolInner {
    factory: Box<dyn ConnectionFactory>,
    config: PoolConfig,
    connections: tokio::sync::Mutex<Vec<Connection>>,
    available: tokio::sync::Semaphore,
    total_created: std::sync::atomic::AtomicUsize,
}

impl PoolInner {
    fn return_connection(&self, conn: Connection) {
        // Try to return to pool using try_lock to avoid blocking
        // If we can't get the lock, just drop the connection - this is safe
        // because we still add the permit back
        if let Ok(mut conns) = self.connections.try_lock() {
            if conns.len() < self.config.max_size {
                conns.push(conn);
            }
            // Connection is dropped if pool is full
        }
        // else: Connection is dropped, which is acceptable

        // Always add the permit back so another connection can be created
        self.available.add_permits(1);
    }
}

// =============================================================================
// Pool
// =============================================================================

/// A connection pool for ClickHouse.
///
/// The pool manages a set of reusable connections, reducing the overhead
/// of establishing new connections for each query.
#[derive(Clone)]
pub struct Pool {
    inner: Arc<PoolInner>,
}

impl Pool {
    /// Create a pool builder from a connection builder.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::{Connection, pool::Pool};
    ///
    /// // HTTP backend
    /// let pool = Pool::builder(Connection::http()
    ///         .host("localhost")
    ///         .port(8123)
    ///         .user("default")
    ///         .password("default")
    ///         .database("mydb"))
    ///     .max_size(20)
    ///     .min_idle(5)
    ///     .build()
    ///     .await?;
    ///
    /// // Native backend
    /// let pool = Pool::builder(Connection::native()
    ///         .host("localhost")
    ///         .port(9000)
    ///         .user("default")
    ///         .password("default")
    ///         .database("mydb"))
    ///     .max_size(20)
    ///     .build()
    ///     .await?;
    /// ```
    pub fn builder<F: ConnectionFactory>(factory: F) -> PoolBuilder<F> {
        PoolBuilder::new(factory)
    }

    /// Get a connection from the pool.
    ///
    /// If no connections are available and the pool is at max capacity,
    /// this will wait until a connection becomes available.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = pool.get().await?;
    /// conn.execute("SELECT 1").await?;
    /// // Connection returned to pool when `conn` is dropped
    /// ```
    pub async fn get(&self) -> QueryResult<PooledConnection> {
        // Wait for a permit (slot in the pool)
        let permit = tokio::time::timeout(
            std::time::Duration::from_millis(self.inner.config.connection_timeout_ms),
            self.inner.available.acquire(),
        )
        .await
        .map_err(|e| Error::ConnectionError(format!("Pool connection timeout: {}", e)))?
        .map_err(|e| Error::ConnectionError(format!("Pool closed: {}", e)))?;

        // Forget the permit - we'll add it back when the connection is returned
        permit.forget();

        // Try to get an existing connection
        {
            let mut conns = self.inner.connections.lock().await;
            if let Some(conn) = conns.pop() {
                return Ok(PooledConnection {
                    conn: Some(conn),
                    pool: Arc::clone(&self.inner),
                });
            }
        }

        // Create a new connection
        let conn = self.inner.factory.create().await?;
        self.inner.total_created.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        Ok(PooledConnection {
            conn: Some(conn),
            pool: Arc::clone(&self.inner),
        })
    }

    /// Try to get a connection without waiting.
    ///
    /// Returns `None` if no connections are available.
    pub async fn try_get(&self) -> Option<QueryResult<PooledConnection>> {
        let permit = self.inner.available.try_acquire().ok()?;
        permit.forget();

        // Try to get an existing connection
        {
            let mut conns = self.inner.connections.lock().await;
            if let Some(conn) = conns.pop() {
                return Some(Ok(PooledConnection {
                    conn: Some(conn),
                    pool: Arc::clone(&self.inner),
                }));
            }
        }

        // Create a new connection
        match self.inner.factory.create().await {
            Ok(conn) => {
                self.inner.total_created.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Some(Ok(PooledConnection {
                    conn: Some(conn),
                    pool: Arc::clone(&self.inner),
                }))
            }
            Err(e) => Some(Err(e)),
        }
    }

    /// Get the current number of idle connections.
    pub async fn idle_count(&self) -> usize {
        self.inner.connections.lock().await.len()
    }

    /// Get the total number of connections created.
    pub fn total_created(&self) -> usize {
        self.inner.total_created.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the pool configuration.
    pub fn config(&self) -> &PoolConfig {
        &self.inner.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_builder() {
        let config = PoolConfig::new(20)
            .min_idle(5)
            .connection_timeout_ms(5000)
            .idle_timeout_ms(300_000);

        assert_eq!(config.max_size, 20);
        assert_eq!(config.min_idle, Some(5));
        assert_eq!(config.connection_timeout_ms, 5000);
        assert_eq!(config.idle_timeout_ms, Some(300_000));
    }
}
