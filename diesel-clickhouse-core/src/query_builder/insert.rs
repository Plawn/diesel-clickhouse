//! INSERT statement builder.

use std::marker::PhantomData;

use crate::backend::Backend;
use crate::query_source::Table;
use crate::result::QueryResult;
use super::{QueryFragment, AstPass};

/// An INSERT statement builder.
#[derive(Debug)]
pub struct InsertStatement<T: Table, V> {
    table: PhantomData<T>,
    values: V,
}

impl<T: Table, V> InsertStatement<T, V> {
    /// Create a new INSERT statement.
    pub fn new(values: V) -> Self {
        Self {
            table: PhantomData,
            values,
        }
    }
}

/// Start building an INSERT statement.
pub fn insert_into<T: Table>(_table: T) -> InsertInto<T> {
    InsertInto {
        _table: PhantomData,
    }
}

/// Builder for INSERT statements.
#[derive(Debug, Clone, Copy)]
pub struct InsertInto<T: Table> {
    _table: PhantomData<T>,
}

impl<T: Table> InsertInto<T> {
    /// Specify the values to insert.
    pub fn values<V>(self, values: V) -> InsertStatement<T, V>
    where
        V: Insertable<T>,
    {
        InsertStatement::new(values)
    }

    /// Create a batch insert operation.
    pub fn batch(self) -> BatchInsertBuilder<T> {
        BatchInsertBuilder::new()
    }
}

/// Trait for types that can be inserted into a table.
pub trait Insertable<T: Table> {
    /// The type of values to insert.
    type Values;

    /// Get the column names for this insert.
    fn column_names() -> &'static [&'static str];

    /// Convert to insertable values.
    fn values(self) -> Self::Values;
}

/// Batch insert builder for efficient bulk inserts.
#[derive(Debug)]
pub struct BatchInsertBuilder<T: Table> {
    _table: PhantomData<T>,
}

impl<T: Table> BatchInsertBuilder<T> {
    /// Create a new batch insert builder.
    pub fn new() -> Self {
        Self {
            _table: PhantomData,
        }
    }
}

impl<T: Table> Default for BatchInsertBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A batch of rows to insert.
#[derive(Debug)]
pub struct BatchInsert<T: Table, R> {
    _table: PhantomData<T>,
    rows: Vec<R>,
    chunk_size: usize,
}

impl<T: Table, R> BatchInsert<T, R> {
    /// Create a new batch insert.
    pub fn new() -> Self {
        Self {
            _table: PhantomData,
            rows: Vec::new(),
            chunk_size: 10_000,
        }
    }

    /// Set the chunk size for batch inserts.
    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Add rows to the batch.
    pub fn values(mut self, rows: impl IntoIterator<Item = R>) -> Self {
        self.rows.extend(rows);
        self
    }

    /// Add a single row to the batch.
    pub fn push(mut self, row: R) -> Self {
        self.rows.push(row);
        self
    }

    /// Get the number of rows in the batch.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Check if the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Get the chunk size.
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Consume and return the rows.
    pub fn into_rows(self) -> Vec<R> {
        self.rows
    }
}

impl<T: Table, R> Default for BatchInsert<T, R> {
    fn default() -> Self {
        Self::new()
    }
}

// QueryFragment for InsertStatement
impl<T, V, DB> QueryFragment<DB> for InsertStatement<T, V>
where
    T: Table,
    V: Insertable<T>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("INSERT INTO ");
        pass.push_identifier(T::table_name());

        // Column names
        let columns = V::column_names();
        if !columns.is_empty() {
            pass.push_sql(" (");
            for (i, col) in columns.iter().enumerate() {
                if i > 0 {
                    pass.push_sql(", ");
                }
                pass.push_identifier(col);
            }
            pass.push_sql(")");
        }

        pass.push_sql(" VALUES ");
        // Values would be serialized here
        // For now, just placeholder
        pass.push_sql("(?)");

        Ok(())
    }
}
