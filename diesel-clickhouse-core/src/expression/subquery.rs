//! Subquery support for diesel-clickhouse.
//!
//! This module provides support for using SELECT statements as:
//! - Scalar subqueries (single value in SELECT or WHERE)
//! - IN/NOT IN subqueries
//! - EXISTS/NOT EXISTS subqueries
//! - FROM clause subqueries (derived tables)
//!
//! # Examples
//!
//! ```rust,ignore
//! use diesel_clickhouse::expression::subquery::*;
//!
//! // IN subquery
//! users::table.filter(
//!     users::id.eq_any(
//!         orders::table
//!             .select(orders::user_id)
//!             .filter(orders::total.gt(100))
//!             .as_subquery()
//!     )
//! )
//!
//! // Scalar subquery
//! users::table.select((
//!     users::name,
//!     orders::table
//!         .select(count_star())
//!         .filter(orders::user_id.eq(users::id))
//!         .single_value()
//! ))
//!
//! // EXISTS
//! users::table.filter(
//!     exists(
//!         orders::table
//!             .filter(orders::user_id.eq(users::id))
//!     )
//! )
//! ```

use std::marker::PhantomData;

use diesel_clickhouse_types::{Bool, SqlType};

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

use super::{AppearsOnTable, Expression, SelectableExpression};

// =============================================================================
// Subquery wrapper
// =============================================================================

/// A subquery that can be used in various contexts.
///
/// This wraps a SELECT statement and allows it to be used as:
/// - A value in IN/NOT IN
/// - A scalar value (when selecting a single column)
/// - Part of an EXISTS check
#[derive(Debug, Clone, Copy)]
pub struct Subquery<Q, ST: SqlType> {
    query: Q,
    _marker: PhantomData<ST>,
}

impl<Q, ST: SqlType> Subquery<Q, ST> {
    /// Create a new subquery.
    pub fn new(query: Q) -> Self {
        Self {
            query,
            _marker: PhantomData,
        }
    }
}

impl<Q, ST: SqlType> Expression for Subquery<Q, ST> {
    type SqlType = ST;
}

impl<Q, ST: SqlType, QS> SelectableExpression<QS> for Subquery<Q, ST> {}
impl<Q, ST: SqlType, QS> AppearsOnTable<QS> for Subquery<Q, ST> {}

impl<Q, ST: SqlType, DB> QueryFragment<DB> for Subquery<Q, ST>
where
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        self.query.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// Scalar subquery (single value)
// =============================================================================

/// A scalar subquery that returns exactly one value.
///
/// Created by calling `.single_value()` on a query.
#[derive(Debug, Clone, Copy)]
pub struct ScalarSubquery<Q, ST: SqlType> {
    query: Q,
    _marker: PhantomData<ST>,
}

impl<Q, ST: SqlType> ScalarSubquery<Q, ST> {
    /// Create a new scalar subquery.
    pub fn new(query: Q) -> Self {
        Self {
            query,
            _marker: PhantomData,
        }
    }
}

impl<Q, ST: SqlType> Expression for ScalarSubquery<Q, ST> {
    type SqlType = ST;
}

impl<Q, ST: SqlType, QS> SelectableExpression<QS> for ScalarSubquery<Q, ST> {}
impl<Q, ST: SqlType, QS> AppearsOnTable<QS> for ScalarSubquery<Q, ST> {}

impl<Q, ST: SqlType, DB> QueryFragment<DB> for ScalarSubquery<Q, ST>
where
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        self.query.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// IN subquery
// =============================================================================

/// expr IN (SELECT ...)
#[derive(Debug, Clone, Copy)]
pub struct InSubquery<E, Q> {
    expr: E,
    subquery: Q,
}

impl<E, Q> InSubquery<E, Q> {
    /// Create a new IN subquery expression.
    pub fn new(expr: E, subquery: Q) -> Self {
        Self { expr, subquery }
    }
}

impl<E: Expression, Q> Expression for InSubquery<E, Q> {
    type SqlType = Bool;
}

impl<E, Q, QS> SelectableExpression<QS> for InSubquery<E, Q>
where
    E: SelectableExpression<QS>,
{
}

impl<E, Q, QS> AppearsOnTable<QS> for InSubquery<E, Q>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, Q, DB> QueryFragment<DB> for InSubquery<E, Q>
where
    E: QueryFragment<DB>,
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" IN (");
        self.subquery.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// NOT IN subquery
// =============================================================================

/// expr NOT IN (SELECT ...)
#[derive(Debug, Clone, Copy)]
pub struct NotInSubquery<E, Q> {
    expr: E,
    subquery: Q,
}

