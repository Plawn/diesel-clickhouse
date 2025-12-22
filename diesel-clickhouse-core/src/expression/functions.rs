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

// =============================================================================
// ClickHouse Advanced Aggregate Functions
// =============================================================================

/// quantile aggregate function.
/// Returns the approximate quantile (e.g., quantile(0.5) for median).
#[derive(Debug, Clone, Copy)]
pub struct Quantile<E> {
    level: f64,
    expr: E,
}

impl<E: Expression> Expression for Quantile<E> {
    type SqlType = Float64;
}

impl<E, QS> SelectableExpression<QS> for Quantile<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Quantile<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Quantile<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(&format!("quantile({})(", self.level));
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a quantile expression.
pub fn quantile<E: Expression>(level: f64, expr: E) -> Quantile<E> {
    Quantile { level, expr }
}

/// median aggregate function (same as quantile(0.5)).
pub fn median<E: Expression>(expr: E) -> Quantile<E> {
    Quantile { level: 0.5, expr }
}

/// quantileTDigest - more accurate quantile using t-digest.
#[derive(Debug, Clone, Copy)]
pub struct QuantileTDigest<E> {
    level: f64,
    expr: E,
}

impl<E: Expression> Expression for QuantileTDigest<E> {
    type SqlType = Float64;
}

impl<E, QS> SelectableExpression<QS> for QuantileTDigest<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for QuantileTDigest<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for QuantileTDigest<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(&format!("quantileTDigest({})(", self.level));
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a quantileTDigest expression.
pub fn quantile_tdigest<E: Expression>(level: f64, expr: E) -> QuantileTDigest<E> {
    QuantileTDigest { level, expr }
}

/// argMax - returns the value of the first argument at the maximum value of the second.
#[derive(Debug, Clone, Copy)]
pub struct ArgMax<V, K> {
    value: V,
    key: K,
}

impl<V: Expression, K: Expression> Expression for ArgMax<V, K> {
    type SqlType = V::SqlType;
}

impl<V, K, QS> SelectableExpression<QS> for ArgMax<V, K>
where
    V: SelectableExpression<QS>,
    K: SelectableExpression<QS>,
{
}

impl<V, K, QS> AppearsOnTable<QS> for ArgMax<V, K>
where
    V: AppearsOnTable<QS>,
    K: AppearsOnTable<QS>,
{
}

impl<V, K, DB> QueryFragment<DB> for ArgMax<V, K>
where
    V: QueryFragment<DB>,
    K: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("argMax(");
        self.value.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.key.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an argMax expression.
pub fn arg_max<V: Expression, K: Expression>(value: V, key: K) -> ArgMax<V, K> {
    ArgMax { value, key }
}

/// argMin - returns the value of the first argument at the minimum value of the second.
#[derive(Debug, Clone, Copy)]
pub struct ArgMin<V, K> {
    value: V,
    key: K,
}

impl<V: Expression, K: Expression> Expression for ArgMin<V, K> {
    type SqlType = V::SqlType;
}

impl<V, K, QS> SelectableExpression<QS> for ArgMin<V, K>
where
    V: SelectableExpression<QS>,
    K: SelectableExpression<QS>,
{
}

impl<V, K, QS> AppearsOnTable<QS> for ArgMin<V, K>
where
    V: AppearsOnTable<QS>,
    K: AppearsOnTable<QS>,
{
}

impl<V, K, DB> QueryFragment<DB> for ArgMin<V, K>
where
    V: QueryFragment<DB>,
    K: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("argMin(");
        self.value.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.key.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an argMin expression.
pub fn arg_min<V: Expression, K: Expression>(value: V, key: K) -> ArgMin<V, K> {
    ArgMin { value, key }
}

/// any - returns any value from the group.
#[derive(Debug, Clone, Copy)]
pub struct Any<E> {
    expr: E,
}

