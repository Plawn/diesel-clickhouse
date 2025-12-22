//! ClickHouse-specific IF and multiIf expressions.
//!
//! These are shorthand alternatives to CASE expressions that are specific to ClickHouse.

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use crate::expression::{AppearsOnTable, Expression, SelectableExpression};

// =============================================================================
// If expression (ClickHouse-specific shorthand)
// =============================================================================

/// ClickHouse IF(condition, then, else) function.
///
/// This is a shorthand for simple two-branch CASE expressions.
#[derive(Debug, Clone, Copy)]
pub struct If<C, T, E> {
    condition: C,
    then_value: T,
    else_value: E,
}

/// Create an IF expression (ClickHouse-specific).
///
/// # Example
///
/// ```rust,ignore
/// if_(column.gt(100), "high", "low")
/// // Generates: if(column > 100, 'high', 'low')
/// ```
pub fn if_<C, T, E>(condition: C, then_value: T, else_value: E) -> If<C, T, E>
where
    C: Expression,
    T: Expression,
    E: Expression,
{
    If {
        condition,
        then_value,
        else_value,
    }
}

impl<C, T, E> Expression for If<C, T, E>
where
    C: Expression,
    T: Expression,
    E: Expression,
{
    type SqlType = T::SqlType;
}

impl<C, T, E, QS> SelectableExpression<QS> for If<C, T, E>
where
    C: SelectableExpression<QS>,
    T: SelectableExpression<QS>,
    E: SelectableExpression<QS>,
{
}

impl<C, T, E, QS> AppearsOnTable<QS> for If<C, T, E>
where
    C: AppearsOnTable<QS>,
    T: AppearsOnTable<QS>,
    E: AppearsOnTable<QS>,
{
}

impl<C, T, E, DB> QueryFragment<DB> for If<C, T, E>
where
    C: QueryFragment<DB>,
    T: QueryFragment<DB>,
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("if(");
        self.condition.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.then_value.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.else_value.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// multiIf expression (ClickHouse-specific)
// =============================================================================

/// ClickHouse multiIf function for multiple conditions.
///
/// Generates: `multiIf(cond1, val1, cond2, val2, ..., default)`
#[derive(Debug, Clone)]
pub struct MultiIf<Branches, Default> {
    branches: Branches,
    default: Default,
}

/// A single condition-value branch for multiIf.
#[derive(Debug, Clone, Copy)]
pub struct Branch<C, V> {
    pub(crate) condition: C,
    pub(crate) value: V,
}

/// Builder for multiIf.
#[derive(Debug, Clone, Copy)]
pub struct MultiIfBuilder<Branches> {
    branches: Branches,
}

/// Start building a multiIf expression.
///
/// # Example
///
/// ```rust,ignore
/// multi_if()
///     .when(col.gt(100)).then("high")
///     .when(col.gt(50)).then("medium")
///     .else_("low")
/// ```
pub fn multi_if() -> MultiIfBuilder<()> {
    MultiIfBuilder { branches: () }
}

impl<B> MultiIfBuilder<B> {
    /// Add a condition.
    pub fn when<C>(self, condition: C) -> MultiIfWhen<B, C>
    where
        C: Expression,
    {
        MultiIfWhen {
            branches: self.branches,
            condition,
        }
    }
}

/// Intermediate: waiting for THEN.
#[derive(Debug, Clone, Copy)]
pub struct MultiIfWhen<B, C> {
    branches: B,
    condition: C,
}

impl<B, C> MultiIfWhen<B, C>
where
    C: Expression,
{
    /// Add the value for this condition.
    pub fn then<V>(self, value: V) -> MultiIfBuilder<(B, Branch<C, V>)>
    where
        V: Expression,
    {
        MultiIfBuilder {
            branches: (self.branches, Branch {
                condition: self.condition,
                value,
            }),
        }
    }
}

impl<B> MultiIfBuilder<B> {
    /// Add the default value.
    pub fn else_<D>(self, default: D) -> MultiIf<B, D>
    where
        D: Expression,
    {
        MultiIf {
            branches: self.branches,
            default,
        }
    }
}

// Trait for walking multiIf branches
pub(crate) trait MultiIfBranches<DB: Backend> {
    fn walk_branches<'b>(&'b self, pass: AstPass<'_, 'b, DB>) -> QueryResult<()>;
}

impl<DB: Backend> MultiIfBranches<DB> for () {
    fn walk_branches<'b>(&'b self, _pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        Ok(())
    }
}

impl<B, C, V, DB> MultiIfBranches<DB> for (B, Branch<C, V>)
where
    B: MultiIfBranches<DB>,
    C: QueryFragment<DB>,
    V: QueryFragment<DB> + Expression,
    DB: Backend,
{
    fn walk_branches<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.0.walk_branches(pass.reborrow())?;
        self.1.condition.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.1.value.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        Ok(())
    }
}

impl<B, D> Expression for MultiIf<B, D>
where
    D: Expression,
{
    type SqlType = D::SqlType;
}

impl<B, D, QS> SelectableExpression<QS> for MultiIf<B, D>
where
    D: SelectableExpression<QS>,
{
}

impl<B, D, QS> AppearsOnTable<QS> for MultiIf<B, D>
where
    D: AppearsOnTable<QS>,
{
}

impl<B, D, DB> QueryFragment<DB> for MultiIf<B, D>
where
    B: MultiIfBranches<DB>,
    D: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("multiIf(");
        self.branches.walk_branches(pass.reborrow())?;
        self.default.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}
