//! SQL functions for ClickHouse.


use diesel_clickhouse_types::{UInt64, Float64, CHString, Array, Bool};
use crate::backend::Backend;
use crate::query_builder::{QueryFragment, AstPass};
use crate::result::QueryResult;
use super::{Expression, SelectableExpression, AppearsOnTable};

// =============================================================================
// Aggregate Functions
// =============================================================================

/// COUNT(*) aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct CountStar;

impl Expression for CountStar {
    type SqlType = UInt64;
}

impl<QS> SelectableExpression<QS> for CountStar {}
impl<QS> AppearsOnTable<QS> for CountStar {}

impl<DB: Backend> QueryFragment<DB> for CountStar {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("count(*)");
        Ok(())
    }
}

/// Create a COUNT(*) expression.
pub fn count_star() -> CountStar {
    CountStar
}

/// COUNT(expr) aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Count<E> {
    expr: E,
}

impl<E: Expression> Expression for Count<E> {
    type SqlType = UInt64;
}

impl<E, QS> SelectableExpression<QS> for Count<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Count<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Count<E>
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

/// Create a COUNT(expr) expression.
pub fn count<E: Expression>(expr: E) -> Count<E> {
    Count { expr }
}

/// SUM aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Sum<E> {
    expr: E,
}

impl<E: Expression> Expression for Sum<E> {
    // Sum returns the same type or a larger type
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for Sum<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Sum<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Sum<E>
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

/// Create a SUM expression.
pub fn sum<E: Expression>(expr: E) -> Sum<E> {
    Sum { expr }
}

/// AVG aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Avg<E> {
    expr: E,
}

impl<E: Expression> Expression for Avg<E> {
    type SqlType = Float64;
}

impl<E, QS> SelectableExpression<QS> for Avg<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Avg<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Avg<E>
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

/// Create an AVG expression.
pub fn avg<E: Expression>(expr: E) -> Avg<E> {
    Avg { expr }
}

/// MIN aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Min<E> {
    expr: E,
}

impl<E: Expression> Expression for Min<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for Min<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Min<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Min<E>
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

/// Create a MIN expression.
pub fn min<E: Expression>(expr: E) -> Min<E> {
    Min { expr }
}

/// MAX aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Max<E> {
    expr: E,
}

impl<E: Expression> Expression for Max<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for Max<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Max<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Max<E>
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

/// Create a MAX expression.
pub fn max<E: Expression>(expr: E) -> Max<E> {
    Max { expr }
}

// =============================================================================
// ClickHouse-specific Aggregate Functions
// =============================================================================

/// groupArray aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct GroupArray<E> {
    expr: E,
}

impl<E: Expression> Expression for GroupArray<E> {
    type SqlType = Array<E::SqlType>;
}

impl<E, QS> SelectableExpression<QS> for GroupArray<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for GroupArray<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for GroupArray<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("groupArray(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a groupArray expression.
pub fn group_array<E: Expression>(expr: E) -> GroupArray<E> {
    GroupArray { expr }
}

/// uniq aggregate function (approximate count distinct).
#[derive(Debug, Clone, Copy)]
pub struct Uniq<E> {
    expr: E,
}

impl<E: Expression> Expression for Uniq<E> {
    type SqlType = UInt64;
}

impl<E, QS> SelectableExpression<QS> for Uniq<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Uniq<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Uniq<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("uniq(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a uniq expression.
pub fn uniq<E: Expression>(expr: E) -> Uniq<E> {
    Uniq { expr }
}

// =============================================================================
// Array Functions
// =============================================================================

/// arrayLength function.
#[derive(Debug, Clone, Copy)]
pub struct ArrayLength<E> {
    expr: E,
}

impl<E: Expression> Expression for ArrayLength<E> {
    type SqlType = UInt64;
}

impl<E, QS> SelectableExpression<QS> for ArrayLength<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for ArrayLength<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for ArrayLength<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("length(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an arrayLength expression.
pub fn array_length<E: Expression>(expr: E) -> ArrayLength<E> {
    ArrayLength { expr }
}

/// has function (array contains element).
#[derive(Debug, Clone, Copy)]
pub struct Has<A, E> {
    array: A,
    element: E,
}

impl<A: Expression, E: Expression> Expression for Has<A, E> {
    type SqlType = Bool;
}

impl<A, E, QS> SelectableExpression<QS> for Has<A, E>
where
    A: SelectableExpression<QS>,
    E: SelectableExpression<QS>,
{
}

impl<A, E, QS> AppearsOnTable<QS> for Has<A, E>
where
    A: AppearsOnTable<QS>,
    E: AppearsOnTable<QS>,
{
}

impl<A, E, DB> QueryFragment<DB> for Has<A, E>
where
    A: QueryFragment<DB>,
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("has(");
        self.array.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.element.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a has expression.
pub fn has<A: Expression, E: Expression>(array: A, element: E) -> Has<A, E> {
    Has { array, element }
}

// =============================================================================
// Date/Time Functions
// =============================================================================

/// now() function.
#[derive(Debug, Clone, Copy)]
pub struct Now;

impl Expression for Now {
    type SqlType = diesel_clickhouse_types::DateTime;
}

impl<QS> SelectableExpression<QS> for Now {}
impl<QS> AppearsOnTable<QS> for Now {}

impl<DB: Backend> QueryFragment<DB> for Now {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("now()");
        Ok(())
    }
}

/// Create a now() expression.
pub fn now() -> Now {
    Now
}

/// today() function.
#[derive(Debug, Clone, Copy)]
pub struct Today;

impl Expression for Today {
    type SqlType = diesel_clickhouse_types::Date;
}

impl<QS> SelectableExpression<QS> for Today {}
impl<QS> AppearsOnTable<QS> for Today {}

impl<DB: Backend> QueryFragment<DB> for Today {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("today()");
        Ok(())
    }
}

/// Create a today() expression.
pub fn today() -> Today {
    Today
}

// =============================================================================
// Type Conversion Functions
// =============================================================================

/// toString function.
#[derive(Debug, Clone, Copy)]
pub struct ToString<E> {
    expr: E,
}

impl<E: Expression> Expression for ToString<E> {
    type SqlType = CHString;
}

impl<E, QS> SelectableExpression<QS> for ToString<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for ToString<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for ToString<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("toString(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a toString expression.
pub fn to_string<E: Expression>(expr: E) -> ToString<E> {
    ToString { expr }
}

/// COALESCE function.
#[derive(Debug, Clone)]
pub struct Coalesce<E, D> {
    expr: E,
    default: D,
}

impl<E: Expression, D: Expression<SqlType = E::SqlType>> Expression for Coalesce<E, D> {
    type SqlType = E::SqlType;
}

impl<E, D, QS> SelectableExpression<QS> for Coalesce<E, D>
where
    E: SelectableExpression<QS> + Expression,
    D: SelectableExpression<QS> + Expression<SqlType = E::SqlType>,
{
}

impl<E, D, QS> AppearsOnTable<QS> for Coalesce<E, D>
where
    E: AppearsOnTable<QS> + Expression,
    D: AppearsOnTable<QS> + Expression<SqlType = E::SqlType>,
{
}

impl<E, D, DB> QueryFragment<DB> for Coalesce<E, D>
where
    E: QueryFragment<DB>,
    D: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("coalesce(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.default.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a coalesce expression.
pub fn coalesce<E: Expression, D: Expression>(expr: E, default: D) -> Coalesce<E, D> {
    Coalesce { expr, default }
}
