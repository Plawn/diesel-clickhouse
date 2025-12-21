//! CASE expression support for conditional logic in queries.
//!
//! This module provides support for SQL CASE expressions:
//! - Simple CASE: `CASE expr WHEN value THEN result ... END`
//! - Searched CASE: `CASE WHEN condition THEN result ... END`
//!
//! # Examples
//!
//! ```rust,ignore
//! use diesel_clickhouse::expression::case::{case_when, case};
//!
//! // Searched CASE (CASE WHEN ... THEN ... END)
//! let status = case_when(orders::amount.gt(1000))
//!     .then("premium")
//!     .when(orders::amount.gt(100))
//!     .then("standard")
//!     .else_("basic");
//!
//! // Simple CASE (CASE expr WHEN value THEN result END)
//! let label = case(users::status)
//!     .when(1).then("active")
//!     .when(2).then("inactive")
//!     .else_("unknown");
//! ```

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use super::{AppearsOnTable, Expression, SelectableExpression};

// =============================================================================
// Searched CASE (CASE WHEN condition THEN result ...)
// =============================================================================

/// Builder for a searched CASE expression.
///
/// Created by calling `case_when(condition)`.
#[derive(Debug, Clone, Copy)]
pub struct CaseWhenBuilder<C> {
    condition: C,
}

/// A CASE WHEN with a THEN clause, ready for more WHENs or ELSE.
#[derive(Debug, Clone, Copy)]
pub struct CaseWhenThen<C, T, Rest = ()> {
    condition: C,
    then_value: T,
    rest: Rest,
}

/// A complete CASE expression with an ELSE clause.
#[derive(Debug, Clone, Copy)]
pub struct CaseWhenElse<C, T, E, Rest = ()> {
    condition: C,
    then_value: T,
    else_value: E,
    rest: Rest,
}

/// Start building a searched CASE expression.
///
/// # Example
///
/// ```rust,ignore
/// case_when(column.gt(100))
///     .then("high")
///     .else_("low")
/// ```
pub fn case_when<C>(condition: C) -> CaseWhenBuilder<C>
where
    C: Expression,
{
    CaseWhenBuilder { condition }
}

impl<C> CaseWhenBuilder<C>
where
    C: Expression,
{
    /// Add the THEN clause for this condition.
    pub fn then<T>(self, value: T) -> CaseWhenThen<C, T>
    where
        T: Expression,
    {
        CaseWhenThen {
            condition: self.condition,
            then_value: value,
            rest: (),
        }
    }
}

impl<C, T, Rest> CaseWhenThen<C, T, Rest>
where
    C: Expression,
    T: Expression,
{
    /// Add another WHEN clause.
    pub fn when<C2>(self, condition: C2) -> CaseWhenThenWhen<C, T, Rest, C2>
    where
        C2: Expression,
    {
        CaseWhenThenWhen {
            prev: self,
            condition,
        }
    }

    /// Add the ELSE clause, completing the CASE expression.
    pub fn else_<E>(self, value: E) -> CaseWhenElse<C, T, E, Rest>
    where
        E: Expression,
    {
        CaseWhenElse {
            condition: self.condition,
            then_value: self.then_value,
            else_value: value,
            rest: self.rest,
        }
    }
}

/// Intermediate state: waiting for THEN after a WHEN.
#[derive(Debug, Clone, Copy)]
pub struct CaseWhenThenWhen<C, T, Rest, C2> {
    prev: CaseWhenThen<C, T, Rest>,
    condition: C2,
}

impl<C, T, Rest, C2> CaseWhenThenWhen<C, T, Rest, C2>
where
    C: Expression,
    T: Expression,
    C2: Expression,
{
    /// Add the THEN clause for this WHEN.
    pub fn then<T2>(self, value: T2) -> CaseWhenThen<C2, T2, CaseWhenThen<C, T, Rest>>
    where
        T2: Expression,
    {
        CaseWhenThen {
            condition: self.condition,
            then_value: value,
            rest: self.prev,
        }
    }
}

// Expression implementations for CaseWhenThen (without ELSE, returns Nullable)
impl<C, T, Rest> Expression for CaseWhenThen<C, T, Rest>
where
    C: Expression,
    T: Expression,
{
    type SqlType = diesel_clickhouse_types::Nullable<T::SqlType>;
}

impl<C, T, Rest, QS> SelectableExpression<QS> for CaseWhenThen<C, T, Rest>
where
    C: SelectableExpression<QS>,
    T: SelectableExpression<QS>,
    Rest: SelectableExpressionRest<QS>,
{
}

