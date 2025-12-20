//! DELETE statement builder.

use std::marker::PhantomData;

use crate::backend::Backend;
use crate::expression::Expression;
use crate::query_source::Table;
use crate::result::QueryResult;
use super::{QueryFragment, AstPass};

/// A DELETE statement (ALTER TABLE ... DELETE in ClickHouse).
#[derive(Debug, Clone)]
pub struct DeleteStatement<T, W = ()> {
    table: PhantomData<T>,
    filter: W,
}

impl<T: Table> DeleteStatement<T, ()> {
    /// Create a new delete statement for a table.
    pub fn new() -> Self {
        Self {
            table: PhantomData,
            filter: (),
        }
    }
}

impl<T: Table> Default for DeleteStatement<T, ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, W> DeleteStatement<T, W>
where
    T: Table,
{
    /// Add a filter condition.
    pub fn filter<P>(self, predicate: P) -> DeleteStatement<T, P>
    where
        P: Expression,
    {
        DeleteStatement {
            table: PhantomData,
            filter: predicate,
        }
    }
}

impl<T, W, DB> QueryFragment<DB> for DeleteStatement<T, W>
where
    DB: Backend,
    T: Table + QueryFragment<DB>,
    W: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("ALTER TABLE ");
        T::table_name().walk_ast(pass.reborrow())?;
        pass.push_sql(" DELETE");

        // Check if we have a filter (W is not ())
        if std::mem::size_of::<W>() > 0 {
            pass.push_sql(" WHERE ");
            self.filter.walk_ast(pass.reborrow())?;
        }

        Ok(())
    }
}

/// Helper to create a delete statement.
pub fn delete<T: Table>(table: T) -> DeleteTarget<T> {
    DeleteTarget { table }
}

/// A target for a DELETE statement.
pub struct DeleteTarget<T> {
    #[allow(dead_code)]
    table: T,
}

impl<T: Table> DeleteTarget<T> {
    /// Filter the rows to delete.
    pub fn filter<P>(self, predicate: P) -> DeleteStatement<T, P>
    where
        P: Expression,
    {
        DeleteStatement {
            table: PhantomData,
            filter: predicate,
        }
    }
}
