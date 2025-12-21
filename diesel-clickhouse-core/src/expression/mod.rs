//! Expression system for diesel-clickhouse.
//!
//! This module provides the core traits and types for building SQL expressions
//! in a type-safe manner.

pub mod operators;
pub mod functions;
pub mod methods;
pub mod case;
pub mod subquery;
pub mod window;

use std::marker::PhantomData;

use crate::backend::Backend;
use crate::query_builder::{QueryFragment, AstPass};
use crate::result::QueryResult;
use diesel_clickhouse_types::SqlType;

// Re-exports
pub use operators::*;
pub use functions::*;
pub use methods::ExpressionMethods;
pub use case::{
    case_when, case, if_, multi_if,
    CaseWhenBuilder, CaseWhenThen, CaseWhenElse,
    SimpleCaseBuilder, SimpleCaseWhenThen, SimpleCaseElse,
    If, MultiIf,
};
pub use subquery::{
    Subquery, ScalarSubquery, DerivedTable,
    InSubquery, NotInSubquery, EqAny, NeAll,
    Exists, NotExists, exists, not_exists,
    AsSubquery, SubqueryExpressionMethods,
};
pub use window::{
    // Window definition
    Window, Over,
    // Frame bounds
    UnboundedPreceding, UnboundedFollowing, CurrentRow, Preceding, Following,
    FrameBound, RowsFrame, RangeFrame,
    // Window functions
    RowNumber, row_number,
    Rank, rank,
    DenseRank, dense_rank,
    Ntile, ntile,
    Lag, lag,
    Lead, lead,
    FirstValue, first_value,
    LastValue, last_value,
    NthValue, nth_value,
    // Window aggregates
    WindowAggregate, WindowAggregateExt,
    SumWindow, sum_over,
    AvgWindow, avg_over,
    CountWindow, count_over,
    MinWindow, min_over,
    MaxWindow, max_over,
};

/// The core trait for all SQL expressions.
///
/// Every column, literal, function call, and operation that can appear
/// in a SQL query implements this trait.
pub trait Expression {
    /// The SQL type of this expression.
    type SqlType: SqlType;
}

/// Expressions that can be selected from a query source.
///
/// This is implemented by columns for their own table, and by expressions
/// that combine selectable expressions.
pub trait SelectableExpression<QS>: Expression {}

/// Expressions that can appear in a WHERE clause for a given query source.
///
/// This ensures you can't filter on columns from tables not in the FROM clause.
pub trait AppearsOnTable<QS>: Expression {}

/// Expressions that can be compared for equality.
pub trait EqExpression<Rhs>: Expression {}

/// A boxable expression that can be used for dynamic query building.
pub trait BoxableExpression<QS, DB>:
    Expression + SelectableExpression<QS> + QueryFragment<DB>
where
    DB: Backend,
{
}

// Blanket implementation for BoxableExpression
impl<T, QS, DB> BoxableExpression<QS, DB> for T
where
    T: Expression + SelectableExpression<QS> + QueryFragment<DB>,
    DB: Backend,
{
}

// =============================================================================
// Basic Expression Types
// =============================================================================

/// A bound parameter value.
#[derive(Debug, Clone)]
pub struct Bound<T, ST: SqlType> {
    value: T,
    _marker: PhantomData<ST>,
}

impl<T, ST: SqlType> Bound<T, ST> {
    /// Create a new bound value.
    pub fn new(value: T) -> Self {
        Self {
            value,
            _marker: PhantomData,
        }
    }
}

impl<T, ST: SqlType> Expression for Bound<T, ST> {
    type SqlType = ST;
}

impl<T, ST: SqlType, QS> SelectableExpression<QS> for Bound<T, ST> {}
impl<T, ST: SqlType, QS> AppearsOnTable<QS> for Bound<T, ST> {}

// Implement QueryFragment for Bound with common types
impl<ST: SqlType, DB: crate::backend::Backend> crate::query_builder::QueryFragment<DB> for Bound<&str, ST> {
    fn walk_ast<'b>(&'b self, mut pass: crate::query_builder::AstPass<'_, 'b, DB>) -> crate::result::QueryResult<()> {
        pass.push_sql("'");
        // Escape single quotes
        pass.push_sql(&self.value.replace('\'', "''"));
        pass.push_sql("'");
        Ok(())
    }
}

impl<ST: SqlType, DB: crate::backend::Backend> crate::query_builder::QueryFragment<DB> for Bound<String, ST> {
    fn walk_ast<'b>(&'b self, mut pass: crate::query_builder::AstPass<'_, 'b, DB>) -> crate::result::QueryResult<()> {
        pass.push_sql("'");
        pass.push_sql(&self.value.replace('\'', "''"));
        pass.push_sql("'");
        Ok(())
    }
}

macro_rules! impl_bound_numeric {
    ($($t:ty),*) => {
        $(
            impl<ST: SqlType, DB: crate::backend::Backend> crate::query_builder::QueryFragment<DB> for Bound<$t, ST> {
                fn walk_ast<'b>(&'b self, mut pass: crate::query_builder::AstPass<'_, 'b, DB>) -> crate::result::QueryResult<()> {
                    pass.push_sql(&self.value.to_string());
                    Ok(())
                }
            }
        )*
    };
}

impl_bound_numeric!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, f32, f64);

impl<ST: SqlType, DB: crate::backend::Backend> crate::query_builder::QueryFragment<DB> for Bound<bool, ST> {
    fn walk_ast<'b>(&'b self, mut pass: crate::query_builder::AstPass<'_, 'b, DB>) -> crate::result::QueryResult<()> {
        pass.push_sql(if self.value { "true" } else { "false" });
        Ok(())
    }
}

