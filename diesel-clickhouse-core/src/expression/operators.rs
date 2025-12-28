//! SQL operators for expressions.
//!
//! This module provides type-safe SQL operators using macros to reduce boilerplate.

use diesel_clickhouse_types::Bool;
use crate::backend::Backend;
use crate::query_builder::{QueryFragment, AstPass};
use crate::result::QueryResult;
use super::{Expression, SelectableExpression, AppearsOnTable};

// =============================================================================
// Macros for generating operators
// =============================================================================

/// Generates a binary operator that returns Bool.
/// Format: `left OP right` (e.g., `a = b`, `a > b`)
macro_rules! binary_operator {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $sql_op:literal,
        $left_field:ident,
        $right_field:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $struct_name<L, R> {
            #[doc = concat!("Left side of the ", stringify!($struct_name), ".")]
            pub $left_field: L,
            #[doc = concat!("Right side of the ", stringify!($struct_name), ".")]
            pub $right_field: R,
        }

        impl<L: Expression, R: Expression> Expression for $struct_name<L, R> {
            type SqlType = Bool;
        }

        impl<L, R, QS> SelectableExpression<QS> for $struct_name<L, R>
        where
            L: SelectableExpression<QS>,
            R: SelectableExpression<QS>,
        {}

        impl<L, R, QS> AppearsOnTable<QS> for $struct_name<L, R>
        where
            L: AppearsOnTable<QS>,
            R: AppearsOnTable<QS>,
        {}

        impl<L, R, DB> QueryFragment<DB> for $struct_name<L, R>
        where
            L: QueryFragment<DB>,
            R: QueryFragment<DB>,
            DB: Backend,
        {
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                self.$left_field.walk_ast(pass.reborrow())?;
                pass.push_sql($sql_op);
                self.$right_field.walk_ast(pass.reborrow())?;
                Ok(())
            }
        }
    };
}

/// Generates a binary operator wrapped in parentheses that requires Bool operands.
/// Format: `(left OP right)` (e.g., `(a AND b)`, `(a OR b)`)
macro_rules! wrapped_binary_operator {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $sql_op:literal,
        $left_field:ident,
        $right_field:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $struct_name<L, R> {
            #[doc = concat!("Left side of the ", stringify!($struct_name), ".")]
            pub $left_field: L,
            #[doc = concat!("Right side of the ", stringify!($struct_name), ".")]
            pub $right_field: R,
        }

        impl<L: Expression<SqlType = Bool>, R: Expression<SqlType = Bool>> Expression for $struct_name<L, R> {
            type SqlType = Bool;
        }

        impl<L, R, QS> SelectableExpression<QS> for $struct_name<L, R>
        where
            L: SelectableExpression<QS> + Expression<SqlType = Bool>,
            R: SelectableExpression<QS> + Expression<SqlType = Bool>,
        {}

        impl<L, R, QS> AppearsOnTable<QS> for $struct_name<L, R>
        where
            L: AppearsOnTable<QS> + Expression<SqlType = Bool>,
            R: AppearsOnTable<QS> + Expression<SqlType = Bool>,
        {}

        impl<L, R, DB> QueryFragment<DB> for $struct_name<L, R>
        where
            L: QueryFragment<DB>,
            R: QueryFragment<DB>,
            DB: Backend,
        {
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                pass.push_sql("(");
                self.$left_field.walk_ast(pass.reborrow())?;
                pass.push_sql($sql_op);
                self.$right_field.walk_ast(pass.reborrow())?;
                pass.push_sql(")");
                Ok(())
            }
        }
    };
}

/// Generates a unary operator with suffix syntax.
/// Format: `expr SUFFIX` (e.g., `x IS NULL`)
macro_rules! unary_suffix_operator {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $sql_suffix:literal
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $struct_name<E> {
            /// The expression to check.
            pub expr: E,
        }

        impl<E: Expression> Expression for $struct_name<E> {
            type SqlType = Bool;
        }

        impl<E, QS> SelectableExpression<QS> for $struct_name<E>
        where
            E: SelectableExpression<QS>,
        {}

        impl<E, QS> AppearsOnTable<QS> for $struct_name<E>
        where
            E: AppearsOnTable<QS>,
        {}

        impl<E, DB> QueryFragment<DB> for $struct_name<E>
        where
            E: QueryFragment<DB>,
            DB: Backend,
        {
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                self.expr.walk_ast(pass.reborrow())?;
                pass.push_sql($sql_suffix);
                Ok(())
            }
        }
    };
}

