//! Window function support for diesel-clickhouse.
//!
//! This module provides SQL window functions (OVER clause) support.
//!
//! # Examples
//!
//! ```rust,ignore
//! use diesel_clickhouse::expression::window::*;
//!
//! // ROW_NUMBER
//! row_number().over(
//!     Window::new()
//!         .partition_by(users::department)
//!         .order_by(users::salary.desc())
//! )
//!
//! // RANK with partition
//! rank().over(Window::partition_by(category).order_by(price.desc()))
//!
//! // Aggregate as window function
//! sum(orders::amount)
//!     .over(Window::new()
//!         .partition_by(orders::user_id)
//!         .order_by(orders::date)
//!         .rows_between(Preceding::Unbounded, Current))
//!
//! // LAG/LEAD
//! lag(orders::amount, 1, 0)
//!     .over(Window::partition_by(orders::user_id).order_by(orders::date))
//! ```

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use super::{AppearsOnTable, Expression, SelectableExpression};

// =============================================================================
// Window Definition
// =============================================================================

/// A window definition for the OVER clause.
///
/// Built using the builder pattern with `partition_by`, `order_by`, and frame specs.
#[derive(Debug, Clone, Copy)]
pub struct Window<P = (), O = (), F = ()> {
    partition_by: P,
    order_by: O,
    frame: F,
}

impl Window<(), (), ()> {
    /// Create a new empty window definition.
    pub fn new() -> Self {
        Self {
            partition_by: (),
            order_by: (),
            frame: (),
        }
    }
}

impl Default for Window<(), (), ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P, O, F> Window<P, O, F> {
    /// Add or replace PARTITION BY clause.
    pub fn partition_by<P2: Expression>(self, expr: P2) -> Window<P2, O, F> {
        Window {
            partition_by: expr,
            order_by: self.order_by,
            frame: self.frame,
        }
    }

    /// Add or replace ORDER BY clause.
    pub fn order_by<O2: Expression>(self, expr: O2) -> Window<P, O2, F> {
        Window {
            partition_by: self.partition_by,
            order_by: expr,
            frame: self.frame,
        }
    }

    /// Add ROWS BETWEEN frame.
    pub fn rows_between<S: FrameBound, E: FrameBound>(self, start: S, end: E) -> Window<P, O, RowsFrame<S, E>> {
        Window {
            partition_by: self.partition_by,
            order_by: self.order_by,
            frame: RowsFrame { start, end },
        }
    }

    /// Add RANGE BETWEEN frame.
    pub fn range_between<S: FrameBound, E: FrameBound>(self, start: S, end: E) -> Window<P, O, RangeFrame<S, E>> {
        Window {
            partition_by: self.partition_by,
            order_by: self.order_by,
            frame: RangeFrame { start, end },
        }
    }

    /// Add ROWS frame from start to CURRENT ROW.
    pub fn rows_from<S: FrameBound>(self, start: S) -> Window<P, O, RowsFrame<S, CurrentRow>> {
        self.rows_between(start, CurrentRow)
    }

    /// Add RANGE frame from start to CURRENT ROW.
    pub fn range_from<S: FrameBound>(self, start: S) -> Window<P, O, RangeFrame<S, CurrentRow>> {
        self.range_between(start, CurrentRow)
    }
}

// Trait to check if window clauses are empty
trait IsWindowClause {
    fn is_empty(&self) -> bool;
}

impl IsWindowClause for () {
    fn is_empty(&self) -> bool {
        true
    }
}

impl<T: Expression> IsWindowClause for T {
    fn is_empty(&self) -> bool {
        false
    }
}

impl<S: FrameBound, E: FrameBound> IsWindowClause for RowsFrame<S, E> {
    fn is_empty(&self) -> bool {
        false
    }
}

impl<S: FrameBound, E: FrameBound> IsWindowClause for RangeFrame<S, E> {
    fn is_empty(&self) -> bool {
        false
    }
}

impl<P, O, F, DB> QueryFragment<DB> for Window<P, O, F>
where
    P: QueryFragment<DB> + IsWindowClause,
    O: QueryFragment<DB> + IsWindowClause,
    F: QueryFragment<DB> + IsWindowClause,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        let has_partition = !self.partition_by.is_empty();
        let has_order = !self.order_by.is_empty();
        let has_frame = !self.frame.is_empty();

        if has_partition {
            pass.push_sql("PARTITION BY ");
            self.partition_by.walk_ast(pass.reborrow())?;
        }

        if has_order {
            if has_partition {
                pass.push_sql(" ");
            }
            pass.push_sql("ORDER BY ");
            self.order_by.walk_ast(pass.reborrow())?;
        }

        if has_frame {
            if has_partition || has_order {
                pass.push_sql(" ");
            }
            self.frame.walk_ast(pass.reborrow())?;
        }

        Ok(())
    }
}

