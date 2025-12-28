//! SQL functions for ClickHouse.
//!
//! This module provides type-safe SQL function wrappers for ClickHouse.
//! Functions are generated using macros to reduce boilerplate.

use diesel_clickhouse_types::{UInt64, UInt16, UInt8, Int64, Float64, CHString, Array, Bool};
use crate::backend::Backend;
use crate::query_builder::{QueryFragment, AstPass};
use crate::result::QueryResult;
use super::{Expression, SelectableExpression, AppearsOnTable};

// =============================================================================
// Macros for generating SQL functions
// =============================================================================

/// Generates a nullary SQL function (no arguments).
/// Example: `now()`, `today()`, `count(*)`
macro_rules! sql_function_nullary {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $fn_name:ident,
        $sql:literal,
        $return_type:ty
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $struct_name;

        impl Expression for $struct_name {
            type SqlType = $return_type;
        }

        impl<QS> SelectableExpression<QS> for $struct_name {}
        impl<QS> AppearsOnTable<QS> for $struct_name {}

        impl<DB: Backend> QueryFragment<DB> for $struct_name {
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                pass.push_sql($sql);
                Ok(())
            }
        }

        $(#[$meta])*
        pub fn $fn_name() -> $struct_name {
            $struct_name
        }
    };
}

/// Generates a unary SQL function with a fixed return type.
/// Example: `count(expr)` -> UInt64, `avg(expr)` -> Float64
macro_rules! sql_function_unary {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $fn_name:ident,
        $sql:literal,
        $return_type:ty
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $struct_name<E> {
            expr: E,
        }

        impl<E: Expression> Expression for $struct_name<E> {
            type SqlType = $return_type;
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
                pass.push_sql(concat!($sql, "("));
                self.expr.walk_ast(pass.reborrow())?;
                pass.push_sql(")");
                Ok(())
            }
        }

        $(#[$meta])*
        pub fn $fn_name<E: Expression>(expr: E) -> $struct_name<E> {
            $struct_name { expr }
        }
    };
}

/// Generates a unary SQL function that preserves the input type.
/// Example: `min(expr)`, `max(expr)`, `sum(expr)`
macro_rules! sql_function_unary_preserve_type {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $fn_name:ident,
        $sql:literal
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $struct_name<E> {
            expr: E,
        }

        impl<E: Expression> Expression for $struct_name<E> {
            type SqlType = E::SqlType;
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
                pass.push_sql(concat!($sql, "("));
                self.expr.walk_ast(pass.reborrow())?;
                pass.push_sql(")");
                Ok(())
            }
        }

        $(#[$meta])*
        pub fn $fn_name<E: Expression>(expr: E) -> $struct_name<E> {
            $struct_name { expr }
        }
    };
}

/// Generates a unary SQL function that returns Array<E::SqlType>.
/// Example: `groupArray(expr)`, `groupUniqArray(expr)`
macro_rules! sql_function_unary_to_array {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $fn_name:ident,
        $sql:literal
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $struct_name<E> {
            expr: E,
        }

        impl<E: Expression> Expression for $struct_name<E> {
            type SqlType = Array<E::SqlType>;
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
                pass.push_sql(concat!($sql, "("));
                self.expr.walk_ast(pass.reborrow())?;
                pass.push_sql(")");
                Ok(())
            }
        }

        $(#[$meta])*
        pub fn $fn_name<E: Expression>(expr: E) -> $struct_name<E> {
            $struct_name { expr }
        }
    };
}

/// Generates a binary SQL function with a fixed return type.
/// Example: `has(array, element)` -> Bool
macro_rules! sql_function_binary {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $fn_name:ident,
        $sql:literal,
        $return_type:ty,
        $field1:ident,
        $field2:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $struct_name<A, B> {
            $field1: A,
            $field2: B,
        }

        impl<A: Expression, B: Expression> Expression for $struct_name<A, B> {
            type SqlType = $return_type;
        }

        impl<A, B, QS> SelectableExpression<QS> for $struct_name<A, B>
        where
            A: SelectableExpression<QS>,
            B: SelectableExpression<QS>,
        {}

        impl<A, B, QS> AppearsOnTable<QS> for $struct_name<A, B>
        where
            A: AppearsOnTable<QS>,
            B: AppearsOnTable<QS>,
        {}

        impl<A, B, DB> QueryFragment<DB> for $struct_name<A, B>
        where
            A: QueryFragment<DB>,
            B: QueryFragment<DB>,
            DB: Backend,
        {
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                pass.push_sql(concat!($sql, "("));
                self.$field1.walk_ast(pass.reborrow())?;
                pass.push_sql(", ");
                self.$field2.walk_ast(pass.reborrow())?;
                pass.push_sql(")");
                Ok(())
            }
        }

        $(#[$meta])*
        pub fn $fn_name<A: Expression, B: Expression>($field1: A, $field2: B) -> $struct_name<A, B> {
            $struct_name { $field1, $field2 }
        }
    };
}

