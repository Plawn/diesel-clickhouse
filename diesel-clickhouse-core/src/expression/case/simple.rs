//! Simple CASE expressions (CASE expr WHEN value THEN result ...)
//!
//! This module provides the simple CASE expression where a single expression
//! is compared against multiple values.

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use crate::expression::{AppearsOnTable, Expression, SelectableExpression};

// =============================================================================
// Simple CASE (CASE expr WHEN value THEN result ...)
// =============================================================================

/// Builder for a simple CASE expression.
///
/// Created by calling `case(expr)`.
#[derive(Debug, Clone, Copy)]
pub struct SimpleCaseBuilder<E> {
    pub(crate) expr: E,
}

/// A simple CASE with one WHEN-THEN pair.
#[derive(Debug, Clone, Copy)]
pub struct SimpleCaseWhenThen<E, W, T, Rest = ()> {
    pub(crate) expr: E,
    pub(crate) when_value: W,
    pub(crate) then_value: T,
    pub(crate) rest: Rest,
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
pub(crate) trait SimpleCaseSelectableRest<QS> {}
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

pub(crate) trait SimpleCaseAppearsRest<QS> {}
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

pub(crate) trait SimpleCaseQueryFragmentRest<DB: Backend> {
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