impl<E, Q> NotInSubquery<E, Q> {
    /// Create a new NOT IN subquery expression.
    pub fn new(expr: E, subquery: Q) -> Self {
        Self { expr, subquery }
    }
}

impl<E: Expression, Q> Expression for NotInSubquery<E, Q> {
    type SqlType = Bool;
}

impl<E, Q, QS> SelectableExpression<QS> for NotInSubquery<E, Q>
where
    E: SelectableExpression<QS>,
{
}

impl<E, Q, QS> AppearsOnTable<QS> for NotInSubquery<E, Q>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, Q, DB> QueryFragment<DB> for NotInSubquery<E, Q>
where
    E: QueryFragment<DB>,
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" NOT IN (");
        self.subquery.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// EXISTS
// =============================================================================

/// EXISTS (SELECT ...)
#[derive(Debug, Clone, Copy)]
pub struct Exists<Q> {
    subquery: Q,
}

impl<Q> Exists<Q> {
    /// Create a new EXISTS expression.
    pub fn new(subquery: Q) -> Self {
        Self { subquery }
    }
}

/// Create an EXISTS subquery expression.
///
/// # Example
///
/// ```rust,ignore
/// users::table.filter(
///     exists(
///         orders::table
///             .filter(orders::user_id.eq(users::id))
///             .filter(orders::status.eq("pending"))
///     )
/// )
/// ```
pub fn exists<Q>(subquery: Q) -> Exists<Q> {
    Exists::new(subquery)
}

impl<Q> Expression for Exists<Q> {
    type SqlType = Bool;
}

impl<Q, QS> SelectableExpression<QS> for Exists<Q> {}
impl<Q, QS> AppearsOnTable<QS> for Exists<Q> {}

impl<Q, DB> QueryFragment<DB> for Exists<Q>
where
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("EXISTS (");
        self.subquery.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// NOT EXISTS
// =============================================================================

/// NOT EXISTS (SELECT ...)
#[derive(Debug, Clone, Copy)]
pub struct NotExists<Q> {
    subquery: Q,
}

impl<Q> NotExists<Q> {
    /// Create a new NOT EXISTS expression.
    pub fn new(subquery: Q) -> Self {
        Self { subquery }
    }
}

/// Create a NOT EXISTS subquery expression.
///
/// # Example
///
/// ```rust,ignore
/// users::table.filter(
///     not_exists(
///         orders::table
///             .filter(orders::user_id.eq(users::id))
///     )
/// )
/// ```
pub fn not_exists<Q>(subquery: Q) -> NotExists<Q> {
    NotExists::new(subquery)
}

impl<Q> Expression for NotExists<Q> {
    type SqlType = Bool;
}

impl<Q, QS> SelectableExpression<QS> for NotExists<Q> {}
impl<Q, QS> AppearsOnTable<QS> for NotExists<Q> {}

impl<Q, DB> QueryFragment<DB> for NotExists<Q>
where
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("NOT EXISTS (");
        self.subquery.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// ANY / ALL comparison subqueries
// =============================================================================

/// expr = ANY (SELECT ...)
#[derive(Debug, Clone, Copy)]
pub struct EqAny<E, Q> {
    expr: E,
    subquery: Q,
}

impl<E, Q> EqAny<E, Q> {
    /// Create a new = ANY expression.
    pub fn new(expr: E, subquery: Q) -> Self {
        Self { expr, subquery }
    }
}

impl<E: Expression, Q> Expression for EqAny<E, Q> {
    type SqlType = Bool;
}

impl<E, Q, QS> SelectableExpression<QS> for EqAny<E, Q>
where
    E: SelectableExpression<QS>,
{
}

impl<E, Q, QS> AppearsOnTable<QS> for EqAny<E, Q>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, Q, DB> QueryFragment<DB> for EqAny<E, Q>
where
    E: QueryFragment<DB>,
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" = ANY (");
        self.subquery.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

/// expr != ALL (SELECT ...)
#[derive(Debug, Clone, Copy)]
pub struct NeAll<E, Q> {
    expr: E,
    subquery: Q,
}

impl<E, Q> NeAll<E, Q> {
    /// Create a new != ALL expression.
    pub fn new(expr: E, subquery: Q) -> Self {
        Self { expr, subquery }
    }
}

impl<E: Expression, Q> Expression for NeAll<E, Q> {
    type SqlType = Bool;
}

impl<E, Q, QS> SelectableExpression<QS> for NeAll<E, Q>
where
    E: SelectableExpression<QS>,
{
}

impl<E, Q, QS> AppearsOnTable<QS> for NeAll<E, Q>
where
    E: AppearsOnTable<QS>,
{
}

