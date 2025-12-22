//! Window functions (ROW_NUMBER, RANK, LAG, LEAD, etc.)
//!
//! This module provides the core window functions that can be used with OVER clauses.

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use super::over::Over;
use crate::expression::{AppearsOnTable, Expression, SelectableExpression};

// =============================================================================
// ROW_NUMBER
// =============================================================================

/// ROW_NUMBER() window function.
#[derive(Debug, Clone, Copy)]
pub struct RowNumber;

/// Create a ROW_NUMBER() window function.
pub fn row_number() -> RowNumber {
    RowNumber
}

impl Expression for RowNumber {
    type SqlType = diesel_clickhouse_types::UInt64;
}

impl<QS> SelectableExpression<QS> for RowNumber {}
impl<QS> AppearsOnTable<QS> for RowNumber {}

impl<DB: Backend> QueryFragment<DB> for RowNumber {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("row_number()");
        Ok(())
    }
}

impl RowNumber {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// RANK
// =============================================================================

/// RANK() window function.
#[derive(Debug, Clone, Copy)]
pub struct Rank;

/// Create a RANK() window function.
pub fn rank() -> Rank {
    Rank
}

impl Expression for Rank {
    type SqlType = diesel_clickhouse_types::UInt64;
}

impl<QS> SelectableExpression<QS> for Rank {}
impl<QS> AppearsOnTable<QS> for Rank {}

impl<DB: Backend> QueryFragment<DB> for Rank {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("rank()");
        Ok(())
    }
}

impl Rank {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// DENSE_RANK
// =============================================================================

/// DENSE_RANK() window function.
#[derive(Debug, Clone, Copy)]
pub struct DenseRank;

/// Create a DENSE_RANK() window function.
pub fn dense_rank() -> DenseRank {
    DenseRank
}

impl Expression for DenseRank {
    type SqlType = diesel_clickhouse_types::UInt64;
}

impl<QS> SelectableExpression<QS> for DenseRank {}
impl<QS> AppearsOnTable<QS> for DenseRank {}

impl<DB: Backend> QueryFragment<DB> for DenseRank {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("dense_rank()");
        Ok(())
    }
}

impl DenseRank {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// NTILE
// =============================================================================

/// NTILE(n) window function.
#[derive(Debug, Clone, Copy)]
pub struct Ntile {
    buckets: i64,
}

/// Create an NTILE(n) window function.
pub fn ntile(buckets: i64) -> Ntile {
    Ntile { buckets }
}

impl Expression for Ntile {
    type SqlType = diesel_clickhouse_types::UInt64;
}

impl<QS> SelectableExpression<QS> for Ntile {}
impl<QS> AppearsOnTable<QS> for Ntile {}

impl<DB: Backend> QueryFragment<DB> for Ntile {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("ntile(");
        pass.push_bindable(&self.buckets)?;
        pass.push_sql(")");
        Ok(())
    }
}

impl Ntile {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// LAG
// =============================================================================

/// LAG(expr, offset, default) window function.
#[derive(Debug, Clone, Copy)]
pub struct Lag<E, D> {
    expr: E,
    offset: i64,
    default: D,
}

/// Create a LAG window function.
pub fn lag<E, D>(expr: E, offset: i64, default: D) -> Lag<E, D>
where
    E: Expression,
    D: Expression,
{
    Lag { expr, offset, default }
}

impl<E: Expression, D> Expression for Lag<E, D> {
    type SqlType = E::SqlType;
}

impl<E, D, QS> SelectableExpression<QS> for Lag<E, D>
where
    E: SelectableExpression<QS>,
{
}

impl<E, D, QS> AppearsOnTable<QS> for Lag<E, D>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, D, DB> QueryFragment<DB> for Lag<E, D>
where
    E: QueryFragment<DB>,
    D: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("lag(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        pass.push_bindable(&self.offset)?;
        pass.push_sql(", ");
        self.default.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E, D> Lag<E, D> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// LEAD
// =============================================================================

/// LEAD(expr, offset, default) window function.
#[derive(Debug, Clone, Copy)]
pub struct Lead<E, D> {
    expr: E,
    offset: i64,
    default: D,
}

/// Create a LEAD window function.
pub fn lead<E, D>(expr: E, offset: i64, default: D) -> Lead<E, D>
where
    E: Expression,
    D: Expression,
{
    Lead { expr, offset, default }
}

impl<E: Expression, D> Expression for Lead<E, D> {
    type SqlType = E::SqlType;
}

impl<E, D, QS> SelectableExpression<QS> for Lead<E, D>
where
    E: SelectableExpression<QS>,
{
}

impl<E, D, QS> AppearsOnTable<QS> for Lead<E, D>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, D, DB> QueryFragment<DB> for Lead<E, D>
where
    E: QueryFragment<DB>,
    D: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("lead(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        pass.push_bindable(&self.offset)?;
        pass.push_sql(", ");
        self.default.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E, D> Lead<E, D> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// FIRST_VALUE
// =============================================================================

/// FIRST_VALUE(expr) window function.
#[derive(Debug, Clone, Copy)]
pub struct FirstValue<E> {
    expr: E,
}

/// Create a FIRST_VALUE window function.
pub fn first_value<E>(expr: E) -> FirstValue<E>
where
    E: Expression,
{
    FirstValue { expr }
}

impl<E: Expression> Expression for FirstValue<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for FirstValue<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for FirstValue<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for FirstValue<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("first_value(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E> FirstValue<E> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// LAST_VALUE
// =============================================================================

/// LAST_VALUE(expr) window function.
#[derive(Debug, Clone, Copy)]
pub struct LastValue<E> {
    expr: E,
}

/// Create a LAST_VALUE window function.
pub fn last_value<E>(expr: E) -> LastValue<E>
where
    E: Expression,
{
    LastValue { expr }
}

impl<E: Expression> Expression for LastValue<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for LastValue<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for LastValue<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for LastValue<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("last_value(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E> LastValue<E> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}

// =============================================================================
// NTH_VALUE
// =============================================================================

/// NTH_VALUE(expr, n) window function.
#[derive(Debug, Clone, Copy)]
pub struct NthValue<E> {
    expr: E,
    n: i64,
}

/// Create an NTH_VALUE window function.
pub fn nth_value<E>(expr: E, n: i64) -> NthValue<E>
where
    E: Expression,
{
    NthValue { expr, n }
}

impl<E: Expression> Expression for NthValue<E> {
    type SqlType = diesel_clickhouse_types::Nullable<E::SqlType>;
}

impl<E, QS> SelectableExpression<QS> for NthValue<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for NthValue<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for NthValue<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("nth_value(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        pass.push_bindable(&self.n)?;
        pass.push_sql(")");
        Ok(())
    }
}

impl<E> NthValue<E> {
    /// Apply an OVER clause.
    pub fn over<W>(self, window: W) -> Over<Self, W> {
        Over::new(self, window)
    }
}
