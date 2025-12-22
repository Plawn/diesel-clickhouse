//! CASE expression support for conditional logic in queries.
//!
//! This module provides support for SQL CASE expressions:
//! - Simple CASE: `CASE expr WHEN value THEN result ... END`
//! - Searched CASE: `CASE WHEN condition THEN result ... END`
//! - ClickHouse-specific: `if()` and `multiIf()`
//!
//! # Examples
//!
//! ```rust,ignore
//! use diesel_clickhouse::expression::case::{case_when, case, if_, multi_if};
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
//!
//! // ClickHouse IF (shorthand for simple conditions)
//! let tier = if_(amount.gt(100), "high", "low");
//!
//! // ClickHouse multiIf (shorthand for multiple conditions)
//! let tier = multi_if()
//!     .when(amount.gt(1000)).then("premium")
//!     .when(amount.gt(100)).then("standard")
//!     .else_("basic");
//! ```

mod if_expr;
mod searched;
mod simple;

pub use if_expr::*;
pub use searched::*;
pub use simple::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BindCollector, HttpBackend, HttpBindCollector, HttpQueryBuilder, QueryBuilder as _};
    use crate::expression::{Bound, Gt};
    use crate::query_builder::AstPass;
    use diesel_clickhouse_types::{CHString, UInt64};

    /// Build SQL with bindings inlined for testing.
    /// This substitutes placeholders with their actual values.
    fn to_sql<T: crate::query_builder::QueryFragment<HttpBackend>>(fragment: &T) -> String {
        let mut builder = HttpQueryBuilder::default();
        let mut collector = HttpBindCollector::default();
        let pass = AstPass::<HttpBackend>::new(&mut builder, &mut collector);
        fragment.walk_ast(pass).ok();

        // Inline bindings into the SQL for easier test assertions
        let mut sql = builder.finish();
        for binding in collector.bindable_values().iter().rev() {
            // Find and replace the last placeholder with its value
            if let Some(pos) = sql.rfind("{p") {
                if let Some(end) = sql[pos..].find('}') {
                    sql.replace_range(pos..pos + end + 1, &binding.sql_literal());
                }
            }
        }
        sql
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
