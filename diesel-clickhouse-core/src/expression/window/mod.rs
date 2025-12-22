//! Window function support for diesel-clickhouse.
//!
//! This module provides SQL window functions (OVER clause) support.
//!
//! # Examples
//!
//! ```rust,ignore
//! use diesel_clickhouse::expression::window::*;
//!
//! // ROW_NUMBER
//! row_number().over(
//!     Window::new()
//!         .partition_by(users::department)
//!         .order_by(users::salary.desc())
//! )
//!
//! // RANK with partition
//! rank().over(Window::partition_by(category).order_by(price.desc()))
//!
//! // Aggregate as window function
//! sum(orders::amount)
//!     .over(Window::new()
//!         .partition_by(orders::user_id)
//!         .order_by(orders::date)
//!         .rows_between(Preceding::Unbounded, Current))
//!
//! // LAG/LEAD
//! lag(orders::amount, 1, 0)
//!     .over(Window::partition_by(orders::user_id).order_by(orders::date))
//! ```

mod aggregates;
mod frame;
mod functions;
mod over;

pub use aggregates::*;
pub use frame::*;
pub use functions::*;
pub use over::*;

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use crate::expression::Expression;

// =============================================================================
// Window Definition
// =============================================================================

/// A window definition for the OVER clause.
///
/// Built using the builder pattern with `partition_by`, `order_by`, and frame specs.
#[derive(Debug, Clone, Copy)]
pub struct Window<P = (), O = (), F = ()> {
    partition_by: P,
    order_by: O,
    frame: F,
}

impl Window<(), (), ()> {
    /// Create a new empty window definition.
    pub fn new() -> Self {
        Self {
            partition_by: (),
            order_by: (),
            frame: (),
        }
    }
}

impl Default for Window<(), (), ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P, O, F> Window<P, O, F> {
    /// Add or replace PARTITION BY clause.
    pub fn partition_by<P2: Expression>(self, expr: P2) -> Window<P2, O, F> {
        Window {
            partition_by: expr,
            order_by: self.order_by,
            frame: self.frame,
        }
    }

    /// Add or replace ORDER BY clause.
    pub fn order_by<O2: Expression>(self, expr: O2) -> Window<P, O2, F> {
        Window {
            partition_by: self.partition_by,
            order_by: expr,
            frame: self.frame,
        }
    }

    /// Add ROWS BETWEEN frame.
    pub fn rows_between<S: FrameBound, E: FrameBound>(self, start: S, end: E) -> Window<P, O, RowsFrame<S, E>> {
        Window {
            partition_by: self.partition_by,
            order_by: self.order_by,
            frame: RowsFrame { start, end },
        }
    }

    /// Add RANGE BETWEEN frame.
    pub fn range_between<S: FrameBound, E: FrameBound>(self, start: S, end: E) -> Window<P, O, RangeFrame<S, E>> {
        Window {
            partition_by: self.partition_by,
            order_by: self.order_by,
            frame: RangeFrame { start, end },
        }
    }

    /// Add ROWS frame from start to CURRENT ROW.
    pub fn rows_from<S: FrameBound>(self, start: S) -> Window<P, O, RowsFrame<S, CurrentRow>> {
        self.rows_between(start, CurrentRow)
    }

    /// Add RANGE frame from start to CURRENT ROW.
    pub fn range_from<S: FrameBound>(self, start: S) -> Window<P, O, RangeFrame<S, CurrentRow>> {
        self.range_between(start, CurrentRow)
    }
}

// Implement IsWindowClause for Expression types
impl<T: Expression> IsWindowClause for T {
    fn is_empty(&self) -> bool {
        false
    }
}