/// Generates a unary operator with prefix syntax that requires Bool operand.
/// Format: `PREFIX (expr)` (e.g., `NOT (x)`)
macro_rules! unary_prefix_operator {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $sql_prefix:literal
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $struct_name<E> {
            /// The expression to negate.
            pub expr: E,
        }

        impl<E: Expression<SqlType = Bool>> Expression for $struct_name<E> {
            type SqlType = Bool;
        }

        impl<E, QS> SelectableExpression<QS> for $struct_name<E>
        where
            E: SelectableExpression<QS> + Expression<SqlType = Bool>,
        {}

        impl<E, QS> AppearsOnTable<QS> for $struct_name<E>
        where
            E: AppearsOnTable<QS> + Expression<SqlType = Bool>,
        {}

        impl<E, DB> QueryFragment<DB> for $struct_name<E>
        where
            E: QueryFragment<DB>,
            DB: Backend,
        {
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                pass.push_sql($sql_prefix);
                self.expr.walk_ast(pass.reborrow())?;
                pass.push_sql(")");
                Ok(())
            }
        }
    };
}

// =============================================================================
// Comparison Operators
// =============================================================================

binary_operator!(
    /// Equality comparison (=).
    Eq, " = ", left, right
);

binary_operator!(
    /// Not equal comparison (!=).
    NotEq, " != ", left, right
);

binary_operator!(
    /// Greater than comparison (>).
    Gt, " > ", left, right
);

binary_operator!(
    /// Greater than or equal comparison (>=).
    GtEq, " >= ", left, right
);

binary_operator!(
    /// Less than comparison (<).
    Lt, " < ", left, right
);

binary_operator!(
    /// Less than or equal comparison (<=).
    LtEq, " <= ", left, right
);

// =============================================================================
// Logical Operators
// =============================================================================

wrapped_binary_operator!(
    /// Logical AND.
    And, " AND ", left, right
);

wrapped_binary_operator!(
    /// Logical OR.
    Or, " OR ", left, right
);

unary_prefix_operator!(
    /// Logical NOT.
    Not, "NOT ("
);

// =============================================================================
// Null Checks
// =============================================================================

unary_suffix_operator!(
    /// IS NULL check.
    IsNull, " IS NULL"
);

unary_suffix_operator!(
    /// IS NOT NULL check.
    IsNotNull, " IS NOT NULL"
);

// =============================================================================
// String Operators
// =============================================================================

binary_operator!(
    /// LIKE pattern matching.
    Like, " LIKE ", left, right
);

binary_operator!(
    /// ILIKE case-insensitive pattern matching (ClickHouse uses ilike).
    ILike, " ILIKE ", left, right
);

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
{}

impl<L, R, QS> AppearsOnTable<QS> for In<L, R>
where
    L: AppearsOnTable<QS>,
{}

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
{}

impl<L, R, QS> AppearsOnTable<QS> for NotIn<L, R>
where
    L: AppearsOnTable<QS>,
{}

// QueryFragment implementations for In/NotIn with Vec and slice
// Uses native parameter binding for each element

use crate::backend::ToBindableValue;

/// Helper macro for IN/NOT IN QueryFragment implementations
macro_rules! impl_in_query_fragment {
    ($struct_name:ident, $sql_keyword:literal, $container:ty) => {
        impl<L, T, DB> QueryFragment<DB> for $struct_name<L, $container>
        where
            L: QueryFragment<DB>,
            T: ToBindableValue,
            DB: Backend,
        {
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                self.left.walk_ast(pass.reborrow())?;
                pass.push_sql($sql_keyword);
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
    };
}

impl_in_query_fragment!(In, " IN (", Vec<T>);
impl_in_query_fragment!(In, " IN (", &[T]);
impl_in_query_fragment!(NotIn, " NOT IN (", Vec<T>);
impl_in_query_fragment!(NotIn, " NOT IN (", &[T]);

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
{}

impl<E, L, H, QS> AppearsOnTable<QS> for Between<E, L, H>
where
    E: AppearsOnTable<QS>,
    L: AppearsOnTable<QS>,
    H: AppearsOnTable<QS>,
{}

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