impl<C, T, Rest, QS> AppearsOnTable<QS> for CaseWhenThen<C, T, Rest>
where
    C: AppearsOnTable<QS>,
    T: AppearsOnTable<QS>,
    Rest: AppearsOnTableRest<QS>,
{
}

// Expression implementations for CaseWhenElse
impl<C, T, E, Rest> Expression for CaseWhenElse<C, T, E, Rest>
where
    C: Expression,
    T: Expression,
    E: Expression,
{
    type SqlType = T::SqlType;
}

impl<C, T, E, Rest, QS> SelectableExpression<QS> for CaseWhenElse<C, T, E, Rest>
where
    C: SelectableExpression<QS>,
    T: SelectableExpression<QS>,
    E: SelectableExpression<QS>,
    Rest: SelectableExpressionRest<QS>,
{
}

impl<C, T, E, Rest, QS> AppearsOnTable<QS> for CaseWhenElse<C, T, E, Rest>
where
    C: AppearsOnTable<QS>,
    T: AppearsOnTable<QS>,
    E: AppearsOnTable<QS>,
    Rest: AppearsOnTableRest<QS>,
{
}

// Helper traits for Rest type bounds
trait SelectableExpressionRest<QS> {}
impl<QS> SelectableExpressionRest<QS> for () {}
impl<C, T, Rest, QS> SelectableExpressionRest<QS> for CaseWhenThen<C, T, Rest>
where
    C: SelectableExpression<QS>,
    T: SelectableExpression<QS>,
    Rest: SelectableExpressionRest<QS>,
{
}

trait AppearsOnTableRest<QS> {}
impl<QS> AppearsOnTableRest<QS> for () {}
impl<C, T, Rest, QS> AppearsOnTableRest<QS> for CaseWhenThen<C, T, Rest>
where
    C: AppearsOnTable<QS>,
    T: AppearsOnTable<QS>,
    Rest: AppearsOnTableRest<QS>,
{
}

// QueryFragment for CaseWhenThen (without ELSE)
impl<C, T, Rest, DB> QueryFragment<DB> for CaseWhenThen<C, T, Rest>
where
    C: QueryFragment<DB>,
    T: QueryFragment<DB>,
    Rest: QueryFragmentRest<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("CASE");
        self.rest.walk_ast_rest(pass.reborrow())?;
        pass.push_sql(" WHEN ");
        self.condition.walk_ast(pass.reborrow())?;
        pass.push_sql(" THEN ");
        self.then_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" END");
        Ok(())
    }
}

// QueryFragment for CaseWhenElse
impl<C, T, E, Rest, DB> QueryFragment<DB> for CaseWhenElse<C, T, E, Rest>
where
    C: QueryFragment<DB>,
    T: QueryFragment<DB>,
    E: QueryFragment<DB>,
    Rest: QueryFragmentRest<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("CASE");
        self.rest.walk_ast_rest(pass.reborrow())?;
        pass.push_sql(" WHEN ");
        self.condition.walk_ast(pass.reborrow())?;
        pass.push_sql(" THEN ");
        self.then_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" ELSE ");
        self.else_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" END");
        Ok(())
    }
}

// Helper trait for walking rest of CASE branches
trait QueryFragmentRest<DB: Backend> {
    fn walk_ast_rest<'b>(&'b self, pass: AstPass<'_, 'b, DB>) -> QueryResult<()>;
}

impl<DB: Backend> QueryFragmentRest<DB> for () {
    fn walk_ast_rest<'b>(&'b self, _pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        Ok(())
    }
}

