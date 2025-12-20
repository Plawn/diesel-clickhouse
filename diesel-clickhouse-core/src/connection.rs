//! Async connection traits for ClickHouse.

use crate::backend::Backend;
use crate::deserialize::FromRow;
use crate::query_builder::QueryFragment;
use crate::result::QueryResult;

/// Async connection trait for ClickHouse.
#[async_trait::async_trait]
pub trait AsyncConnection: Send + Sized {
    /// The backend type for this connection.
    type Backend: Backend;

    /// Establish a new connection.
    async fn establish(url: &str) -> QueryResult<Self>;

    /// Execute a raw SQL query.
    async fn execute(&mut self, sql: &str) -> QueryResult<()>;

    /// Execute a query and load results.
    async fn load<T, U>(&mut self, query: T) -> QueryResult<Vec<U>>
    where
        T: QueryFragment<Self::Backend> + Send,
        U: FromRow + Send;

    /// Execute a query and return affected row count.
    async fn execute_query<T>(&mut self, query: T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + Send;

    /// Begin a batch insert operation.
    fn batch_insert<T>(&mut self, table_name: &str) -> BatchInserter<'_, Self>
    where
        Self: Sized,
    {
        BatchInserter::new(table_name)
    }

    /// Ping the connection to verify it's alive.
    async fn ping(&mut self) -> QueryResult<()> {
        self.execute("SELECT 1").await
    }
}

/// A batch insert helper.
pub struct BatchInserter<'a, C> {
    table_name: String,
    _conn: std::marker::PhantomData<&'a C>,
}

impl<'a, C: AsyncConnection> BatchInserter<'a, C> {
    /// Create a new batch inserter.
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            _conn: std::marker::PhantomData,
        }
    }

    /// Get the table name.
    pub fn table_name(&self) -> &str {
        &self.table_name
    }
}

/// Settings for a connection.
#[derive(Debug, Clone)]
pub struct ConnectionSettings {
    /// Maximum number of retries for failed queries.
    pub max_retries: u32,
    /// Timeout for queries in seconds.
    pub query_timeout_secs: u64,
    /// Whether to enable query logging.
    pub log_queries: bool,
    /// Whether to use async insert mode.
    pub async_insert: bool,
    /// Whether to wait for async insert completion.
    pub wait_for_async_insert: bool,
    /// Compression algorithm.
    pub compression: Compression,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            max_retries: 3,
            query_timeout_secs: 300,
            log_queries: false,
            async_insert: false,
            wait_for_async_insert: true,
            compression: Compression::Lz4,
        }
    }
}

/// Compression algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// No compression.
    None,
    /// LZ4 compression (recommended).
    Lz4,
    /// LZ4 high compression.
    Lz4Hc,
    /// Zstandard compression.
    Zstd,
}

/// A pool of connections.
#[cfg(feature = "pool")]
pub struct ConnectionPool<C: AsyncConnection> {
    _conn: std::marker::PhantomData<C>,
}

/// Transaction handle (limited support in ClickHouse).
pub struct Transaction<'a, C: AsyncConnection> {
    conn: &'a mut C,
    committed: bool,
}

impl<'a, C: AsyncConnection> Transaction<'a, C> {
    /// Create a new transaction.
    pub fn new(conn: &'a mut C) -> Self {
        Self {
            conn,
            committed: false,
        }
    }

    /// Commit the transaction.
    pub async fn commit(mut self) -> QueryResult<()> {
        // ClickHouse has limited transaction support
        self.committed = true;
        Ok(())
    }

    /// Rollback the transaction.
    pub async fn rollback(self) -> QueryResult<()> {
        // ClickHouse has limited transaction support
        Ok(())
    }

    /// Get the inner connection.
    pub fn conn(&mut self) -> &mut C {
        self.conn
    }
}

impl<'a, C: AsyncConnection> Drop for Transaction<'a, C> {
    fn drop(&mut self) {
        if !self.committed {
            // Would rollback here if supported
        }
    }
}
