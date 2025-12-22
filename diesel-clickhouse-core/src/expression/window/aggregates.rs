//! Window aggregate functions (SUM, AVG, COUNT, MIN, MAX as window functions).
//!
//! This module provides aggregate functions that can be used with OVER clauses.

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use super::over::Over;
use crate::expression::{AppearsOnTable, Expression, SelectableExpression};

// =============================================================================
// Window-compatible aggregate wrappers
// =============================================================================

/// A wrapper that makes an aggregate function usable as a window function.
#[derive(Debug, Clone, Copy)]
pub struct WindowAggregate<A> {
    aggregate: A,
}

impl<A> WindowAggregate<A> {
    /// Create a new window aggregate.
    pub fn new(aggregate: A) -> Self {
        Self { aggregate }
    }

    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

impl<A: Expression> Expression for WindowAggregate<A> {
    type SqlType = A::SqlType;
}

impl<A, QS> SelectableExpression<QS> for WindowAggregate<A>
where
    A: SelectableExpression<QS>,
{
}

impl<A, QS> AppearsOnTable<QS> for WindowAggregate<A>
where
    A: AppearsOnTable<QS>,
{
}

impl<A, DB> QueryFragment<DB> for WindowAggregate<A>
where
    A: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.aggregate.walk_ast(pass)
    }
}

/// Extension trait for using aggregates as window functions.
pub trait WindowAggregateExt: Sized {
    /// Convert this aggregate to a window aggregate.
    fn as_window(self) -> WindowAggregate<Self> {
        WindowAggregate::new(self)
    }
}

// Blanket implementation for all expressions
impl<T: Expression> WindowAggregateExt for T {}

// =============================================================================
// SUM window function
// =============================================================================

/// SUM() as a window function.
#[derive(Debug, Clone, Copy)]
pub struct SumWindow<E> {
    expr: E,
}

/// Create a SUM window function.
pub fn sum_over<E>(expr: E) -> SumWindow<E>
where
    E: Expression,
{
    SumWindow { expr }
}

impl<E: Expression> Expression for SumWindow<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for SumWindow<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for SumWindow<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for SumWindow<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("sum(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E> SumWindow<E> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// AVG window function
// =============================================================================

/// AVG() as a window function.
#[derive(Debug, Clone, Copy)]
pub struct AvgWindow<E> {
    expr: E,
}

/// Create an AVG window function.
pub fn avg_over<E>(expr: E) -> AvgWindow<E>
where
    E: Expression,
{
    AvgWindow { expr }
}

impl<E: Expression> Expression for AvgWindow<E> {
    type SqlType = diesel_clickhouse_types::Float64;
}

impl<E, QS> SelectableExpression<QS> for AvgWindow<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for AvgWindow<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for AvgWindow<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("avg(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E> AvgWindow<E> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// COUNT window function
// =============================================================================

/// COUNT() as a window function.
#[derive(Debug, Clone, Copy)]
pub struct CountWindow<E> {
    expr: E,
}

/// Create a COUNT window function.
pub fn count_over<E>(expr: E) -> CountWindow<E>
where
    E: Expression,
{
    CountWindow { expr }
}

impl<E: Expression> Expression for CountWindow<E> {
    type SqlType = diesel_clickhouse_types::UInt64;
}

impl<E, QS> SelectableExpression<QS> for CountWindow<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for CountWindow<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for CountWindow<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("count(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E> CountWindow<E> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// MIN window function
// =============================================================================

/// MIN() as a window function.
#[derive(Debug, Clone, Copy)]
pub struct MinWindow<E> {
    expr: E,
}

/// Create a MIN window function.
pub fn min_over<E>(expr: E) -> MinWindow<E>
where
    E: Expression,
{
    MinWindow { expr }
}

impl<E: Expression> Expression for MinWindow<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for MinWindow<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for MinWindow<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for MinWindow<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("min(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E> MinWindow<E> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// MAX window function
// =============================================================================

/// MAX() as a window function.
#[derive(Debug, Clone, Copy)]
pub struct MaxWindow<E> {
    expr: E,
}

/// Create a MAX window function.
pub fn max_over<E>(expr: E) -> MaxWindow<E>
where
    E: Expression,
{
    MaxWindow { expr }
}

impl<E: Expression> Expression for MaxWindow<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for MaxWindow<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for MaxWindow<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for MaxWindow<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("max(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E> MaxWindow<E> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}
