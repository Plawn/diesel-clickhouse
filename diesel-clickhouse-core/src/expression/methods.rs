//! Expression methods for building queries.

use super::operators::*;
use super::{Expression, AsExpression};
use diesel_clickhouse_types::{SqlType, Bool};

/// Extension methods for all expressions.
pub trait ExpressionMethods: Expression + Sized {
    /// Create an equality comparison.
    fn eq<R>(self, rhs: R) -> Eq<Self, R::Expression>
    where
        R: AsExpression<Self::SqlType>,
    {
        Eq {
            left: self,
            right: rhs.as_expression(),
        }
    }

    /// Create a not-equal comparison.
    fn ne<R>(self, rhs: R) -> NotEq<Self, R::Expression>
    where
        R: AsExpression<Self::SqlType>,
    {
        NotEq {
            left: self,
            right: rhs.as_expression(),
        }
    }

    /// Create a greater-than comparison.
    fn gt<R>(self, rhs: R) -> Gt<Self, R::Expression>
    where
        R: AsExpression<Self::SqlType>,
    {
        Gt {
            left: self,
            right: rhs.as_expression(),
        }
    }

    /// Create a greater-than-or-equal comparison.
    fn ge<R>(self, rhs: R) -> GtEq<Self, R::Expression>
    where
        R: AsExpression<Self::SqlType>,
    {
        GtEq {
            left: self,
            right: rhs.as_expression(),
        }
    }

    /// Create a less-than comparison.
    fn lt<R>(self, rhs: R) -> Lt<Self, R::Expression>
    where
        R: AsExpression<Self::SqlType>,
    {
        Lt {
            left: self,
            right: rhs.as_expression(),
        }
    }

    /// Create a less-than-or-equal comparison.
    fn le<R>(self, rhs: R) -> LtEq<Self, R::Expression>
    where
        R: AsExpression<Self::SqlType>,
    {
        LtEq {
            left: self,
            right: rhs.as_expression(),
        }
    }

    /// Create a BETWEEN range check.
    fn between<L, H>(self, low: L, high: H) -> Between<Self, L::Expression, H::Expression>
    where
        L: AsExpression<Self::SqlType>,
        H: AsExpression<Self::SqlType>,
    {
        Between {
            expr: self,
            low: low.as_expression(),
            high: high.as_expression(),
        }
    }

    /// Check if value is in a set.
    fn eq_any<T>(self, values: T) -> In<Self, T> {
        In {
            left: self,
            values,
        }
    }

    /// Check if value is not in a set.
    fn ne_all<T>(self, values: T) -> NotIn<Self, T> {
        NotIn {
            left: self,
            values,
        }
    }
}

// Blanket implementation for all expressions
impl<T: Expression> ExpressionMethods for T {}

/// Extension methods for nullable expressions.
pub trait NullableExpressionMethods: Expression + Sized {
    /// Check if the value is NULL.
    fn is_null(self) -> IsNull<Self> {
        IsNull { expr: self }
    }

    /// Check if the value is NOT NULL.
    fn is_not_null(self) -> IsNotNull<Self> {
        IsNotNull { expr: self }
    }
}

// Blanket implementation for all expressions
impl<T: Expression> NullableExpressionMethods for T {}

/// Extension methods for boolean expressions.
pub trait BoolExpressionMethods: Expression<SqlType = Bool> + Sized {
    /// Combine with another boolean expression using AND.
    fn and<R>(self, rhs: R) -> And<Self, R::Expression>
    where
        R: AsExpression<Bool>,
    {
        And {
            left: self,
            right: rhs.as_expression(),
        }
    }

    /// Combine with another boolean expression using OR.
    fn or<R>(self, rhs: R) -> Or<Self, R::Expression>
    where
        R: AsExpression<Bool>,
    {
        Or {
            left: self,
            right: rhs.as_expression(),
        }
    }

    /// Negate this boolean expression.
    fn not(self) -> Not<Self> {
        Not { expr: self }
    }
}

// Blanket implementation for boolean expressions
impl<T: Expression<SqlType = Bool>> BoolExpressionMethods for T {}

/// Extension methods for string expressions.
pub trait TextExpressionMethods: Expression + Sized {
    /// LIKE pattern matching.
    fn like<R>(self, pattern: R) -> Like<Self, R::Expression>
    where
        R: AsExpression<diesel_clickhouse_types::CHString>,
    {
        Like {
            left: self,
            right: pattern.as_expression(),
        }
    }