// =============================================================================
// Frame Bounds
// =============================================================================

/// Trait for frame bounds (UNBOUNDED PRECEDING, CURRENT ROW, etc.)
pub trait FrameBound: QueryFragment<crate::backend::HttpBackend> + QueryFragment<crate::backend::NativeBackend> {}

/// UNBOUNDED PRECEDING
#[derive(Debug, Clone, Copy)]
pub struct UnboundedPreceding;

impl FrameBound for UnboundedPreceding {}

impl<DB: Backend> QueryFragment<DB> for UnboundedPreceding {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("UNBOUNDED PRECEDING");
        Ok(())
    }
}

/// UNBOUNDED FOLLOWING
#[derive(Debug, Clone, Copy)]
pub struct UnboundedFollowing;

impl FrameBound for UnboundedFollowing {}

impl<DB: Backend> QueryFragment<DB> for UnboundedFollowing {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("UNBOUNDED FOLLOWING");
        Ok(())
    }
}

/// CURRENT ROW
#[derive(Debug, Clone, Copy)]
pub struct CurrentRow;

impl FrameBound for CurrentRow {}

impl<DB: Backend> QueryFragment<DB> for CurrentRow {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("CURRENT ROW");
        Ok(())
    }
}

/// N PRECEDING
#[derive(Debug, Clone, Copy)]
pub struct Preceding(pub i64);

impl FrameBound for Preceding {}

impl<DB: Backend> QueryFragment<DB> for Preceding {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_bindable(&self.0)?;
        pass.push_sql(" PRECEDING");
        Ok(())
    }
}

/// N FOLLOWING
#[derive(Debug, Clone, Copy)]
pub struct Following(pub i64);

impl FrameBound for Following {}

impl<DB: Backend> QueryFragment<DB> for Following {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_bindable(&self.0)?;
        pass.push_sql(" FOLLOWING");
        Ok(())
    }
}

// =============================================================================
// Frame Types
// =============================================================================

/// ROWS BETWEEN frame specification.
#[derive(Debug, Clone, Copy)]
pub struct RowsFrame<S, E> {
    start: S,
    end: E,
}

impl<S: FrameBound, E: FrameBound, DB: Backend> QueryFragment<DB> for RowsFrame<S, E>
where
    S: QueryFragment<DB>,
    E: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("ROWS BETWEEN ");
        self.start.walk_ast(pass.reborrow())?;
        pass.push_sql(" AND ");
        self.end.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// RANGE BETWEEN frame specification.
#[derive(Debug, Clone, Copy)]
pub struct RangeFrame<S, E> {
    start: S,
    end: E,
}

impl<S: FrameBound, E: FrameBound, DB: Backend> QueryFragment<DB> for RangeFrame<S, E>
where
    S: QueryFragment<DB>,
    E: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("RANGE BETWEEN ");
        self.start.walk_ast(pass.reborrow())?;
        pass.push_sql(" AND ");
        self.end.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// =============================================================================
// OVER expression
// =============================================================================

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

