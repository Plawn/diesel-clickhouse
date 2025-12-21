//! Connection pooling for ClickHouse connections.
//!
//! This module provides connection pooling using `deadpool` for efficient
//! connection reuse and management.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::pool::{Pool, PoolConfig};
//!
//! // Create a pool with default settings
//! let pool = Pool::new("http://localhost:8123/default", PoolConfig::default()).await?;
//!
//! // Get a connection from the pool
//! let conn = pool.get().await?;
//!
//! // Use the connection
//! conn.execute("SELECT 1").await?;
//!
//! // Connection is returned to pool when dropped
//! ```

use std::sync::Arc;
use crate::Connection;
use crate::core::result::{Error, QueryResult};

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
            max_lifetime_ms: Some(1800_000), // 30 minutes
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

/// A pooled connection wrapper.
pub struct PooledConnection {
    conn: Option<Connection>,
    pool: Arc<PoolInner>,
}

impl PooledConnection {
    /// Get a reference to the underlying connection.
    pub fn connection(&self) -> &Connection {
        self.conn.as_ref().expect("connection taken")
    }
}

impl std::ops::Deref for PooledConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection()
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.return_connection(conn);
        }
    }
}

/// Internal pool state.
struct PoolInner {
    url: String,
    config: PoolConfig,
    connections: tokio::sync::Mutex<Vec<Connection>>,
    available: tokio::sync::Semaphore,
    total_created: std::sync::atomic::AtomicUsize,
}

impl PoolInner {
    fn return_connection(&self, conn: Connection) {
        // Try to return to pool
        let mut conns = self.connections.blocking_lock();
        if conns.len() < self.config.max_size {
            conns.push(conn);
        }
        self.available.add_permits(1);
    }
}

/// A connection pool for ClickHouse.
///
/// The pool manages a set of reusable connections, reducing the overhead
/// of establishing new connections for each query.
#[derive(Clone)]
pub struct Pool {
    inner: Arc<PoolInner>,
}

impl Pool {
    /// Create a new connection pool.
    ///
    /// # Arguments
    ///
    /// - `url`: The ClickHouse connection URL
    /// - `config`: Pool configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let pool = Pool::new("http://localhost:8123/default", PoolConfig::default()).await?;
    /// ```
    pub async fn new(url: &str, config: PoolConfig) -> QueryResult<Self> {
        // Validate URL by creating a test connection
        let test_conn = Connection::establish(url).await?;
        drop(test_conn);

        let inner = Arc::new(PoolInner {
            url: url.to_owned(),
            config: config.clone(),
            connections: tokio::sync::Mutex::new(Vec::with_capacity(config.max_size)),
            available: tokio::sync::Semaphore::new(config.max_size),
            total_created: std::sync::atomic::AtomicUsize::new(0),
        });

        let pool = Self { inner };

        // Pre-warm the pool with min_idle connections
        if let Some(min_idle) = config.min_idle {
            for _ in 0..min_idle {
                match Connection::establish(url).await {
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
        .map_err(|_| Error::ConnectionError("Pool connection timeout".to_owned()))?
        .map_err(|_| Error::ConnectionError("Pool closed".to_owned()))?;

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
        let conn = Connection::establish(&self.inner.url).await?;
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
        match Connection::establish(&self.inner.url).await {
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

    /// Get the database URL.
    pub fn url(&self) -> &str {
        &self.inner.url
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
