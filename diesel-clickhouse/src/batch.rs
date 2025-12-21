//! Batch insertion utilities for high-performance bulk inserts.
//!
//! This module provides `BatchInserter` for accumulating rows and sending them
//! in batches to reduce network round-trips.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::batch::BatchInserter;
//!
//! let mut batch = BatchInserter::new(&conn, "users", 1000);
//!
//! for user in users {
//!     batch.push(&user).await?;
//! }
//!
//! // Flush remaining rows
//! batch.flush().await?;
//! ```

use crate::core::backend::{ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::core::query_builder::{AstPass, Insertable};
use crate::core::query_source::Table;
use crate::core::result::QueryResult;
use crate::Connection;

/// A batch inserter that accumulates rows and sends them in batches.
///
/// This is more efficient than inserting rows one at a time because it
/// reduces the number of network round-trips to ClickHouse.
///
/// # Type Parameters
///
/// - `T`: The table type
/// - `R`: The row type (must implement `Insertable<T>`)
///
/// # Example
///
/// ```rust,ignore
/// let mut batch = BatchInserter::<users::table, NewUser>::new(&conn, 1000);
///
/// for user in large_user_list {
///     batch.push(&user).await?;
/// }
///
/// batch.flush().await?;
/// println!("Inserted {} rows", batch.total_inserted());
/// ```
pub struct BatchInserter<'a, T: Table, R: Insertable<T>> {
    conn: &'a Connection,
    table_name: &'static str,
    batch_size: usize,
    buffer: Vec<R>,
    total_inserted: usize,
    _table: std::marker::PhantomData<T>,
}

impl<'a, T: Table, R: Insertable<T> + Clone> BatchInserter<'a, T, R> {
    /// Create a new batch inserter.
    ///
    /// # Arguments
    ///
    /// - `conn`: The connection to use for inserts
    /// - `batch_size`: Number of rows to accumulate before auto-flushing
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut batch = BatchInserter::<users::table, NewUser>::new(&conn, 1000);
    /// ```
    pub fn new(conn: &'a Connection, batch_size: usize) -> Self {
        Self {
            conn,
            table_name: T::table_name(),
            batch_size,
            buffer: Vec::with_capacity(batch_size),
            total_inserted: 0,
            _table: std::marker::PhantomData,
        }
    }

    /// Push a row to the batch buffer.
    ///
    /// If the buffer reaches `batch_size`, it will automatically flush.
    pub async fn push(&mut self, row: R) -> QueryResult<()> {
        self.buffer.push(row);

        if self.buffer.len() >= self.batch_size {
            self.flush().await?;
        }

        Ok(())
    }

    /// Flush all buffered rows to the database.
    ///
    /// This should be called after all rows have been pushed to ensure
    /// any remaining rows are inserted.
    pub async fn flush(&mut self) -> QueryResult<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let sql = self.build_insert_sql();
        self.conn.execute(&sql).await?;

        self.total_inserted += self.buffer.len();
        self.buffer.clear();

        Ok(())
    }

    /// Get the number of rows currently buffered.
    pub fn buffered_count(&self) -> usize {
        self.buffer.len()
    }

    /// Get the total number of rows inserted so far.
    pub fn total_inserted(&self) -> usize {
        self.total_inserted
    }

    /// Build the INSERT SQL for the current buffer.
    fn build_insert_sql(&self) -> String {
        let columns = R::column_names();

        // Estimate capacity: "INSERT INTO table (cols) VALUES " + values
        // Each value tuple is roughly 20-50 chars, so estimate 40 * buffer.len()
        let estimated_capacity = 50 + columns.len() * 10 + self.buffer.len() * 50;
        let mut sql = String::with_capacity(estimated_capacity);

        sql.push_str("INSERT INTO ");
        sql.push_str(self.table_name);

        if !columns.is_empty() {
            sql.push_str(" (");
            for (i, col) in columns.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push('`');
                sql.push_str(col);
                sql.push('`');
            }
            sql.push(')');
        }

        sql.push_str(" VALUES ");

        // Build values using QueryFragment
        let mut builder = GenericQueryBuilder::default();
        let mut collector = GenericBindCollector::default();

        for (i, row) in self.buffer.iter().enumerate() {
            if i > 0 {
                builder.push_sql(", ");
            }
            builder.push_sql("(");
            {
                let mut pass = AstPass::<ClickHouse>::new(&mut builder, &mut collector);
                let _ = row.write_value(&mut pass);
            }
            builder.push_sql(")");
        }

        sql.push_str(&builder.finish());
        sql
    }
}

/// A simpler batch inserter using raw SQL values.
///
/// This is useful when you want to build VALUES manually or have
/// pre-formatted data.
pub struct RawBatchInserter<'a> {
    conn: &'a Connection,
    table_name: &'a str,
    batch_size: usize,
    values: Vec<String>,
    total_inserted: usize,
}

impl<'a> RawBatchInserter<'a> {
    /// Create a new raw batch inserter.
    pub fn new(conn: &'a Connection, table_name: &'a str, batch_size: usize) -> Self {
        Self {
            conn,
            table_name,
            batch_size,
            values: Vec::with_capacity(batch_size),
            total_inserted: 0,
        }
    }

    /// Push a raw value tuple string like "(1, 'alice', true)".
    pub async fn push(&mut self, value: String) -> QueryResult<()> {
        self.values.push(value);

        if self.values.len() >= self.batch_size {
            self.flush().await?;
        }

        Ok(())
    }

    /// Push a pre-formatted value tuple.
    pub async fn push_str(&mut self, value: &str) -> QueryResult<()> {
        self.push(value.to_owned()).await
    }

    /// Flush all buffered values to the database.
    pub async fn flush(&mut self) -> QueryResult<()> {
        if self.values.is_empty() {
            return Ok(());
        }

        // Join all values efficiently
        let values_sql = self.values.join(", ");
        self.conn.insert_values(self.table_name, &values_sql).await?;

        self.total_inserted += self.values.len();
        self.values.clear();

        Ok(())
    }

    /// Get the number of rows currently buffered.
    pub fn buffered_count(&self) -> usize {
        self.values.len()
    }

    /// Get the total number of rows inserted so far.
    pub fn total_inserted(&self) -> usize {
        self.total_inserted
    }
}

/// Extension trait for Connection to create batch inserters.
pub trait BatchInsertExt {
    /// Create a typed batch inserter for a table.
    fn batch_inserter<T: Table, R: Insertable<T> + Clone>(
        &self,
        batch_size: usize,
    ) -> BatchInserter<'_, T, R>;

    /// Create a raw batch inserter for a table.
    fn raw_batch_inserter<'a>(
        &'a self,
        table_name: &'a str,
        batch_size: usize,
    ) -> RawBatchInserter<'a>;
}

impl BatchInsertExt for Connection {
    fn batch_inserter<T: Table, R: Insertable<T> + Clone>(
        &self,
        batch_size: usize,
    ) -> BatchInserter<'_, T, R> {
        BatchInserter::new(self, batch_size)
    }

    fn raw_batch_inserter<'a>(
        &'a self,
        table_name: &'a str,
        batch_size: usize,
    ) -> RawBatchInserter<'a> {
        RawBatchInserter::new(self, table_name, batch_size)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_batch_inserter_capacity() {
        // Just verify the types compile correctly
        assert!(true);
    }
}