impl<P, O, F, DB> QueryFragment<DB> for Window<P, O, F>
where
    P: QueryFragment<DB> + IsWindowClause,
    O: QueryFragment<DB> + IsWindowClause,
    F: QueryFragment<DB> + IsWindowClause,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        let has_partition = !self.partition_by.is_empty();
        let has_order = !self.order_by.is_empty();
        let has_frame = !self.frame.is_empty();

        if has_partition {
            pass.push_sql("PARTITION BY ");
            self.partition_by.walk_ast(pass.reborrow())?;
        }

        if has_order {
            if has_partition {
                pass.push_sql(" ");
            }
            pass.push_sql("ORDER BY ");
            self.order_by.walk_ast(pass.reborrow())?;
        }

        if has_frame {
            if has_partition || has_order {
                pass.push_sql(" ");
            }
            self.frame.walk_ast(pass.reborrow())?;
        }

        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BindCollector, HttpBackend, HttpBindCollector, HttpQueryBuilder, QueryBuilder as _};
    use crate::expression::{AppearsOnTable, Bound, SelectableExpression};
    use diesel_clickhouse_types::UInt64;

    // Test column
    #[derive(Debug, Clone, Copy)]
    struct AmountColumn;

    impl Expression for AmountColumn {
        type SqlType = UInt64;
    }
    impl<T> SelectableExpression<T> for AmountColumn {}
    impl<T> AppearsOnTable<T> for AmountColumn {}

    impl<DB: Backend> QueryFragment<DB> for AmountColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("amount");
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct UserIdColumn;

    impl Expression for UserIdColumn {
        type SqlType = UInt64;
    }
    impl<T> SelectableExpression<T> for UserIdColumn {}
    impl<T> AppearsOnTable<T> for UserIdColumn {}

    impl<DB: Backend> QueryFragment<DB> for UserIdColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("user_id");
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct DateColumn;

    impl Expression for DateColumn {
        type SqlType = diesel_clickhouse_types::Date;
    }
    impl<T> SelectableExpression<T> for DateColumn {}
    impl<T> AppearsOnTable<T> for DateColumn {}

    impl<DB: Backend> QueryFragment<DB> for DateColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("date");
            Ok(())
        }
    }

    fn to_sql<T: QueryFragment<HttpBackend>>(fragment: &T) -> String {
        let mut builder = HttpQueryBuilder::default();
        let mut collector = HttpBindCollector::default();
        let pass = AstPass::<HttpBackend>::new(&mut builder, &mut collector);
        fragment.walk_ast(pass).ok();

        // Inline bindings into the SQL for easier test assertions
        let mut sql = builder.finish();
        for binding in collector.bindable_values().iter().rev() {
            if let Some(pos) = sql.rfind("{p") {
                if let Some(end) = sql[pos..].find('}') {
                    sql.replace_range(pos..pos + end + 1, &binding.sql_literal());
                }
            }
        }
        sql
    }

    #[test]
    fn test_row_number() {
        let expr = row_number().over(Window::new());
        let sql = to_sql(&expr);
        assert_eq!(sql, "row_number() OVER ()");
    }

    #[test]
    fn test_row_number_with_partition() {
        let expr = row_number().over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "row_number() OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_row_number_with_order() {
        let expr = row_number().over(Window::new().order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "row_number() OVER (ORDER BY `date`)");
    }

    #[test]
    fn test_row_number_with_partition_and_order() {
        let expr = row_number().over(
            Window::new()
                .partition_by(UserIdColumn)
                .order_by(DateColumn)
        );
        let sql = to_sql(&expr);
        assert_eq!(sql, "row_number() OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_rank() {
        let expr = rank().over(Window::new().order_by(AmountColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "rank() OVER (ORDER BY `amount`)");
    }

    #[test]
    fn test_dense_rank() {
        let expr = dense_rank().over(Window::new().order_by(AmountColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "dense_rank() OVER (ORDER BY `amount`)");
    }

    #[test]
    fn test_ntile() {
        let expr = ntile(4).over(Window::new().order_by(AmountColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "ntile(4) OVER (ORDER BY `amount`)");
    }

    #[test]
    fn test_lag() {
        let expr = lag(AmountColumn, 1, Bound::<_, UInt64>::new(0u64))
            .over(Window::new().partition_by(UserIdColumn).order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "lag(`amount`, 1, 0) OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_lead() {
        let expr = lead(AmountColumn, 1, Bound::<_, UInt64>::new(0u64))
            .over(Window::new().order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "lead(`amount`, 1, 0) OVER (ORDER BY `date`)");
    }

    #[test]
    fn test_first_value() {
        let expr = first_value(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn).order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "first_value(`amount`) OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_last_value() {
        let expr = last_value(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn).order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "last_value(`amount`) OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_nth_value() {
        let expr = nth_value(AmountColumn, 3)
            .over(Window::new().order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "nth_value(`amount`, 3) OVER (ORDER BY `date`)");
    }

    #[test]
    fn test_sum_window() {
        let expr = sum_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn).order_by(DateColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "sum(`amount`) OVER (PARTITION BY `user_id` ORDER BY `date`)");
    }

    #[test]
    fn test_avg_window() {
        let expr = avg_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "avg(`amount`) OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_count_window() {
        let expr = count_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "count(`amount`) OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_min_window() {
        let expr = min_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "min(`amount`) OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_max_window() {
        let expr = max_over(AmountColumn)
            .over(Window::new().partition_by(UserIdColumn));
        let sql = to_sql(&expr);
        assert_eq!(sql, "max(`amount`) OVER (PARTITION BY `user_id`)");
    }

    #[test]
    fn test_rows_between() {
        let expr = sum_over(AmountColumn)
            .over(
                Window::new()
                    .partition_by(UserIdColumn)
                    .order_by(DateColumn)
                    .rows_between(UnboundedPreceding, CurrentRow)
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "sum(`amount`) OVER (PARTITION BY `user_id` ORDER BY `date` ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)"
        );
    }

    #[test]
    fn test_rows_between_n_preceding() {
        let expr = avg_over(AmountColumn)
            .over(
                Window::new()
                    .order_by(DateColumn)
                    .rows_between(Preceding(3), CurrentRow)
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "avg(`amount`) OVER (ORDER BY `date` ROWS BETWEEN 3 PRECEDING AND CURRENT ROW)"
        );
    }

    #[test]
    fn test_range_between() {
        let expr = sum_over(AmountColumn)
            .over(
                Window::new()
                    .order_by(DateColumn)
                    .range_between(UnboundedPreceding, UnboundedFollowing)
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "sum(`amount`) OVER (ORDER BY `date` RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING)"
        );
    }

    #[test]
    fn test_rows_from_shorthand() {
        let expr = sum_over(AmountColumn)
            .over(
                Window::new()
                    .order_by(DateColumn)
                    .rows_from(Preceding(5))
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "sum(`amount`) OVER (ORDER BY `date` ROWS BETWEEN 5 PRECEDING AND CURRENT ROW)"
        );
    }

    #[test]
    fn test_following_frame() {
        let expr = sum_over(AmountColumn)
            .over(
                Window::new()
                    .order_by(DateColumn)
                    .rows_between(CurrentRow, Following(3))
            );
        let sql = to_sql(&expr);
        assert_eq!(
            sql,
            "sum(`amount`) OVER (ORDER BY `date` ROWS BETWEEN CURRENT ROW AND 3 FOLLOWING)"
        );
    }
}
