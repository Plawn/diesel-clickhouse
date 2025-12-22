//! SQL operators for expressions.


use diesel_clickhouse_types::Bool;
use crate::backend::Backend;
use crate::query_builder::{QueryFragment, AstPass};
use crate::result::QueryResult;
use super::{Expression, SelectableExpression, AppearsOnTable};

// =============================================================================
// Comparison Operators
// =============================================================================

/// Equality comparison (=).
#[derive(Debug, Clone, Copy)]
pub struct Eq<L, R> {
    /// Left side of the comparison.
    pub left: L,
    /// Right side of the comparison.
    pub right: R,
}

impl<L: Expression, R: Expression> Expression for Eq<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for Eq<L, R>
where
    L: SelectableExpression<QS>,
    R: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for Eq<L, R>
where
    L: AppearsOnTable<QS>,
    R: AppearsOnTable<QS>,
{
}

impl<L, R, DB> QueryFragment<DB> for Eq<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" = ");
        self.right.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// Not equal comparison (!=).
#[derive(Debug, Clone, Copy)]
pub struct NotEq<L, R> {
    /// Left side of the comparison.
    pub left: L,
    /// Right side of the comparison.
    pub right: R,
}

impl<L: Expression, R: Expression> Expression for NotEq<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for NotEq<L, R>
where
    L: SelectableExpression<QS>,
    R: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for NotEq<L, R>
where
    L: AppearsOnTable<QS>,
    R: AppearsOnTable<QS>,
{
}

impl<L, R, DB> QueryFragment<DB> for NotEq<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" != ");
        self.right.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// Greater than comparison (>).
#[derive(Debug, Clone, Copy)]
pub struct Gt<L, R> {
    /// Left side of the comparison.
    pub left: L,
    /// Right side of the comparison.
    pub right: R,
}

impl<L: Expression, R: Expression> Expression for Gt<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for Gt<L, R>
where
    L: SelectableExpression<QS>,
    R: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for Gt<L, R>
where
    L: AppearsOnTable<QS>,
    R: AppearsOnTable<QS>,
{
}

impl<L, R, DB> QueryFragment<DB> for Gt<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" > ");
        self.right.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// Greater than or equal comparison (>=).
#[derive(Debug, Clone, Copy)]
pub struct GtEq<L, R> {
    /// Left side of the comparison.
    pub left: L,
    /// Right side of the comparison.
    pub right: R,
}

impl<L: Expression, R: Expression> Expression for GtEq<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for GtEq<L, R>
where
    L: SelectableExpression<QS>,
    R: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for GtEq<L, R>
where
    L: AppearsOnTable<QS>,
    R: AppearsOnTable<QS>,
{
}

impl<L, R, DB> QueryFragment<DB> for GtEq<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" >= ");
        self.right.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// Less than comparison (<).
#[derive(Debug, Clone, Copy)]
pub struct Lt<L, R> {
    /// Left side of the comparison.
    pub left: L,
    /// Right side of the comparison.
    pub right: R,
}

impl<L: Expression, R: Expression> Expression for Lt<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for Lt<L, R>
where
    L: SelectableExpression<QS>,
    R: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for Lt<L, R>
where
    L: AppearsOnTable<QS>,
    R: AppearsOnTable<QS>,
{
}

impl<L, R, DB> QueryFragment<DB> for Lt<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" < ");
        self.right.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// Less than or equal comparison (<=).
#[derive(Debug, Clone, Copy)]
pub struct LtEq<L, R> {
    /// Left side of the comparison.
    pub left: L,
    /// Right side of the comparison.
    pub right: R,
}

impl<L: Expression, R: Expression> Expression for LtEq<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for LtEq<L, R>
where
    L: SelectableExpression<QS>,
    R: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for LtEq<L, R>
where
    L: AppearsOnTable<QS>,
    R: AppearsOnTable<QS>,
{
}

impl<L, R, DB> QueryFragment<DB> for LtEq<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" <= ");
        self.right.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// =============================================================================
// Logical Operators
// =============================================================================

/// Logical AND.
#[derive(Debug, Clone, Copy)]
pub struct And<L, R> {
    /// Left side of the AND.
    pub left: L,
    /// Right side of the AND.
    pub right: R,
}