// =============================================================================
// Window Functions
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
// SUM window function (direct, not wrapped)
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BindCollector, HttpBackend, HttpBindCollector, HttpQueryBuilder, QueryBuilder as _};
    use crate::expression::Bound;
    use diesel_clickhouse_types::UInt64;

    // Test column
    #[derive(Debug, Clone, Copy)]
    struct AmountColumn;

    impl Expression for AmountColumn {
        type SqlType = UInt64;
    }
    impl<T> SelectableExpression<T> for AmountColumn {}
    impl<T> AppearsOnTable<T> for AmountColumn {}

    impl<DB: Backend> QueryFragment<DB> for AmountColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("amount");
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct UserIdColumn;

    impl Expression for UserIdColumn {
        type SqlType = UInt64;
    }
    impl<T> SelectableExpression<T> for UserIdColumn {}
    impl<T> AppearsOnTable<T> for UserIdColumn {}

    impl<DB: Backend> QueryFragment<DB> for UserIdColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("user_id");
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct DateColumn;

    impl Expression for DateColumn {
        type SqlType = diesel_clickhouse_types::Date;
    }
    impl<T> SelectableExpression<T> for DateColumn {}
    impl<T> AppearsOnTable<T> for DateColumn {}

    impl<DB: Backend> QueryFragment<DB> for DateColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("date");
            Ok(())
        }
    }

    fn to_sql<T: QueryFragment<HttpBackend>>(fragment: &T) -> String {
        let mut builder = HttpQueryBuilder::default();
        let mut collector = HttpBindCollector::default();
        let pass = AstPass::<HttpBackend>::new(&mut builder, &mut collector);
        fragment.walk_ast(pass).ok();

        // Inline bindings into the SQL for easier test assertions
        let mut sql = builder.finish();
        for binding in collector.bindable_values().iter().rev() {
            if let Some(pos) = sql.rfind("{p") {
                if let Some(end) = sql[pos..].find('}') {
                    sql.replace_range(pos..pos + end + 1, &binding.sql_literal());
                }
            }
        }
        sql
    }

    #[test]
    fn test_row_number() {
        let expr = row_number().over(Window::new());
        let sql = to_sql(&expr);
        assert_eq!(sql, "row_number() OVER ()");
    }

    #[test]
    fn test_row_number_with_partition() {
        let expr = row_number().over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "row_number() OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_row_number_with_order() {
        let expr = row_number().over(Window::new().order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "row_number() OVER (ORDER BY `date`)");
    }

    #[test]
    fn test_row_number_with_partition_and_order() {
        let expr = row_number().over(
            Window::new()
                .partition_by(UserIdColumn)
                .order_by(DateColumn)
        );
        let sql = to_sql(&expr);
        assert_eq!(sql, "row_number() OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_rank() {
        let expr = rank().over(Window::new().order_by(AmountColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "rank() OVER (ORDER BY `amount`)");
    }

    #[test]
    fn test_dense_rank() {
        let expr = dense_rank().over(Window::new().order_by(AmountColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "dense_rank() OVER (ORDER BY `amount`)");
    }

    #[test]
    fn test_ntile() {
        let expr = ntile(4).over(Window::new().order_by(AmountColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "ntile(4) OVER (ORDER BY `amount`)");
    }

    #[test]
    fn test_lag() {
        let expr = lag(AmountColumn, 1, Bound::<_, UInt64>::new(0u64))
            .over(Window::new().partition_by(UserIdColumn).order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "lag(`amount`, 1, 0) OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_lead() {
        let expr = lead(AmountColumn, 1, Bound::<_, UInt64>::new(0u64))
            .over(Window::new().order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "lead(`amount`, 1, 0) OVER (ORDER BY `date`)");
    }

    #[test]
    fn test_first_value() {
        let expr = first_value(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn).order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "first_value(`amount`) OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_last_value() {
        let expr = last_value(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn).order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "last_value(`amount`) OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_nth_value() {
        let expr = nth_value(AmountColumn, 3)
            .over(Window::new().order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "nth_value(`amount`, 3) OVER (ORDER BY `date`)");
    }

    #[test]
    fn test_sum_window() {
        let expr = sum_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn).order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "sum(`amount`) OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_avg_window() {
        let expr = avg_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "avg(`amount`) OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_count_window() {
        let expr = count_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "count(`amount`) OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_min_window() {
        let expr = min_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "min(`amount`) OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_max_window() {
        let expr = max_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "max(`amount`) OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_rows_between() {
        let expr = sum_over(AmountColumn)
            .over(
                Window::new()
                    .partition_by(UserIdColumn)
                    .order_by(DateColumn)
                    .rows_between(UnboundedPreceding, CurrentRow)
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "sum(`amount`) OVER (PARTITION BY `user_id` ORDER BY `date` ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)"
        );
    }

    #[test]
    fn test_rows_between_n_preceding() {
        let expr = avg_over(AmountColumn)
            .over(
                Window::new()
                    .order_by(DateColumn)
                    .rows_between(Preceding(3), CurrentRow)
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "avg(`amount`) OVER (ORDER BY `date` ROWS BETWEEN 3 PRECEDING AND CURRENT ROW)"
        );
    }

    #[test]
    fn test_range_between() {
        let expr = sum_over(AmountColumn)
            .over(
                Window::new()
                    .order_by(DateColumn)
                    .range_between(UnboundedPreceding, UnboundedFollowing)
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "sum(`amount`) OVER (ORDER BY `date` RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING)"
        );
    }

    #[test]
    fn test_rows_from_shorthand() {
        let expr = sum_over(AmountColumn)
            .over(
                Window::new()
                    .order_by(DateColumn)
                    .rows_from(Preceding(5))
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "sum(`amount`) OVER (ORDER BY `date` ROWS BETWEEN 5 PRECEDING AND CURRENT ROW)"
        );
    }

    #[test]
    fn test_following_frame() {
        let expr = sum_over(AmountColumn)
            .over(
                Window::new()
                    .order_by(DateColumn)
                    .rows_between(CurrentRow, Following(3))
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "sum(`amount`) OVER (ORDER BY `date` ROWS BETWEEN CURRENT ROW AND 3 FOLLOWING)"
        );
    }
}
