//! Native Arrow backend for zero-copy streaming.
//!
//! This module uses `clickhouse-arrow` to provide zero-copy row iteration
//! over query results using the native protocol.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::native::NativeClientBuilder;
//!
//! let conn = NativeClientBuilder::new()
//!     .host("localhost")
//!     .port(9000)
//!     .database("default")
//!     .user("default")
//!     .password("")
//!     .build()
//!     .await?;
//!
//! // Zero-copy iteration over rows
//! let count = conn.load_zero_copy(
//!     "SELECT id, name FROM users",
//!     |row| {
//!         let id = row.get_u64("id")?;
//!         let name = row.get_str("name")?;  // Zero-copy borrow!
//!         println!("{}: {}", id, name);
//!         Ok(())
//!     }
//! ).await?;
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::RecordBatch;
use clickhouse_arrow::{ArrowFormat, Client};
use futures::StreamExt;

use crate::core::result::{Error, QueryResult};

// Re-use ArrowRow from our arrow module
pub use crate::arrow::{ArrowRow, build_column_index, for_each_row};

/// A connection to ClickHouse using the native protocol with Arrow support.
///
/// This is used internally by `NativeConnection` for zero-copy streaming.
pub(crate) struct NativeArrowConnection {
    client: Client<ArrowFormat>,
}

impl NativeArrowConnection {
    /// Establish a connection to ClickHouse.
    pub async fn establish(
        addr: &str,
        database: &str,
        user: &str,
        password: &str,
    ) -> QueryResult<Self> {
        let mut builder = Client::<ArrowFormat>::builder()
            .with_endpoint(addr)
            .with_database(database);

        if !user.is_empty() {
            builder = builder.with_username(user);
        }
        if !password.is_empty() {
            builder = builder.with_password(password);
        }

        let client = builder
            .build()
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        Ok(Self { client })
    }

    /// Load rows using zero-copy streaming with a callback.
    ///
    /// This method streams RecordBatches and processes them row-by-row.
    /// Each row is accessed through `ArrowRow` which provides zero-copy
    /// access to the underlying Arrow buffers.
    pub async fn load_zero_copy<F>(&self, sql: &str, mut callback: F) -> QueryResult<usize>
    where
        F: for<'a> FnMut(ArrowRow<'a>) -> QueryResult<()>,
    {
        let mut stream = self.client
            .query(sql, None)
            .await
            .map_err(|e| Error::QueryError(e.to_string()))?;

        let mut total_count = 0;
        let mut column_indices: Option<HashMap<Arc<str>, usize>> = None;

        while let Some(batch_result) = stream.next().await {
            let batch: RecordBatch = batch_result
                .map_err(|e| Error::QueryError(e.to_string()))?;

            // Build column index on first batch
            let indices = column_indices.get_or_insert_with(|| {
                build_column_index(&batch.schema())
            });

            let count = for_each_row(&batch, indices, &mut callback)?;
            total_count += count;
        }

        Ok(total_count)
    }
}