impl<L: Expression<SqlType = Bool>, R: Expression<SqlType = Bool>> Expression for And<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for And<L, R>
where
    L: SelectableExpression<QS> + Expression<SqlType = Bool>,
    R: SelectableExpression<QS> + Expression<SqlType = Bool>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for And<L, R>
where
    L: AppearsOnTable<QS> + Expression<SqlType = Bool>,
    R: AppearsOnTable<QS> + Expression<SqlType = Bool>,
{
}

impl<L, R, DB> QueryFragment<DB> for And<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" AND ");
        self.right.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Logical OR.
#[derive(Debug, Clone, Copy)]
pub struct Or<L, R> {
    /// Left side of the OR.
    pub left: L,
    /// Right side of the OR.
    pub right: R,
}

impl<L: Expression<SqlType = Bool>, R: Expression<SqlType = Bool>> Expression for Or<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for Or<L, R>
where
    L: SelectableExpression<QS> + Expression<SqlType = Bool>,
    R: SelectableExpression<QS> + Expression<SqlType = Bool>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for Or<L, R>
where
    L: AppearsOnTable<QS> + Expression<SqlType = Bool>,
    R: AppearsOnTable<QS> + Expression<SqlType = Bool>,
{
}

impl<L, R, DB> QueryFragment<DB> for Or<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" OR ");
        self.right.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Logical NOT.
#[derive(Debug, Clone, Copy)]
pub struct Not<E> {
    /// The expression to negate.
    pub expr: E,
}

impl<E: Expression<SqlType = Bool>> Expression for Not<E> {
    type SqlType = Bool;
}

impl<E, QS> SelectableExpression<QS> for Not<E>
where
    E: SelectableExpression<QS> + Expression<SqlType = Bool>,
{
}

impl<E, QS> AppearsOnTable<QS> for Not<E>
where
    E: AppearsOnTable<QS> + Expression<SqlType = Bool>,
{
}

impl<E, DB> QueryFragment<DB> for Not<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("NOT (");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// Null Checks
// =============================================================================

/// IS NULL check.
#[derive(Debug, Clone, Copy)]
pub struct IsNull<E> {
    /// The expression to check.
    pub expr: E,
}

impl<E: Expression> Expression for IsNull<E> {
    type SqlType = Bool;
}

impl<E, QS> SelectableExpression<QS> for IsNull<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for IsNull<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for IsNull<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" IS NULL");
        Ok(())
    }
}

/// IS NOT NULL check.
#[derive(Debug, Clone, Copy)]
pub struct IsNotNull<E> {
    /// The expression to check.
    pub expr: E,
}

impl<E: Expression> Expression for IsNotNull<E> {
    type SqlType = Bool;
}

impl<E, QS> SelectableExpression<QS> for IsNotNull<E>
where
    E: SelectableExpression<QS>,
{
}

impl<E, QS> AppearsOnTable<QS> for IsNotNull<E>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for IsNotNull<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" IS NOT NULL");
        Ok(())
    }
}

// =============================================================================
// String Operators
// =============================================================================

/// LIKE pattern matching.
#[derive(Debug, Clone, Copy)]
pub struct Like<L, R> {
    /// The expression to match.
    pub left: L,
    /// The pattern to match against.
    pub right: R,
}

impl<L: Expression, R: Expression> Expression for Like<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for Like<L, R>
where
    L: SelectableExpression<QS>,
    R: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for Like<L, R>
where
    L: AppearsOnTable<QS>,
    R: AppearsOnTable<QS>,
{
}

impl<L, R, DB> QueryFragment<DB> for Like<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" LIKE ");
        self.right.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// ILIKE case-insensitive pattern matching (ClickHouse uses ilike).
#[derive(Debug, Clone, Copy)]
pub struct ILike<L, R> {
    /// The expression to match.
    pub left: L,
    /// The pattern to match against.
    pub right: R,
}

impl<L: Expression, R: Expression> Expression for ILike<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for ILike<L, R>
where
    L: SelectableExpression<QS>,
    R: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for ILike<L, R>