impl<E, Q, DB> QueryFragment<DB> for NeAll<E, Q>
where
    E: QueryFragment<DB>,
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" != ALL (");
        self.subquery.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// Derived table (subquery in FROM)
// =============================================================================

/// A subquery used as a derived table in the FROM clause.
///
/// ClickHouse requires an alias for derived tables.
#[derive(Debug, Clone)]
pub struct DerivedTable<Q> {
    query: Q,
    alias: String,
}

impl<Q> DerivedTable<Q> {
    /// Create a new derived table with an alias.
    pub fn new(query: Q, alias: impl Into<String>) -> Self {
        Self {
            query,
            alias: alias.into(),
        }
    }

    /// Get the alias.
    pub fn alias(&self) -> &str {
        &self.alias
    }
}

impl<Q, DB> QueryFragment<DB> for DerivedTable<Q>
where
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        self.query.walk_ast(pass.reborrow())?;
        pass.push_sql(") AS ");
        pass.push_identifier(&self.alias);
        Ok(())
    }
}

// =============================================================================
// Extension traits
// =============================================================================

/// Extension trait for converting queries to subqueries.
#[allow(clippy::wrong_self_convention)] // Intentional: fluent API consumes self
pub trait AsSubquery: Sized {
    /// The SQL type of the subquery result.
    type SqlType: SqlType;

    /// Convert this query to a subquery for use in IN clauses.
    fn as_subquery(self) -> Subquery<Self, Self::SqlType> {
        Subquery::new(self)
    }

    /// Convert this query to a scalar subquery (expects single value).
    fn single_value(self) -> ScalarSubquery<Self, Self::SqlType> {
        ScalarSubquery::new(self)
    }

    /// Convert this query to a derived table for use in FROM clauses.
    fn as_derived_table(self, alias: impl Into<String>) -> DerivedTable<Self> {
        DerivedTable::new(self, alias)
    }
}

/// Extension trait for subquery comparison methods.
pub trait SubqueryExpressionMethods: Expression + Sized {
    /// Check if this expression is in the subquery result.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// users::id.in_subquery(
    ///     orders::table.select(orders::user_id)
    /// )
    /// ```
    fn in_subquery<Q>(self, subquery: Q) -> InSubquery<Self, Q> {
        InSubquery::new(self, subquery)
    }

    /// Check if this expression is not in the subquery result.
    fn not_in_subquery<Q>(self, subquery: Q) -> NotInSubquery<Self, Q> {
        NotInSubquery::new(self, subquery)
    }

    /// Check if this expression equals any value from the subquery.
    fn eq_any_subquery<Q>(self, subquery: Q) -> EqAny<Self, Q> {
        EqAny::new(self, subquery)
    }

    /// Check if this expression is not equal to all values from the subquery.
    fn ne_all_subquery<Q>(self, subquery: Q) -> NeAll<Self, Q> {
        NeAll::new(self, subquery)
    }
}

