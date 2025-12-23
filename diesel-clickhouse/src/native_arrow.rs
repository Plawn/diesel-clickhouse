//! Native Arrow backend with true zero-copy streaming.
//!
//! This module uses `clickhouse-arrow` to provide:
//! - Native protocol (faster than HTTP)
//! - True zero-copy streaming of RecordBatches
//! - No intermediate buffering
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::native_arrow::NativeArrowConnection;
//! use futures::StreamExt;
//!
//! let conn = NativeArrowConnection::establish("localhost:9000", "default").await?;
//!
//! // Stream RecordBatches as they arrive (true zero-copy)
//! let mut stream = conn.stream_arrow("SELECT * FROM huge_table").await?;
//! while let Some(batch) = stream.next().await {
//!     let batch = batch?;
//!     // Process batch...
//! }
//! ```

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use arrow::array::RecordBatch;
use arrow::datatypes::Schema;
use clickhouse_arrow::{ArrowFormat, Client};
use futures::{Stream, StreamExt};

use crate::core::result::{Error, QueryResult};

// Re-use ArrowRow from our arrow module implementation
pub use crate::arrow::{ArrowRow, build_column_index, for_each_row};

/// Result containing Arrow RecordBatches.
#[derive(Debug)]
pub struct NativeArrowResult {
    schema: Arc<Schema>,
    batches: Vec<RecordBatch>,
    total_rows: usize,
}

impl NativeArrowResult {
    /// Create a new result from schema and batches.
    pub fn new(schema: Arc<Schema>, batches: Vec<RecordBatch>) -> Self {
        let total_rows = batches.iter().map(|b| b.num_rows()).sum();
        Self {
            schema,
            batches,
            total_rows,
        }
    }

    /// Get the schema.
    pub fn schema(&self) -> &Arc<Schema> {
        &self.schema
    }

    /// Get the record batches.
    pub fn batches(&self) -> &[RecordBatch] {
        &self.batches
    }

    /// Consume and return the batches.
    pub fn into_batches(self) -> Vec<RecordBatch> {
        self.batches
    }

    /// Get total number of rows.
    pub fn num_rows(&self) -> usize {
        self.total_rows
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.total_rows == 0
    }
}

/// A connection to ClickHouse using the native protocol with Arrow support.
///
/// This provides true zero-copy streaming of Arrow RecordBatches directly
/// from ClickHouse's native protocol, without HTTP overhead.
pub struct NativeArrowConnection {
    client: Client<ArrowFormat>,
}

impl NativeArrowConnection {
    /// Establish a connection to ClickHouse.
    ///
    /// # Arguments
    ///
    /// * `addr` - Server address (e.g., "localhost:9000")
    /// * `database` - Database name
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = NativeArrowConnection::establish("localhost:9000", "default").await?;
    /// ```
    pub async fn establish(addr: &str, database: &str) -> QueryResult<Self> {
        let client = Client::<ArrowFormat>::builder()
            .with_endpoint(addr)
            .with_database(database)
            .build()
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        Ok(Self { client })
    }

    /// Establish a connection with custom options.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let conn = NativeArrowConnection::establish_with_options(
    ///     "localhost:9000",
    ///     "default",
    ///     "user",
    ///     "password",
    /// ).await?;
    /// ```
    pub async fn establish_with_options(
        addr: &str,
        database: &str,
        user: &str,
        password: &str,
    ) -> QueryResult<Self> {
        let client = Client::<ArrowFormat>::builder()
            .with_endpoint(addr)
            .with_database(database)
            .with_username(user)
            .with_password(password)
            .build()
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        Ok(Self { client })
    }

    /// Stream Arrow RecordBatches from a query.
    ///
    /// This is the primary zero-copy streaming method. RecordBatches are
    /// streamed as they arrive from the server, with no intermediate buffering.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use futures::StreamExt;
    ///
    /// let mut stream = conn.stream_arrow("SELECT * FROM events LIMIT 1000000").await?;
    /// while let Some(batch) = stream.next().await {
    ///     let batch = batch?;
    ///     println!("Received {} rows", batch.num_rows());
    /// }
    /// ```
    pub async fn stream_arrow(&self, sql: &str) -> QueryResult<NativeArrowStream> {
        let response = self.client
            .query(sql, None)
            .await
            .map_err(|e| Error::QueryError(e.to_string()))?;

        Ok(NativeArrowStream::new(response))
    }