where
    L: AppearsOnTable<QS>,
    R: AppearsOnTable<QS>,
{
}

impl<L, R, DB> QueryFragment<DB> for ILike<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" ILIKE ");
        self.right.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// =============================================================================
// IN Operator
// =============================================================================

/// IN set membership check.
#[derive(Debug, Clone)]
pub struct In<L, R> {
    /// The expression to check.
    pub left: L,
    /// The set of values.
    pub values: R,
}

impl<L: Expression, R> Expression for In<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for In<L, R>
where
    L: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for In<L, R>
where
    L: AppearsOnTable<QS>,
{
}

/// NOT IN set membership check.
#[derive(Debug, Clone)]
pub struct NotIn<L, R> {
    /// The expression to check.
    pub left: L,
    /// The set of values.
    pub values: R,
}

impl<L: Expression, R> Expression for NotIn<L, R> {
    type SqlType = Bool;
}

impl<L, R, QS> SelectableExpression<QS> for NotIn<L, R>
where
    L: SelectableExpression<QS>,
{
}

impl<L, R, QS> AppearsOnTable<QS> for NotIn<L, R>
where
    L: AppearsOnTable<QS>,
{
}

// QueryFragment implementations for In/NotIn with Vec and slice
// Uses native parameter binding for each element

use crate::backend::ToBindableValue;

impl<L, T, DB> QueryFragment<DB> for In<L, Vec<T>>
where
    L: QueryFragment<DB>,
    T: ToBindableValue,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" IN (");
        for (i, value) in self.values.iter().enumerate() {
            if i > 0 {
                pass.push_sql(", ");
            }
            pass.push_bindable(value)?;
        }
        pass.push_sql(")");
        Ok(())
    }
}

impl<L, T, DB> QueryFragment<DB> for NotIn<L, Vec<T>>
where
    L: QueryFragment<DB>,
    T: ToBindableValue,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" NOT IN (");
        for (i, value) in self.values.iter().enumerate() {
            if i > 0 {
                pass.push_sql(", ");
            }
            pass.push_bindable(value)?;
        }
        pass.push_sql(")");
        Ok(())
    }
}

// Also implement for slices
impl<L, T, DB> QueryFragment<DB> for In<L, &[T]>
where
    L: QueryFragment<DB>,
    T: ToBindableValue,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" IN (");
        for (i, value) in self.values.iter().enumerate() {
            if i > 0 {
                pass.push_sql(", ");
            }
            pass.push_bindable(value)?;
        }
        pass.push_sql(")");
        Ok(())
    }
}

impl<L, T, DB> QueryFragment<DB> for NotIn<L, &[T]>
where
    L: QueryFragment<DB>,
    T: ToBindableValue,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" NOT IN (");
        for (i, value) in self.values.iter().enumerate() {
            if i > 0 {
                pass.push_sql(", ");
            }
            pass.push_bindable(value)?;
        }
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// BETWEEN Operator
// =============================================================================

/// BETWEEN range check.
#[derive(Debug, Clone, Copy)]
pub struct Between<E, L, H> {
    /// The expression to check.
    pub expr: E,
    /// The lower bound.
    pub low: L,
    /// The upper bound.
    pub high: H,
}

impl<E: Expression, L: Expression, H: Expression> Expression for Between<E, L, H> {
    type SqlType = Bool;
}

impl<E, L, H, QS> SelectableExpression<QS> for Between<E, L, H>
where
    E: SelectableExpression<QS>,
    L: SelectableExpression<QS>,
    H: SelectableExpression<QS>,
{
}

impl<E, L, H, QS> AppearsOnTable<QS> for Between<E, L, H>
where
    E: AppearsOnTable<QS>,
    L: AppearsOnTable<QS>,
    H: AppearsOnTable<QS>,
{
}

impl<E, L, H, DB> QueryFragment<DB> for Between<E, L, H>
where
    E: QueryFragment<DB>,
    L: QueryFragment<DB>,
    H: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" BETWEEN ");
        self.low.walk_ast(pass.reborrow())?;
        pass.push_sql(" AND ");
        self.high.walk_ast(pass.reborrow())?;
        Ok(())
    }
}