    /// Case-insensitive LIKE pattern matching.
    fn ilike<R>(self, pattern: R) -> ILike<Self, R::Expression>
    where
        R: AsExpression<diesel_clickhouse_types::CHString>,
    {
        ILike {
            left: self,
            right: pattern.as_expression(),
        }
    }
}

// Implement for string types
impl<T: Expression<SqlType = diesel_clickhouse_types::CHString>> TextExpressionMethods for T {}

/// Extension methods for array expressions.
pub trait ArrayExpressionMethods<E: SqlType>: Expression<SqlType = diesel_clickhouse_types::Array<E>> + Sized {
    /// Check if array contains an element.
    fn contains<R>(self, element: R) -> super::functions::Has<Self, R::Expression>
    where
        R: AsExpression<E>,
    {
        super::functions::has(self, element.as_expression())
    }

    /// Get the length of the array.
    fn length(self) -> super::functions::ArrayLength<Self> {
        super::functions::array_length(self)
    }
}

// Implement for array types
impl<T, E: SqlType> ArrayExpressionMethods<E> for T
where
    T: Expression<SqlType = diesel_clickhouse_types::Array<E>>,
{
}

/// Extension for ordering.
pub trait OrderExpressionMethods: Expression + Sized {
    /// Order ascending.
    fn asc(self) -> Asc<Self> {
        Asc { expr: self }
    }

    /// Order descending.
    fn desc(self) -> Desc<Self> {
        Desc { expr: self }
    }

    /// Order ascending with NULLS FIRST.
    fn asc_nulls_first(self) -> AscNullsFirst<Self> {
        AscNullsFirst { expr: self }
    }

    /// Order ascending with NULLS LAST.
    fn asc_nulls_last(self) -> AscNullsLast<Self> {
        AscNullsLast { expr: self }
    }

    /// Order descending with NULLS FIRST.
    fn desc_nulls_first(self) -> DescNullsFirst<Self> {
        DescNullsFirst { expr: self }
    }

    /// Order descending with NULLS LAST.
    fn desc_nulls_last(self) -> DescNullsLast<Self> {
        DescNullsLast { expr: self }
    }
}

impl<T: Expression> OrderExpressionMethods for T {}

// =============================================================================
// Order Direction Types
// =============================================================================

use crate::backend::Backend;
use crate::query_builder::{QueryFragment, AstPass};
use crate::result::QueryResult;

/// Ascending order.
#[derive(Debug, Clone, Copy)]
pub struct Asc<E> {
    pub(crate) expr: E,
}

impl<E: Expression> Expression for Asc<E> {
    type SqlType = E::SqlType;
}

impl<E, DB> QueryFragment<DB> for Asc<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" ASC");
        Ok(())
    }
}

/// Descending order.
#[derive(Debug, Clone, Copy)]
pub struct Desc<E> {
    pub(crate) expr: E,
}

impl<E: Expression> Expression for Desc<E> {
    type SqlType = E::SqlType;
}

impl<E, DB> QueryFragment<DB> for Desc<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" DESC");
        Ok(())
    }
}

/// Ascending order with NULLS FIRST.
#[derive(Debug, Clone, Copy)]
pub struct AscNullsFirst<E> {
    pub(crate) expr: E,
}

impl<E: Expression> Expression for AscNullsFirst<E> {
    type SqlType = E::SqlType;
}

impl<E, DB> QueryFragment<DB> for AscNullsFirst<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" ASC NULLS FIRST");
        Ok(())
    }
}

/// Ascending order with NULLS LAST.
#[derive(Debug, Clone, Copy)]
pub struct AscNullsLast<E> {
    pub(crate) expr: E,
}

impl<E: Expression> Expression for AscNullsLast<E> {
    type SqlType = E::SqlType;
}

impl<E, DB> QueryFragment<DB> for AscNullsLast<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" ASC NULLS LAST");
        Ok(())
    }
}

/// Descending order with NULLS FIRST.
#[derive(Debug, Clone, Copy)]
pub struct DescNullsFirst<E> {
    pub(crate) expr: E,
}

impl<E: Expression> Expression for DescNullsFirst<E> {
    type SqlType = E::SqlType;
}

impl<E, DB> QueryFragment<DB> for DescNullsFirst<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" DESC NULLS FIRST");
        Ok(())
    }
}

/// Descending order with NULLS LAST.
#[derive(Debug, Clone, Copy)]
pub struct DescNullsLast<E> {
    pub(crate) expr: E,
}

impl<E: Expression> Expression for DescNullsLast<E> {
    type SqlType = E::SqlType;
}

impl<E, DB> QueryFragment<DB> for DescNullsLast<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" DESC NULLS LAST");
        Ok(())
    }
}