impl<C, T, Rest, DB> QueryFragmentRest<DB> for CaseWhenThen<C, T, Rest>
where
    C: QueryFragment<DB>,
    T: QueryFragment<DB>,
    Rest: QueryFragmentRest<DB>,
    DB: Backend,
{
    fn walk_ast_rest<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.rest.walk_ast_rest(pass.reborrow())?;
        pass.push_sql(" WHEN ");
        self.condition.walk_ast(pass.reborrow())?;
        pass.push_sql(" THEN ");
        self.then_value.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// =============================================================================
// Simple CASE (CASE expr WHEN value THEN result ...)
// =============================================================================

/// Builder for a simple CASE expression.
///
/// Created by calling `case(expr)`.
#[derive(Debug, Clone, Copy)]
pub struct SimpleCaseBuilder<E> {
    expr: E,
}

/// A simple CASE with one WHEN-THEN pair.
#[derive(Debug, Clone, Copy)]
pub struct SimpleCaseWhenThen<E, W, T, Rest = ()> {
    expr: E,
    when_value: W,
    then_value: T,
    rest: Rest,
}

/// A complete simple CASE with ELSE.
#[derive(Debug, Clone, Copy)]
pub struct SimpleCaseElse<E, W, T, El, Rest = ()> {
    expr: E,
    when_value: W,
    then_value: T,
    else_value: El,
    rest: Rest,
}

/// Start building a simple CASE expression.
///
/// # Example
///
/// ```rust,ignore
/// case(users::status)
///     .when(1).then("active")
///     .when(2).then("inactive")
///     .else_("unknown")
/// ```
pub fn case<E>(expr: E) -> SimpleCaseBuilder<E>
where
    E: Expression,
{
    SimpleCaseBuilder { expr }
}

impl<E> SimpleCaseBuilder<E>
where
    E: Expression,
{
    /// Add a WHEN clause.
    pub fn when<W>(self, value: W) -> SimpleCaseWhen<E, W>
    where
        W: Expression,
    {
        SimpleCaseWhen {
            expr: self.expr,
            when_value: value,
        }
    }
}

/// Intermediate: waiting for THEN.
#[derive(Debug, Clone, Copy)]
pub struct SimpleCaseWhen<E, W> {
    expr: E,
    when_value: W,
}

impl<E, W> SimpleCaseWhen<E, W>
where
    E: Expression,
    W: Expression,
{
    /// Add the THEN clause.
    pub fn then<T>(self, value: T) -> SimpleCaseWhenThen<E, W, T>
    where
        T: Expression,
    {
        SimpleCaseWhenThen {
            expr: self.expr,
            when_value: self.when_value,
            then_value: value,
            rest: (),
        }
    }
}

impl<E, W, T, Rest> SimpleCaseWhenThen<E, W, T, Rest>
where
    E: Expression,
    W: Expression,
    T: Expression,
{
    /// Add another WHEN clause.
    pub fn when<W2>(self, value: W2) -> SimpleCaseWhenThenWhen<E, W, T, Rest, W2>
    where
        W2: Expression,
    {
        SimpleCaseWhenThenWhen { prev: self, when_value: value }
    }

    /// Add the ELSE clause.
    pub fn else_<El>(self, value: El) -> SimpleCaseElse<E, W, T, El, Rest>
    where
        El: Expression,
    {
        SimpleCaseElse {
            expr: self.expr,
            when_value: self.when_value,
            then_value: self.then_value,
            else_value: value,
            rest: self.rest,
        }
    }
}

/// Intermediate: waiting for THEN after another WHEN.
#[derive(Debug, Clone, Copy)]
pub struct SimpleCaseWhenThenWhen<E, W, T, Rest, W2> {
    prev: SimpleCaseWhenThen<E, W, T, Rest>,
    when_value: W2,
}

impl<E, W, T, Rest, W2> SimpleCaseWhenThenWhen<E, W, T, Rest, W2>
where
    E: Expression + Clone,
    W: Expression,
    T: Expression,
    W2: Expression,
{
    /// Add the THEN clause.
    pub fn then<T2>(self, value: T2) -> SimpleCaseWhenThen<E, W2, T2, SimpleCaseWhenThenRest<W, T, Rest>>
    where
        T2: Expression,
    {
        SimpleCaseWhenThen {
            expr: self.prev.expr.clone(),
            when_value: self.when_value,
            then_value: value,
            rest: SimpleCaseWhenThenRest {
                when_value: self.prev.when_value,
                then_value: self.prev.then_value,
                rest: self.prev.rest,
            },
        }
    }
}

/// Rest of simple CASE branches (without the expr, since it's only at top level).
#[derive(Debug, Clone, Copy)]
pub struct SimpleCaseWhenThenRest<W, T, Rest> {
    when_value: W,
    then_value: T,
    rest: Rest,
}

// Expression for SimpleCaseWhenThen (without ELSE)
impl<E, W, T, Rest> Expression for SimpleCaseWhenThen<E, W, T, Rest>
where
    E: Expression,
    W: Expression,
    T: Expression,
{
    type SqlType = diesel_clickhouse_types::Nullable<T::SqlType>;
}

impl<E, W, T, Rest, QS> SelectableExpression<QS> for SimpleCaseWhenThen<E, W, T, Rest>
where
    E: SelectableExpression<QS>,
    W: SelectableExpression<QS>,
    T: SelectableExpression<QS>,
    Rest: SimpleCaseSelectableRest<QS>,
{
}

impl<E, W, T, Rest, QS> AppearsOnTable<QS> for SimpleCaseWhenThen<E, W, T, Rest>
where
    E: AppearsOnTable<QS>,
    W: AppearsOnTable<QS>,
    T: AppearsOnTable<QS>,
    Rest: SimpleCaseAppearsRest<QS>,
{
}

// Expression for SimpleCaseElse
impl<E, W, T, El, Rest> Expression for SimpleCaseElse<E, W, T, El, Rest>
where
    E: Expression,
    W: Expression,
    T: Expression,
    El: Expression,
{
    type SqlType = T::SqlType;
}

impl<E, W, T, El, Rest, QS> SelectableExpression<QS> for SimpleCaseElse<E, W, T, El, Rest>
where
    E: SelectableExpression<QS>,
    W: SelectableExpression<QS>,
    T: SelectableExpression<QS>,
    El: SelectableExpression<QS>,
    Rest: SimpleCaseSelectableRest<QS>,
{
}

impl<E, W, T, El, Rest, QS> AppearsOnTable<QS> for SimpleCaseElse<E, W, T, El, Rest>
where
    E: AppearsOnTable<QS>,
    W: AppearsOnTable<QS>,
    T: AppearsOnTable<QS>,
    El: AppearsOnTable<QS>,
    Rest: SimpleCaseAppearsRest<QS>,
{
}

// Helper traits for simple CASE
trait SimpleCaseSelectableRest<QS> {}
impl<QS> SimpleCaseSelectableRest<QS> for () {}
impl<E, W, T, Rest, QS> SimpleCaseSelectableRest<QS> for SimpleCaseWhenThen<E, W, T, Rest>
where
    E: SelectableExpression<QS>,
    W: SelectableExpression<QS>,
    T: SelectableExpression<QS>,
    Rest: SimpleCaseSelectableRest<QS>,
{
}
impl<W, T, Rest, QS> SimpleCaseSelectableRest<QS> for SimpleCaseWhenThenRest<W, T, Rest>
where
    W: SelectableExpression<QS>,
    T: SelectableExpression<QS>,
    Rest: SimpleCaseSelectableRest<QS>,
{
}

trait SimpleCaseAppearsRest<QS> {}
impl<QS> SimpleCaseAppearsRest<QS> for () {}
impl<E, W, T, Rest, QS> SimpleCaseAppearsRest<QS> for SimpleCaseWhenThen<E, W, T, Rest>
where
    E: AppearsOnTable<QS>,
    W: AppearsOnTable<QS>,
    T: AppearsOnTable<QS>,
    Rest: SimpleCaseAppearsRest<QS>,
{
}
impl<W, T, Rest, QS> SimpleCaseAppearsRest<QS> for SimpleCaseWhenThenRest<W, T, Rest>
where
    W: AppearsOnTable<QS>,
    T: AppearsOnTable<QS>,
    Rest: SimpleCaseAppearsRest<QS>,
{
}

// QueryFragment for SimpleCaseWhenThen
impl<E, W, T, Rest, DB> QueryFragment<DB> for SimpleCaseWhenThen<E, W, T, Rest>
where
    E: QueryFragment<DB>,
    W: QueryFragment<DB>,
    T: QueryFragment<DB>,
    Rest: SimpleCaseQueryFragmentRest<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("CASE ");
        self.expr.walk_ast(pass.reborrow())?;
        self.rest.walk_ast_simple_rest(pass.reborrow())?;
        pass.push_sql(" WHEN ");
        self.when_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" THEN ");
        self.then_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" END");
        Ok(())
    }
}