// Blanket implementation
impl<T: Expression> SubqueryExpressionMethods for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BindCollector, HttpBackend, HttpBindCollector, HttpQueryBuilder, QueryBuilder as _};
    use crate::expression::{Bound, Eq, Gt};
    use crate::query_builder::SelectStatement;
    use crate::query_source::Table;
    use diesel_clickhouse_types::UInt64;

    // Test column
    #[derive(Debug, Clone, Copy)]
    struct IdColumn;

    impl Expression for IdColumn {
        type SqlType = UInt64;
    }
    impl<T> SelectableExpression<T> for IdColumn {}
    impl<T> AppearsOnTable<T> for IdColumn {}

    impl<DB: Backend> QueryFragment<DB> for IdColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("id");
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

    // Test table: users
    #[derive(Debug, Clone, Copy)]
    struct UsersTable;

    impl Table for UsersTable {
        type PrimaryKey = IdColumn;
        type AllColumnsSqlType = UInt64;
        type AllColumns = IdColumn;

        fn table_name() -> &'static str { "users" }
        fn primary_key() -> Self::PrimaryKey { IdColumn }
        fn all_columns() -> Self::AllColumns { IdColumn }
    }

    impl crate::query_source::QuerySource for UsersTable {
        type FromClause = Self;
        type DefaultSelection = IdColumn;
        fn from_clause(&self) -> Self::FromClause { *self }
        fn default_selection(&self) -> Self::DefaultSelection { IdColumn }
    }

    impl<DB: Backend> QueryFragment<DB> for UsersTable {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("users");
            Ok(())
        }
    }

    // Test table: orders
    #[derive(Debug, Clone, Copy)]
    struct OrdersTable;

    impl Table for OrdersTable {
        type PrimaryKey = IdColumn;
        type AllColumnsSqlType = UInt64;
        type AllColumns = IdColumn;

        fn table_name() -> &'static str { "orders" }
        fn primary_key() -> Self::PrimaryKey { IdColumn }
        fn all_columns() -> Self::AllColumns { IdColumn }
    }

    impl crate::query_source::QuerySource for OrdersTable {
        type FromClause = Self;
        type DefaultSelection = IdColumn;
        fn from_clause(&self) -> Self::FromClause { *self }
        fn default_selection(&self) -> Self::DefaultSelection { IdColumn }
    }

    impl<DB: Backend> QueryFragment<DB> for OrdersTable {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("orders");
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
    fn test_in_subquery() {
        let subq = SelectStatement::new(OrdersTable)
            .select(UserIdColumn)
            .filter(Gt {
                left: AmountColumn,
                right: Bound::<_, UInt64>::new(100u64),
            });

        let in_expr = IdColumn.in_subquery(subq);
        let sql = to_sql(&in_expr);

        assert_eq!(sql, "`id` IN (SELECT `user_id` FROM `orders` WHERE `amount` > 100)");
    }

    #[test]
    fn test_not_in_subquery() {
        let subq = SelectStatement::new(OrdersTable).select(UserIdColumn);

        let not_in_expr = IdColumn.not_in_subquery(subq);
        let sql = to_sql(&not_in_expr);

        assert_eq!(sql, "`id` NOT IN (SELECT `user_id` FROM `orders`)");
    }

    #[test]
    fn test_exists() {
        let subq = SelectStatement::new(OrdersTable)
            .filter(Eq {
                left: UserIdColumn,
                right: IdColumn,
            });

        let exists_expr = exists(subq);
        let sql = to_sql(&exists_expr);

        assert_eq!(sql, "EXISTS (SELECT * FROM `orders` WHERE `user_id` = `id`)");
    }

    #[test]
    fn test_not_exists() {
        let subq = SelectStatement::new(OrdersTable)
            .filter(Eq {
                left: UserIdColumn,
                right: IdColumn,
            });

        let not_exists_expr = not_exists(subq);
        let sql = to_sql(&not_exists_expr);

        assert_eq!(sql, "NOT EXISTS (SELECT * FROM `orders` WHERE `user_id` = `id`)");
    }

    #[test]
    fn test_derived_table() {
        let subq = SelectStatement::new(OrdersTable)
            .select(UserIdColumn)
            .filter(Gt {
                left: AmountColumn,
                right: Bound::<_, UInt64>::new(100u64),
            });

        let derived = DerivedTable::new(subq, "high_value_orders");
        let sql = to_sql(&derived);

        assert_eq!(sql, "(SELECT `user_id` FROM `orders` WHERE `amount` > 100) AS `high_value_orders`");
    }

    #[test]
    fn test_scalar_subquery() {
        let subq = SelectStatement::new(OrdersTable)
            .select(AmountColumn)
            .filter(Eq {
                left: UserIdColumn,
                right: Bound::<_, UInt64>::new(1u64),
            })
            .limit(1);

        let scalar: ScalarSubquery<_, UInt64> = ScalarSubquery::new(subq);
        let sql = to_sql(&scalar);

        assert_eq!(sql, "(SELECT `amount` FROM `orders` WHERE `user_id` = 1 LIMIT 1)");
    }

    #[test]
    fn test_eq_any() {
        let subq = SelectStatement::new(OrdersTable).select(UserIdColumn);

        let expr = IdColumn.eq_any_subquery(subq);
        let sql = to_sql(&expr);

        assert_eq!(sql, "`id` = ANY (SELECT `user_id` FROM `orders`)");
    }

    #[test]
    fn test_in_subquery_in_filter() {
        let subq = SelectStatement::new(OrdersTable)
            .select(UserIdColumn)
            .filter(Gt {
                left: AmountColumn,
                right: Bound::<_, UInt64>::new(100u64),
            });

        let query = SelectStatement::new(UsersTable)
            .filter(IdColumn.in_subquery(subq));

        let sql = to_sql(&query);
        assert_eq!(sql, "SELECT * FROM `users` WHERE `id` IN (SELECT `user_id` FROM `orders` WHERE `amount` > 100)");
    }

    #[test]
    fn test_exists_in_filter() {
        let subq = SelectStatement::new(OrdersTable)
            .filter(Eq {
                left: UserIdColumn,
                right: IdColumn,
            });

        let query = SelectStatement::new(UsersTable)
            .filter(exists(subq));

        let sql = to_sql(&query);
        assert_eq!(sql, "SELECT * FROM `users` WHERE EXISTS (SELECT * FROM `orders` WHERE `user_id` = `id`)");
    }
}