/// Generates a binary SQL function that preserves the first argument's type.
/// Example: `coalesce(expr, default)` -> E::SqlType
macro_rules! sql_function_binary_preserve_first {
    (
        $(#[$meta:meta])*
        $struct_name:ident,
        $fn_name:ident,
        $sql:literal,
        $field1:ident,
        $field2:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $struct_name<A, B> {
            $field1: A,
            $field2: B,
        }

        impl<A: Expression, B: Expression<SqlType = A::SqlType>> Expression for $struct_name<A, B> {
            type SqlType = A::SqlType;
        }

        impl<A, B, QS> SelectableExpression<QS> for $struct_name<A, B>
        where
            A: SelectableExpression<QS> + Expression,
            B: SelectableExpression<QS> + Expression<SqlType = A::SqlType>,
        {}

        impl<A, B, QS> AppearsOnTable<QS> for $struct_name<A, B>
        where
            A: AppearsOnTable<QS> + Expression,
            B: AppearsOnTable<QS> + Expression<SqlType = A::SqlType>,
        {}

        impl<A, B, DB> QueryFragment<DB> for $struct_name<A, B>
        where
            A: QueryFragment<DB>,
            B: QueryFragment<DB>,
            DB: Backend,
        {
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                pass.push_sql(concat!($sql, "("));
                self.$field1.walk_ast(pass.reborrow())?;
                pass.push_sql(", ");
                self.$field2.walk_ast(pass.reborrow())?;
                pass.push_sql(")");
                Ok(())
            }
        }

        $(#[$meta])*
        pub fn $fn_name<A: Expression, B: Expression>($field1: A, $field2: B) -> $struct_name<A, B> {
            $struct_name { $field1, $field2 }
        }
    };
}

// =============================================================================
// Nullary Functions (no arguments)
// =============================================================================

sql_function_nullary!(
    /// COUNT(*) aggregate function.
    CountStar, count_star, "count(*)", UInt64
);

sql_function_nullary!(
    /// now() function - returns current DateTime.
    Now, now, "now()", diesel_clickhouse_types::DateTime
);

sql_function_nullary!(
    /// today() function - returns current Date.
    Today, today, "today()", diesel_clickhouse_types::Date
);

// =============================================================================
// Aggregate Functions
// =============================================================================

sql_function_unary!(
    /// COUNT(expr) aggregate function.
    Count, count, "count", UInt64
);

sql_function_unary!(
    /// AVG aggregate function - always returns Float64.
    Avg, avg, "avg", Float64
);

sql_function_unary_preserve_type!(
    /// SUM aggregate function - preserves input type.
    Sum, sum, "sum"
);

sql_function_unary_preserve_type!(
    /// MIN aggregate function - preserves input type.
    Min, min, "min"
);

sql_function_unary_preserve_type!(
    /// MAX aggregate function - preserves input type.
    Max, max, "max"
);

sql_function_unary_preserve_type!(
    /// any aggregate function - returns any value from the group.
    Any, any, "any"
);

sql_function_unary_preserve_type!(
    /// anyLast aggregate function - returns the last value encountered.
    AnyLast, any_last, "anyLast"
);

sql_function_unary_preserve_type!(
    /// sumWithOverflow - sum that allows integer overflow.
    SumWithOverflow, sum_with_overflow, "sumWithOverflow"
);

// =============================================================================
// ClickHouse-specific Aggregate Functions
// =============================================================================

sql_function_unary!(
    /// uniq aggregate function (approximate count distinct using HyperLogLog++).
    Uniq, uniq, "uniq", UInt64
);

sql_function_unary!(
    /// uniqExact - exact count distinct (slower but precise).
    UniqExact, uniq_exact, "uniqExact", UInt64
);

sql_function_unary!(
    /// uniqCombined - approximate count distinct using combined algorithm.
    UniqCombined, uniq_combined, "uniqCombined", UInt64
);

sql_function_unary_to_array!(
    /// groupArray aggregate function - collects values into an array.
    GroupArray, group_array, "groupArray"
);

sql_function_unary_to_array!(
    /// groupUniqArray - collects unique values into an array.
    GroupUniqArray, group_uniq_array, "groupUniqArray"
);

// =============================================================================
// Array Functions
// =============================================================================

sql_function_unary!(
    /// length function - returns array or string length.
    ArrayLength, array_length, "length", UInt64
);

sql_function_unary!(
    /// arrayJoin - unfolds an array into rows.
    /// Note: Returns simplified CHString type; actual type depends on array element.
    ArrayJoin, array_join, "arrayJoin", CHString
);

sql_function_binary!(
    /// has function - checks if array contains element.
    Has, has, "has", Bool, array, element
);

sql_function_binary_preserve_first!(
    /// arrayConcat - concatenates two arrays.
    ArrayConcat, array_concat, "arrayConcat", first, second
);

// =============================================================================
// String Functions
// =============================================================================

sql_function_unary!(
    /// lower function - converts string to lowercase.
    Lower, lower, "lower", CHString
);

sql_function_unary!(
    /// upper function - converts string to uppercase.
    Upper, upper, "upper", CHString
);

sql_function_unary!(
    /// length function for strings - returns string length.
    Length, length, "length", UInt64
);

sql_function_unary!(
    /// toString function - converts any value to String.
    ToString, to_string, "toString", CHString
);

sql_function_binary!(
    /// concat function - concatenates two strings.
    Concat, concat, "concat", CHString, first, second
);

// =============================================================================
// Hash Functions
// =============================================================================

sql_function_unary!(
    /// cityHash64 function - CityHash64 hash.
    CityHash64, city_hash_64, "cityHash64", UInt64
);

sql_function_unary!(
    /// xxHash64 function - xxHash64 hash.
    XxHash64, xx_hash_64, "xxHash64", UInt64
);

// =============================================================================
// Date/Time Functions
// =============================================================================

sql_function_unary!(
    /// toDate function - converts to Date.
    ToDate, to_date, "toDate", diesel_clickhouse_types::Date
);

sql_function_unary!(
    /// toDateTime function - converts to DateTime.
    ToDateTime, to_date_time, "toDateTime", diesel_clickhouse_types::DateTime
);

sql_function_unary!(
    /// toYear function - extracts year from date/datetime.
    ToYear, to_year, "toYear", UInt16
);

sql_function_unary!(
    /// toMonth function - extracts month (1-12) from date/datetime.
    ToMonth, to_month, "toMonth", UInt8
);

sql_function_unary!(
    /// toDayOfWeek function - extracts day of week (1-7, Monday=1).
    ToDayOfWeek, to_day_of_week, "toDayOfWeek", UInt8
);

sql_function_unary!(
    /// toHour function - extracts hour (0-23) from datetime.
    ToHour, to_hour, "toHour", UInt8
);

// =============================================================================
// Math Functions
// =============================================================================

sql_function_unary_preserve_type!(
    /// abs function - absolute value.
    Abs, abs, "abs"
);

sql_function_unary_preserve_type!(
    /// floor function - rounds down to nearest integer.
    Floor, floor, "floor"
);

sql_function_unary_preserve_type!(
    /// ceil function - rounds up to nearest integer.
    Ceil, ceil, "ceil"
);

// =============================================================================
// Binary Aggregate Functions
// =============================================================================

sql_function_binary!(
    /// argMax - returns value at the maximum of key.
    ArgMax, arg_max, "argMax", Float64, value, key
);

sql_function_binary!(
    /// argMin - returns value at the minimum of key.
    ArgMin, arg_min, "argMin", Float64, value, key
);

sql_function_binary!(
    /// avgWeighted - weighted average.
    AvgWeighted, avg_weighted, "avgWeighted", Float64, value, weight
);

sql_function_binary_preserve_first!(
    /// coalesce function - returns first non-null value.
    Coalesce, coalesce, "coalesce", expr, default
);

// =============================================================================
// Special Functions (require custom implementations)
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
{}

impl<E, QS> AppearsOnTable<QS> for Quantile<E>
where
    E: AppearsOnTable<QS>,
{}

impl<E, DB> QueryFragment<DB> for Quantile<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("quantile(");
        pass.push_bindable(&self.level)?;
        pass.push_sql(")(");
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

/// quantileTDigest - more accurate quantile using t-digest algorithm.
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
{}

impl<E, QS> AppearsOnTable<QS> for QuantileTDigest<E>
where
    E: AppearsOnTable<QS>,
{}

impl<E, DB> QueryFragment<DB> for QuantileTDigest<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("quantileTDigest(");
        pass.push_bindable(&self.level)?;
        pass.push_sql(")(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a quantileTDigest expression.
pub fn quantile_tdigest<E: Expression>(level: f64, expr: E) -> QuantileTDigest<E> {
    QuantileTDigest { level, expr }
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
{}

impl<E, QS> AppearsOnTable<QS> for TopK<E>
where
    E: AppearsOnTable<QS>,
{}

impl<E, DB> QueryFragment<DB> for TopK<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("topK(");
        pass.push_bindable(&(self.k as i64))?;
        pass.push_sql(")(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a topK expression.
pub fn top_k<E: Expression>(k: u64, expr: E) -> TopK<E> {
    TopK { k, expr }
}

/// substring function with offset and optional length.
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
{}

impl<E, QS> AppearsOnTable<QS> for Substring<E>
where
    E: AppearsOnTable<QS>,
{}

impl<E, DB> QueryFragment<DB> for Substring<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("substring(");
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        pass.push_bindable(&self.offset)?;
        if let Some(len) = self.length {
            pass.push_sql(", ");
            pass.push_bindable(&len)?;
        }
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a substring expression starting at offset.
pub fn substring<E: Expression>(expr: E, offset: i64) -> Substring<E> {
    Substring { expr, offset, length: None }
}

/// Create a substring expression with offset and length.
pub fn substring_with_length<E: Expression>(expr: E, offset: i64, length: i64) -> Substring<E> {
    Substring { expr, offset, length: Some(length) }
}

/// round function with optional precision.
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
{}

impl<E, QS> AppearsOnTable<QS> for Round<E>
where
    E: AppearsOnTable<QS>,
{}

impl<E, DB> QueryFragment<DB> for Round<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("round(");
        self.expr.walk_ast(pass.reborrow())?;
        if let Some(p) = self.precision {
            pass.push_sql(", ");
            pass.push_bindable(&p)?;
        }
        pass.push_sql(")");
        Ok(())
    }
}

/// Create a round expression (rounds to integer).
pub fn round<E: Expression>(expr: E) -> Round<E> {
    Round { expr, precision: None }
}

/// Create a round expression with specified precision.
pub fn round_with_precision<E: Expression>(expr: E, precision: i32) -> Round<E> {
    Round { expr, precision: Some(precision) }
}

/// dateDiff function - calculates difference between dates in specified unit.
#[derive(Debug, Clone)]
pub struct DateDiff<A, B> {
    unit: &'static str,
    start: A,
    end: B,
}

impl<A: Expression, B: Expression> Expression for DateDiff<A, B> {
    type SqlType = Int64;
}

impl<A, B, QS> SelectableExpression<QS> for DateDiff<A, B>
where
    A: SelectableExpression<QS>,
    B: SelectableExpression<QS>,
{}

impl<A, B, QS> AppearsOnTable<QS> for DateDiff<A, B>
where
    A: AppearsOnTable<QS>,
    B: AppearsOnTable<QS>,
{}

impl<A, B, DB> QueryFragment<DB> for DateDiff<A, B>
where
    A: QueryFragment<DB>,
    B: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("dateDiff('");
        pass.push_sql(self.unit);
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
pub fn date_diff<A: Expression, B: Expression>(unit: &'static str, start: A, end: B) -> DateDiff<A, B> {
    DateDiff { unit, start, end }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::build_sql_inlined;

    #[test]
    fn test_nullary_functions() {
        assert_eq!(build_sql_inlined(&count_star()), "count(*)");
        assert_eq!(build_sql_inlined(&now()), "now()");
        assert_eq!(build_sql_inlined(&today()), "today()");
    }

    #[test]
    fn test_quantile_formatting() {
        use crate::expression::sql;
        use diesel_clickhouse_types::UInt64;

        let expr: crate::expression::SqlLiteral<UInt64> = sql("value");
        let q = quantile(0.5, expr);
        assert_eq!(build_sql_inlined(&q), "quantile(0.5)(value)");
    }

    #[test]
    fn test_top_k_formatting() {
        use crate::expression::sql;
        use diesel_clickhouse_types::UInt64;

        let expr: crate::expression::SqlLiteral<UInt64> = sql("value");
        let tk = top_k(10, expr);
        assert_eq!(build_sql_inlined(&tk), "topK(10)(value)");
    }
}