// QueryFragment for SimpleCaseElse
impl<E, W, T, El, Rest, DB> QueryFragment<DB> for SimpleCaseElse<E, W, T, El, Rest>
where
    E: QueryFragment<DB>,
    W: QueryFragment<DB>,
    T: QueryFragment<DB>,
    El: QueryFragment<DB>,
    Rest: SimpleCaseQueryFragmentRest<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("CASE ");
        self.expr.walk_ast(pass.reborrow())?;
        self.rest.walk_ast_simple_rest(pass.reborrow())?;
        pass.push_sql(" WHEN ");
        self.when_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" THEN ");
        self.then_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" ELSE ");
        self.else_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" END");
        Ok(())
    }
}

trait SimpleCaseQueryFragmentRest<DB: Backend> {
    fn walk_ast_simple_rest<'b>(&'b self, pass: AstPass<'_, 'b, DB>) -> QueryResult<()>;
}

impl<DB: Backend> SimpleCaseQueryFragmentRest<DB> for () {
    fn walk_ast_simple_rest<'b>(&'b self, _pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        Ok(())
    }
}

impl<E, W, T, Rest, DB> SimpleCaseQueryFragmentRest<DB> for SimpleCaseWhenThen<E, W, T, Rest>
where
    W: QueryFragment<DB>,
    T: QueryFragment<DB>,
    Rest: SimpleCaseQueryFragmentRest<DB>,
    DB: Backend,
{
    fn walk_ast_simple_rest<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.rest.walk_ast_simple_rest(pass.reborrow())?;
        pass.push_sql(" WHEN ");
        self.when_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" THEN ");
        self.then_value.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