impl<E: Expression> Expression for Any<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for Any<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Any<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Any<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("any(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an any expression.
pub fn any<E: Expression>(expr: E) -> Any<E> {
    Any { expr }
}

/// anyLast - returns the last value encountered in the group.
#[derive(Debug, Clone, Copy)]
pub struct AnyLast<E> {
    expr: E,
}

impl<E: Expression> Expression for AnyLast<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for AnyLast<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for AnyLast<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for AnyLast<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("anyLast(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an anyLast expression.
pub fn any_last<E: Expression>(expr: E) -> AnyLast<E> {
    AnyLast { expr }
}

/// topK - returns the most frequent K values.
#[derive(Debug, Clone, Copy)]
pub struct TopK<E> {
    k: u64,
    expr: E,
}

impl<E: Expression> Expression for TopK<E> {
    type SqlType = Array<E::SqlType>;
}

impl<E, QS> SelectableExpression<QS> for TopK<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for TopK<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for TopK<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(&format!("topK({})(", self.k));
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a topK expression.
pub fn top_k<E: Expression>(k: u64, expr: E) -> TopK<E> {
    TopK { k, expr }
}

/// uniqExact - exact count distinct.
#[derive(Debug, Clone, Copy)]
pub struct UniqExact<E> {
    expr: E,
}

impl<E: Expression> Expression for UniqExact<E> {
    type SqlType = UInt64;
}

impl<E, QS> SelectableExpression<QS> for UniqExact<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for UniqExact<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for UniqExact<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("uniqExact(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a uniqExact expression.
pub fn uniq_exact<E: Expression>(expr: E) -> UniqExact<E> {
    UniqExact { expr }
}

/// uniqCombined - approximate count distinct using HyperLogLog.
#[derive(Debug, Clone, Copy)]
pub struct UniqCombined<E> {
    expr: E,
}

impl<E: Expression> Expression for UniqCombined<E> {
    type SqlType = UInt64;
}

impl<E, QS> SelectableExpression<QS> for UniqCombined<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for UniqCombined<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for UniqCombined<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("uniqCombined(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a uniqCombined expression.
pub fn uniq_combined<E: Expression>(expr: E) -> UniqCombined<E> {
    UniqCombined { expr }
}

/// groupUniqArray - collects unique values into an array.
#[derive(Debug, Clone, Copy)]
pub struct GroupUniqArray<E> {
    expr: E,
}

impl<E: Expression> Expression for GroupUniqArray<E> {
    type SqlType = Array<E::SqlType>;
}

impl<E, QS> SelectableExpression<QS> for GroupUniqArray<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for GroupUniqArray<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for GroupUniqArray<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("groupUniqArray(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a groupUniqArray expression.
pub fn group_uniq_array<E: Expression>(expr: E) -> GroupUniqArray<E> {
    GroupUniqArray { expr }
}

/// sumWithOverflow - sum that allows integer overflow.
#[derive(Debug, Clone, Copy)]
pub struct SumWithOverflow<E> {
    expr: E,
}

impl<E: Expression> Expression for SumWithOverflow<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for SumWithOverflow<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for SumWithOverflow<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for SumWithOverflow<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("sumWithOverflow(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a sumWithOverflow expression.
pub fn sum_with_overflow<E: Expression>(expr: E) -> SumWithOverflow<E> {
    SumWithOverflow { expr }
}

/// avgWeighted - weighted average.
#[derive(Debug, Clone, Copy)]
pub struct AvgWeighted<V, W> {
    value: V,
    weight: W,
}

impl<V: Expression, W: Expression> Expression for AvgWeighted<V, W> {
    type SqlType = Float64;
}

impl<V, W, QS> SelectableExpression<QS> for AvgWeighted<V, W>
where
    V: SelectableExpression<QS>,
    W: SelectableExpression<QS>,
{
}

impl<V, W, QS> AppearsOnTable<QS> for AvgWeighted<V, W>
where
    V: AppearsOnTable<QS>,
    W: AppearsOnTable<QS>,
{
}

impl<V, W, DB> QueryFragment<DB> for AvgWeighted<V, W>
where
    V: QueryFragment<DB>,
    W: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("avgWeighted(");
        self.value.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.weight.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an avgWeighted expression.
pub fn avg_weighted<V: Expression, W: Expression>(value: V, weight: W) -> AvgWeighted<V, W> {
    AvgWeighted { value, weight }
}

// =============================================================================
// Additional Array Functions
// =============================================================================

/// arrayJoin - unfolds an array into rows.
#[derive(Debug, Clone, Copy)]
pub struct ArrayJoin<E> {
    expr: E,
}

// ArrayJoin returns the inner type of the array, but we'll use a marker type
impl<E: Expression> Expression for ArrayJoin<E> {
    type SqlType = CHString; // Simplified - actual type depends on array element type
}

impl<E, QS> SelectableExpression<QS> for ArrayJoin<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for ArrayJoin<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for ArrayJoin<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("arrayJoin(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an arrayJoin expression.
pub fn array_join<E: Expression>(expr: E) -> ArrayJoin<E> {
    ArrayJoin { expr }
}

/// arrayConcat - concatenates arrays.
#[derive(Debug, Clone, Copy)]
pub struct ArrayConcat<A, B> {
    first: A,
    second: B,
}

impl<A: Expression, B: Expression> Expression for ArrayConcat<A, B> {
    type SqlType = A::SqlType;
}

impl<A, B, QS> SelectableExpression<QS> for ArrayConcat<A, B>
where
    A: SelectableExpression<QS>,
    B: SelectableExpression<QS>,
{
}

impl<A, B, QS> AppearsOnTable<QS> for ArrayConcat<A, B>
where
    A: AppearsOnTable<QS>,
    B: AppearsOnTable<QS>,
{
}

impl<A, B, DB> QueryFragment<DB> for ArrayConcat<A, B>
where
    A: QueryFragment<DB>,
    B: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("arrayConcat(");
        self.first.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.second.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an arrayConcat expression.
pub fn array_concat<A: Expression, B: Expression>(first: A, second: B) -> ArrayConcat<A, B> {
    ArrayConcat { first, second }
}

// =============================================================================
// String Functions
// =============================================================================

/// lower function - converts string to lowercase.
#[derive(Debug, Clone, Copy)]
pub struct Lower<E> {
    expr: E,
}

impl<E: Expression> Expression for Lower<E> {
    type SqlType = CHString;
}

impl<E, QS> SelectableExpression<QS> for Lower<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Lower<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Lower<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("lower(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a lower expression.
pub fn lower<E: Expression>(expr: E) -> Lower<E> {
    Lower { expr }
}

/// upper function - converts string to uppercase.
#[derive(Debug, Clone, Copy)]
pub struct Upper<E> {
    expr: E,
}

impl<E: Expression> Expression for Upper<E> {
    type SqlType = CHString;
}

impl<E, QS> SelectableExpression<QS> for Upper<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Upper<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Upper<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("upper(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an upper expression.
pub fn upper<E: Expression>(expr: E) -> Upper<E> {
    Upper { expr }
}

/// length function - returns string length.
#[derive(Debug, Clone, Copy)]
pub struct Length<E> {
    expr: E,
}

impl<E: Expression> Expression for Length<E> {
    type SqlType = UInt64;
}

impl<E, QS> SelectableExpression<QS> for Length<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Length<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Length<E>
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

/// Create a length expression.
pub fn length<E: Expression>(expr: E) -> Length<E> {
    Length { expr }
}

/// concat function - concatenates strings.
#[derive(Debug, Clone, Copy)]
pub struct Concat<A, B> {
    first: A,
    second: B,
}

impl<A: Expression, B: Expression> Expression for Concat<A, B> {
    type SqlType = CHString;
}

impl<A, B, QS> SelectableExpression<QS> for Concat<A, B>
where
    A: SelectableExpression<QS>,
    B: SelectableExpression<QS>,
{
}

impl<A, B, QS> AppearsOnTable<QS> for Concat<A, B>
where
    A: AppearsOnTable<QS>,
    B: AppearsOnTable<QS>,
{
}

impl<A, B, DB> QueryFragment<DB> for Concat<A, B>
where
    A: QueryFragment<DB>,
    B: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("concat(");
        self.first.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.second.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a concat expression.
pub fn concat<A: Expression, B: Expression>(first: A, second: B) -> Concat<A, B> {
    Concat { first, second }
}

/// substring function.
#[derive(Debug, Clone, Copy)]
pub struct Substring<E> {
    expr: E,
    offset: i64,
    length: Option<i64>,
}

impl<E: Expression> Expression for Substring<E> {
    type SqlType = CHString;
}

impl<E, QS> SelectableExpression<QS> for Substring<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Substring<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Substring<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("substring(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(&format!(", {}", self.offset));
        if let Some(len) = self.length {
            pass.push_sql(&format!(", {}", len));
        }
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a substring expression.
pub fn substring<E: Expression>(expr: E, offset: i64) -> Substring<E> {
    Substring { expr, offset, length: None }
}

/// Create a substring expression with length.
pub fn substring_with_length<E: Expression>(expr: E, offset: i64, length: i64) -> Substring<E> {
    Substring { expr, offset, length: Some(length) }
}

// =============================================================================
// Hash Functions
// =============================================================================

/// cityHash64 function.
#[derive(Debug, Clone, Copy)]
pub struct CityHash64<E> {
    expr: E,
}

impl<E: Expression> Expression for CityHash64<E> {
    type SqlType = UInt64;
}

impl<E, QS> SelectableExpression<QS> for CityHash64<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for CityHash64<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for CityHash64<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("cityHash64(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a cityHash64 expression.
pub fn city_hash_64<E: Expression>(expr: E) -> CityHash64<E> {
    CityHash64 { expr }
}

/// xxHash64 function.
#[derive(Debug, Clone, Copy)]
pub struct XxHash64<E> {
    expr: E,
}

impl<E: Expression> Expression for XxHash64<E> {
    type SqlType = UInt64;
}

impl<E, QS> SelectableExpression<QS> for XxHash64<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for XxHash64<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for XxHash64<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("xxHash64(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an xxHash64 expression.
pub fn xx_hash_64<E: Expression>(expr: E) -> XxHash64<E> {
    XxHash64 { expr }
}

// =============================================================================
// Date/Time Additional Functions
// =============================================================================

/// toDate function.
#[derive(Debug, Clone, Copy)]
pub struct ToDate<E> {
    expr: E,
}

impl<E: Expression> Expression for ToDate<E> {
    type SqlType = diesel_clickhouse_types::Date;
}

impl<E, QS> SelectableExpression<QS> for ToDate<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for ToDate<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for ToDate<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("toDate(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a toDate expression.
pub fn to_date<E: Expression>(expr: E) -> ToDate<E> {
    ToDate { expr }
}

/// toDateTime function.
#[derive(Debug, Clone, Copy)]
pub struct ToDateTime<E> {
    expr: E,
}

impl<E: Expression> Expression for ToDateTime<E> {
    type SqlType = diesel_clickhouse_types::DateTime;
}

impl<E, QS> SelectableExpression<QS> for ToDateTime<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for ToDateTime<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for ToDateTime<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("toDateTime(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a toDateTime expression.
pub fn to_date_time<E: Expression>(expr: E) -> ToDateTime<E> {
    ToDateTime { expr }
}

/// toYear function.
#[derive(Debug, Clone, Copy)]
pub struct ToYear<E> {
    expr: E,
}

impl<E: Expression> Expression for ToYear<E> {
    type SqlType = diesel_clickhouse_types::UInt16;
}

impl<E, QS> SelectableExpression<QS> for ToYear<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for ToYear<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for ToYear<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("toYear(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a toYear expression.
pub fn to_year<E: Expression>(expr: E) -> ToYear<E> {
    ToYear { expr }
}

/// toMonth function.
#[derive(Debug, Clone, Copy)]
pub struct ToMonth<E> {
    expr: E,
}

impl<E: Expression> Expression for ToMonth<E> {
    type SqlType = diesel_clickhouse_types::UInt8;
}

impl<E, QS> SelectableExpression<QS> for ToMonth<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for ToMonth<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for ToMonth<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("toMonth(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a toMonth expression.
pub fn to_month<E: Expression>(expr: E) -> ToMonth<E> {
    ToMonth { expr }
}

/// toDayOfWeek function.
#[derive(Debug, Clone, Copy)]
pub struct ToDayOfWeek<E> {
    expr: E,
}

impl<E: Expression> Expression for ToDayOfWeek<E> {
    type SqlType = diesel_clickhouse_types::UInt8;
}

impl<E, QS> SelectableExpression<QS> for ToDayOfWeek<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for ToDayOfWeek<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for ToDayOfWeek<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("toDayOfWeek(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a toDayOfWeek expression.
pub fn to_day_of_week<E: Expression>(expr: E) -> ToDayOfWeek<E> {
    ToDayOfWeek { expr }
}

/// toHour function.
#[derive(Debug, Clone, Copy)]
pub struct ToHour<E> {
    expr: E,
}

impl<E: Expression> Expression for ToHour<E> {
    type SqlType = diesel_clickhouse_types::UInt8;
}

impl<E, QS> SelectableExpression<QS> for ToHour<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for ToHour<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for ToHour<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("toHour(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a toHour expression.
pub fn to_hour<E: Expression>(expr: E) -> ToHour<E> {
    ToHour { expr }
}

/// dateDiff function.
#[derive(Debug, Clone)]
pub struct DateDiff<A, B> {
    unit: String,
    start: A,
    end: B,
}

impl<A: Expression, B: Expression> Expression for DateDiff<A, B> {
    type SqlType = diesel_clickhouse_types::Int64;
}

impl<A, B, QS> SelectableExpression<QS> for DateDiff<A, B>
where
    A: SelectableExpression<QS>,
    B: SelectableExpression<QS>,
{
}

impl<A, B, QS> AppearsOnTable<QS> for DateDiff<A, B>
where
    A: AppearsOnTable<QS>,
    B: AppearsOnTable<QS>,
{
}

impl<A, B, DB> QueryFragment<DB> for DateDiff<A, B>
where
    A: QueryFragment<DB>,
    B: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("dateDiff('");
        pass.push_sql(&self.unit);
        pass.push_sql("', ");
        self.start.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.end.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a dateDiff expression.
/// Units: 'second', 'minute', 'hour', 'day', 'week', 'month', 'quarter', 'year'
pub fn date_diff<A: Expression, B: Expression>(unit: &str, start: A, end: B) -> DateDiff<A, B> {
    DateDiff { unit: unit.to_string(), start, end }
}

// =============================================================================
// Math Functions
// =============================================================================

/// abs function.
#[derive(Debug, Clone, Copy)]
pub struct Abs<E> {
    expr: E,
}

impl<E: Expression> Expression for Abs<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for Abs<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Abs<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Abs<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("abs(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create an abs expression.
pub fn abs<E: Expression>(expr: E) -> Abs<E> {
    Abs { expr }
}

/// round function.
#[derive(Debug, Clone, Copy)]
pub struct Round<E> {
    expr: E,
    precision: Option<i32>,
}

impl<E: Expression> Expression for Round<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for Round<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Round<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Round<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("round(");
        self.expr.walk_ast(pass.reborrow())?;
        if let Some(p) = self.precision {
            pass.push_sql(&format!(", {}", p));
        }
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a round expression.
pub fn round<E: Expression>(expr: E) -> Round<E> {
    Round { expr, precision: None }
}

/// Create a round expression with precision.
pub fn round_with_precision<E: Expression>(expr: E, precision: i32) -> Round<E> {
    Round { expr, precision: Some(precision) }
}

/// floor function.
#[derive(Debug, Clone, Copy)]
pub struct Floor<E> {
    expr: E,
}

impl<E: Expression> Expression for Floor<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for Floor<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Floor<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Floor<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("floor(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a floor expression.
pub fn floor<E: Expression>(expr: E) -> Floor<E> {
    Floor { expr }
}

/// ceil function.
#[derive(Debug, Clone, Copy)]
pub struct Ceil<E> {
    expr: E,
}

impl<E: Expression> Expression for Ceil<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> SelectableExpression<QS> for Ceil<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for Ceil<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Ceil<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("ceil(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a ceil expression.
pub fn ceil<E: Expression>(expr: E) -> Ceil<E> {
    Ceil { expr }
}
