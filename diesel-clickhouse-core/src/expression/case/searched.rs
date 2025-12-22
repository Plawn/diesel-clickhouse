//! Searched CASE expressions (CASE WHEN condition THEN result ...)
//!
//! This module provides the searched CASE expression where each branch
//! has its own condition.

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use crate::expression::{AppearsOnTable, Expression, SelectableExpression};

// =============================================================================
// Searched CASE (CASE WHEN condition THEN result ...)
// =============================================================================

/// Builder for a searched CASE expression.
///
/// Created by calling `case_when(condition)`.
#[derive(Debug, Clone, Copy)]
pub struct CaseWhenBuilder<C> {
    pub(crate) condition: C,
}

/// A CASE WHEN with a THEN clause, ready for more WHENs or ELSE.
#[derive(Debug, Clone, Copy)]
pub struct CaseWhenThen<C, T, Rest = ()> {
    pub(crate) condition: C,
    pub(crate) then_value: T,
    pub(crate) rest: Rest,
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
pub(crate) trait SelectableExpressionRest<QS> {}
impl<QS> SelectableExpressionRest<QS> for () {}
impl<C, T, Rest, QS> SelectableExpressionRest<QS> for CaseWhenThen<C, T, Rest>
where
    C: SelectableExpression<QS>,
    T: SelectableExpression<QS>,
    Rest: SelectableExpressionRest<QS>,
{
}

pub(crate) trait AppearsOnTableRest<QS> {}
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
pub(crate) trait QueryFragmentRest<DB: Backend> {
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