impl<ST: SqlType, DB: crate::backend::Backend> crate::query_builder::QueryFragment<DB> for Bound<&bool, ST> {
    fn walk_ast<'b>(&'b self, mut pass: crate::query_builder::AstPass<'_, 'b, DB>) -> crate::result::QueryResult<()> {
        pass.push_sql(if *self.value { "true" } else { "false" });
        Ok(())
    }
}

/// A raw SQL expression (use with caution).
#[derive(Debug, Clone)]
pub struct SqlLiteral<ST: SqlType> {
    sql: String,
    _marker: PhantomData<ST>,
}

impl<ST: SqlType> SqlLiteral<ST> {
    /// Create a new SQL literal.
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            _marker: PhantomData,
        }
    }
}

impl<ST: SqlType> Expression for SqlLiteral<ST> {
    type SqlType = ST;
}

impl<ST: SqlType, QS> SelectableExpression<QS> for SqlLiteral<ST> {}
impl<ST: SqlType, QS> AppearsOnTable<QS> for SqlLiteral<ST> {}

impl<ST: SqlType, DB: Backend> QueryFragment<DB> for SqlLiteral<ST> {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(&self.sql);
        Ok(())
    }
}

/// Create a raw SQL literal expression.
///
/// # Safety
///
/// The SQL string is inserted directly into the query without escaping.
/// Never use this with user-provided input!
pub fn sql<ST: SqlType>(sql: impl Into<String>) -> SqlLiteral<ST> {
    SqlLiteral::new(sql)
}

/// A tuple of expressions (for SELECT multiple columns).
#[derive(Debug, Clone, Copy)]
pub struct ExpressionTuple<T>(pub T);

// Implement for common tuple sizes
macro_rules! impl_expression_tuple {
    ($($T:ident),+) => {
        impl<$($T: Expression),+> Expression for ($($T,)+) {
            type SqlType = ($($T::SqlType,)+);
        }

        impl<QS, $($T: SelectableExpression<QS>),+> SelectableExpression<QS> for ($($T,)+) {}
        impl<QS, $($T: AppearsOnTable<QS>),+> AppearsOnTable<QS> for ($($T,)+) {}
    };
}

impl_expression_tuple!(A);
impl_expression_tuple!(A, B);
impl_expression_tuple!(A, B, C);
impl_expression_tuple!(A, B, C, D);
impl_expression_tuple!(A, B, C, D, E);
impl_expression_tuple!(A, B, C, D, E, F);
impl_expression_tuple!(A, B, C, D, E, F, G);
impl_expression_tuple!(A, B, C, D, E, F, G, H);

// =============================================================================
// Star expression (SELECT *)
// =============================================================================

/// The * (star) expression for selecting all columns.
#[derive(Debug, Clone, Copy)]
pub struct Star<T>(PhantomData<T>);

impl<T> Star<T> {
    /// Create a new star expression.
    pub const fn new() -> Self {
        Star(PhantomData)
    }
}

impl<T> Default for Star<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: crate::query_source::Table> Expression for Star<T> {
    type SqlType = <T as crate::query_source::Table>::AllColumnsSqlType;
}

impl<T: crate::query_source::Table> SelectableExpression<T> for Star<T> {}

impl<T, DB: Backend> QueryFragment<DB> for Star<T> {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("*");
        Ok(())
    }
}

// =============================================================================
// Aliased expression
// =============================================================================

/// An expression with an alias (AS clause).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Aliased<E, A> {
    expression: E,
    alias: A,
}

impl<E: Expression, A> Expression for Aliased<E, A> {
    type SqlType = E::SqlType;
}

impl<E: SelectableExpression<QS>, A, QS> SelectableExpression<QS> for Aliased<E, A> {}

/// Extension trait for adding an alias to an expression.
pub trait AsExpression<ST: SqlType> {
    /// The expression type.
    type Expression: Expression<SqlType = ST>;

    /// Convert to an expression.
    fn as_expression(self) -> Self::Expression;
}

impl<T: Expression> AsExpression<T::SqlType> for T {
    type Expression = T;

    fn as_expression(self) -> Self::Expression {
        self
    }
}

// Implementations for common Rust types
macro_rules! impl_as_expression {
    ($rust_ty:ty, $sql_ty:ty) => {
        impl AsExpression<$sql_ty> for $rust_ty {
            type Expression = Bound<$rust_ty, $sql_ty>;

            fn as_expression(self) -> Self::Expression {
                Bound::new(self)
            }
        }

        impl<'a> AsExpression<$sql_ty> for &'a $rust_ty {
            type Expression = Bound<&'a $rust_ty, $sql_ty>;

            fn as_expression(self) -> Self::Expression {
                Bound::new(self)
            }
        }
    };
}

use diesel_clickhouse_types::*;

impl_as_expression!(u8, UInt8);
impl_as_expression!(u16, UInt16);
impl_as_expression!(u32, UInt32);
impl_as_expression!(u64, UInt64);
impl_as_expression!(i8, Int8);
impl_as_expression!(i16, Int16);
impl_as_expression!(i32, Int32);
impl_as_expression!(i64, Int64);
impl_as_expression!(f32, Float32);
impl_as_expression!(f64, Float64);
impl_as_expression!(bool, Bool);
impl_as_expression!(String, CHString);

impl<'a> AsExpression<CHString> for &'a str {
    type Expression = Bound<&'a str, CHString>;

    fn as_expression(self) -> Self::Expression {
        Bound::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_literal() {
        let lit: SqlLiteral<UInt64> = sql("1 + 2");
        assert_eq!(std::any::TypeId::of::<UInt64>(),
                   std::any::TypeId::of::<<SqlLiteral<UInt64> as Expression>::SqlType>());
    }
}