    /// Load all results as Arrow RecordBatches.
    ///
    /// This collects all batches into memory. For large result sets,
    /// prefer `stream_arrow()` for streaming.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = conn.load_arrow("SELECT * FROM small_table").await?;
    /// println!("Total rows: {}", result.num_rows());
    /// ```
    pub async fn load_arrow(&self, sql: &str) -> QueryResult<NativeArrowResult> {
        let mut stream = self.stream_arrow(sql).await?;
        let mut batches = Vec::new();
        let mut schema = None;

        while let Some(batch_result) = stream.next().await {
            let batch = batch_result?;
            if schema.is_none() {
                schema = Some(batch.schema());
            }
            batches.push(batch);
        }

        match schema {
            Some(s) => Ok(NativeArrowResult::new(s, batches)),
            None => Err(Error::DeserializationError("No data returned".to_string())),
        }
    }

    /// Load rows using zero-copy streaming with a callback.
    ///
    /// This method streams RecordBatches and processes them row-by-row.
    /// Each row is accessed through `ArrowRow` which provides zero-copy
    /// access to the underlying Arrow buffers.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let count = conn.load_zero_copy(
    ///     "SELECT id, name FROM users",
    ///     |row| {
    ///         let id = row.get_u64("id")?;
    ///         let name = row.get_str("name")?;  // Zero-copy!
    ///         println!("{}: {}", id, name);
    ///         Ok(())
    ///     }
    /// ).await?;
    /// ```
    pub async fn load_zero_copy<F>(&self, sql: &str, mut callback: F) -> QueryResult<usize>
    where
        F: for<'a> FnMut(ArrowRow<'a>) -> QueryResult<()>,
    {
        let mut stream = self.stream_arrow(sql).await?;
        let mut total_count = 0;
        let mut column_indices: Option<HashMap<Arc<str>, usize>> = None;

        while let Some(batch_result) = stream.next().await {
            let batch = batch_result?;

            // Build column index on first batch
            let indices = column_indices.get_or_insert_with(|| {
                build_column_index(&batch.schema())
            });

            let count = for_each_row(&batch, indices, &mut callback)?;
            total_count += count;
        }

        Ok(total_count)
    }

    /// Execute a DDL/DML statement.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// conn.execute("CREATE TABLE test (id UInt64) ENGINE = Memory").await?;
    /// ```
    pub async fn execute(&self, sql: &str) -> QueryResult<()> {
        self.client
            .execute(sql, None)
            .await
            .map_err(|e| Error::QueryError(e.to_string()))
    }
}

/// A stream of Arrow RecordBatches from the native protocol.
///
/// This implements `futures::Stream` for async iteration over batches.
pub struct NativeArrowStream {
    inner: Pin<Box<dyn Stream<Item = Result<RecordBatch, Box<dyn std::error::Error + Send + Sync>>> + Send>>,
}

impl NativeArrowStream {
    fn new<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<RecordBatch, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let mapped = stream.map(|r| r.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>));
        Self {
            inner: Box::pin(mapped),
        }
    }
}

impl Stream for NativeArrowStream {
    type Item = QueryResult<RecordBatch>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(batch))) => Poll::Ready(Some(Ok(batch))),
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Some(Err(Error::QueryError(e.to_string()))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Builder for configuring a NativeArrowConnection.
pub struct NativeArrowConnectionBuilder {
    addr: String,
    database: String,
    user: Option<String>,
    password: Option<String>,
}

impl NativeArrowConnectionBuilder {
    /// Create a new builder with the server address.
    pub fn new(addr: &str) -> Self {
        Self {
            addr: addr.to_string(),
            database: "default".to_string(),
            user: None,
            password: None,
        }
    }

    /// Set the database name.
    pub fn database(mut self, database: &str) -> Self {
        self.database = database.to_string();
        self
    }

    /// Set the username.
    pub fn user(mut self, user: &str) -> Self {
        self.user = Some(user.to_string());
        self
    }

    /// Set the password.
    pub fn password(mut self, password: &str) -> Self {
        self.password = Some(password.to_string());
        self
    }

    /// Build the connection.
    pub async fn build(self) -> QueryResult<NativeArrowConnection> {
        let mut builder = Client::<ArrowFormat>::builder()
            .with_endpoint(&self.addr)
            .with_database(&self.database);

        if let Some(user) = &self.user {
            builder = builder.with_username(user);
        }
        if let Some(password) = &self.password {
            builder = builder.with_password(password);
        }

        let client = builder
            .build()
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        Ok(NativeArrowConnection { client })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let builder = NativeArrowConnectionBuilder::new("localhost:9000")
            .database("test")
            .user("default")
            .password("secret");

        assert_eq!(builder.addr, "localhost:9000");
        assert_eq!(builder.database, "test");
        assert_eq!(builder.user, Some("default".to_string()));
        assert_eq!(builder.password, Some("secret".to_string()));
    }
}