impl<W, T, Rest, DB> SimpleCaseQueryFragmentRest<DB> for SimpleCaseWhenThenRest<W, T, Rest>
where
    W: QueryFragment<DB>,
    T: QueryFragment<DB>,
    Rest: SimpleCaseQueryFragmentRest<DB>,
    DB: Backend,
{
    fn walk_ast_simple_rest<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.rest.walk_ast_simple_rest(pass.reborrow())?;
        pass.push_sql(" WHEN ");
        self.when_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" THEN ");
        self.then_value.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

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
    condition: C,
    value: V,
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
trait MultiIfBranches<DB: Backend> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HttpBackend, HttpBindCollector, HttpQueryBuilder, QueryBuilder as _};
    use crate::expression::{Bound, Gt};
    use diesel_clickhouse_types::{CHString, UInt64};

    fn to_sql<T: QueryFragment<HttpBackend>>(fragment: &T) -> String {
        let mut builder = HttpQueryBuilder::default();
        let mut collector = HttpBindCollector::default();
        let pass = AstPass::<HttpBackend>::new(&mut builder, &mut collector);
        fragment.walk_ast(pass).ok();
        builder.finish()
    }

    #[test]
    fn test_case_when_else() {
        let gt = Gt {
            left: Bound::<_, UInt64>::new(10u64),
            right: Bound::<_, UInt64>::new(5u64),
        };
        let expr = case_when(gt)
            .then(Bound::<_, CHString>::new("yes"))
            .else_(Bound::<_, CHString>::new("no"));

        let sql = to_sql(&expr);
        assert_eq!(sql, "CASE WHEN 10 > 5 THEN 'yes' ELSE 'no' END");
    }

    #[test]
    fn test_case_when_multiple() {
        let gt1 = Gt {
            left: Bound::<_, UInt64>::new(10u64),
            right: Bound::<_, UInt64>::new(100u64),
        };
        let gt2 = Gt {
            left: Bound::<_, UInt64>::new(10u64),
            right: Bound::<_, UInt64>::new(50u64),
        };
        let expr = case_when(gt1)
            .then(Bound::<_, CHString>::new("high"))
            .when(gt2)
            .then(Bound::<_, CHString>::new("medium"))
            .else_(Bound::<_, CHString>::new("low"));

        let sql = to_sql(&expr);
        assert_eq!(sql, "CASE WHEN 10 > 100 THEN 'high' WHEN 10 > 50 THEN 'medium' ELSE 'low' END");
    }

    #[test]
    fn test_if_expression() {
        let gt = Gt {
            left: Bound::<_, UInt64>::new(10u64),
            right: Bound::<_, UInt64>::new(5u64),
        };
        let expr = if_(gt, Bound::<_, CHString>::new("yes"), Bound::<_, CHString>::new("no"));

        let sql = to_sql(&expr);
        assert_eq!(sql, "if(10 > 5, 'yes', 'no')");
    }

    #[test]
    fn test_multi_if() {
        let gt1 = Gt {
            left: Bound::<_, UInt64>::new(10u64),
            right: Bound::<_, UInt64>::new(100u64),
        };
        let gt2 = Gt {
            left: Bound::<_, UInt64>::new(10u64),
            right: Bound::<_, UInt64>::new(50u64),
        };
        let expr = multi_if()
            .when(gt1).then(Bound::<_, CHString>::new("high"))
            .when(gt2).then(Bound::<_, CHString>::new("medium"))
            .else_(Bound::<_, CHString>::new("low"));

        let sql = to_sql(&expr);
        assert_eq!(sql, "multiIf(10 > 100, 'high', 10 > 50, 'medium', 'low')");
    }
}
