//! OVER expression for window functions.

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use crate::expression::{AppearsOnTable, Expression, SelectableExpression};

/// An expression with an OVER clause.
#[derive(Debug, Clone, Copy)]
pub struct Over<F, W> {
    function: F,
    window: W,
}

impl<F, W> Over<F, W> {
    /// Create a new OVER expression.
    pub fn new(function: F, window: W) -> Self {
        Self { function, window }
    }
}

impl<F: Expression, W> Expression for Over<F, W> {
    type SqlType = F::SqlType;
}

impl<F, W, QS> SelectableExpression<QS> for Over<F, W>
where
    F: SelectableExpression<QS>,
{
}

impl<F, W, QS> AppearsOnTable<QS> for Over<F, W>
where
    F: AppearsOnTable<QS>,
{
}

impl<F, W, DB> QueryFragment<DB> for Over<F, W>
where
    F: QueryFragment<DB>,
    W: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.function.walk_ast(pass.reborrow())?;
        pass.push_sql(" OVER (");
        self.window.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}
