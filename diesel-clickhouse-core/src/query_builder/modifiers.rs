//! Query modifiers: LIMIT BY and aliasing.
//!
//! This module provides additional query modifiers specific to ClickHouse.
//!
//! # Examples
//!
//! ```rust,ignore
//! use diesel_clickhouse::query_builder::modifiers::*;
//!
//! // DISTINCT (use SelectStatement methods directly)
//! users::table.select(users::name).distinct()
//!
//! // DISTINCT ON (ClickHouse-specific)
//! users::table
//!     .select((users::name, users::age))
//!     .distinct_on(users::department)
//!
//! // LIMIT BY (ClickHouse-specific)
//! events::table
//!     .order_by(events::date.desc())
//!     .limit_by(3, events::user_id)  // 3 rows per user_id
//!
//! // Aliasing
//! users::table.select(count_star().alias("total"))
//! ```

use compact_str::CompactString;

use crate::backend::Backend;
use crate::expression::Expression;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

// =============================================================================
// LIMIT BY (ClickHouse-specific)
// =============================================================================

/// A query with LIMIT BY modifier.
///
/// ClickHouse syntax: `... LIMIT n BY expr`
/// Returns at most n rows for each distinct value of expr.
#[derive(Debug, Clone, Copy)]
pub struct LimitBy<Q, E> {
    query: Q,
    limit: i64,
    by_expr: E,
}

impl<Q, E> LimitBy<Q, E> {
    /// Create a new LIMIT BY query.
    pub fn new(query: Q, limit: i64, by_expr: E) -> Self {
        Self { query, limit, by_expr }
    }
}

impl<Q, E, DB> QueryFragment<DB> for LimitBy<Q, E>
where
    Q: QueryFragment<DB>,
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.query.walk_ast(pass.reborrow())?;
        pass.push_sql(" LIMIT ");
        pass.push_bindable(&self.limit)?;
        pass.push_sql(" BY ");
        self.by_expr.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// LIMIT n OFFSET m BY expr
#[derive(Debug, Clone, Copy)]
pub struct LimitOffsetBy<Q, E> {
    query: Q,
    limit: i64,
    offset: i64,
    by_expr: E,
}

impl<Q, E> LimitOffsetBy<Q, E> {
    /// Create a new LIMIT OFFSET BY query.
    pub fn new(query: Q, limit: i64, offset: i64, by_expr: E) -> Self {
        Self { query, limit, offset, by_expr }
    }
}

impl<Q, E, DB> QueryFragment<DB> for LimitOffsetBy<Q, E>
where
    Q: QueryFragment<DB>,
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.query.walk_ast(pass.reborrow())?;
        pass.push_sql(" LIMIT ");
        pass.push_bindable(&self.limit)?;
        pass.push_sql(" OFFSET ");
        pass.push_bindable(&self.offset)?;
        pass.push_sql(" BY ");
        self.by_expr.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// =============================================================================
// Aliased Expression
// =============================================================================

/// An expression with an alias (AS clause).
///
/// # Allocation Optimization
///
/// Uses `CompactString` for the alias name, which stores strings up to 24 bytes
/// inline (on the stack) without heap allocation. Since most column aliases are
/// short (e.g., "id", "name", "total_count"), this avoids heap allocation in
/// the vast majority of cases.
#[derive(Debug, Clone)]
pub struct Alias<E> {
    expr: E,
    alias: CompactString,
}

impl<E> Alias<E> {
    /// Create a new aliased expression.
    ///
    /// Accepts any type that can be converted to `CompactString`, including
    /// `&str`, `String`, and `CompactString` itself.
    pub fn new(expr: E, alias: impl Into<CompactString>) -> Self {
        Self {
            expr,
            alias: alias.into(),
        }
    }

    /// Get the alias name.
    #[inline]
    pub fn alias_name(&self) -> &str {
        &self.alias
    }
}

impl<E: Expression> Expression for Alias<E> {
    type SqlType = E::SqlType;
}

impl<E, QS> crate::expression::SelectableExpression<QS> for Alias<E>
where
    E: crate::expression::SelectableExpression<QS>,
{
}

impl<E, QS> crate::expression::AppearsOnTable<QS> for Alias<E>
where
    E: crate::expression::AppearsOnTable<QS>,
{
}

impl<E, DB> QueryFragment<DB> for Alias<E>
where
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.expr.walk_ast(pass.reborrow())?;
        pass.push_sql(" AS ");
        pass.push_identifier(&self.alias);
        Ok(())
    }
}

// =============================================================================
// Extension traits
// =============================================================================

/// Extension trait for LIMIT BY queries.
pub trait LimitByDsl: Sized {
    /// Apply LIMIT BY to the query (ClickHouse-specific).
    ///
    /// Returns at most `limit` rows for each distinct value of `by_expr`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get last 3 events per user
    /// events::table
    ///     .order_by(events::date.desc())
    ///     .limit_by(3, events::user_id)
    /// ```
    fn limit_by<E: Expression>(self, limit: i64, by_expr: E) -> LimitBy<Self, E> {
        LimitBy::new(self, limit, by_expr)
    }

    /// Apply LIMIT OFFSET BY to the query.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Skip first 2, then get 3 events per user
    /// events::table
    ///     .order_by(events::date.desc())
    ///     .limit_offset_by(3, 2, events::user_id)
    /// ```
    fn limit_offset_by<E: Expression>(self, limit: i64, offset: i64, by_expr: E) -> LimitOffsetBy<Self, E> {
        LimitOffsetBy::new(self, limit, offset, by_expr)
    }
}

impl<T> LimitByDsl for T {}

/// Extension trait for aliasing expressions.
pub trait AliasDsl: Sized {
    /// Add an alias to this expression.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// count_star().alias("total_count")
    /// // Generates: count(*) AS `total_count`
    /// ```
    fn alias(self, name: impl Into<CompactString>) -> Alias<Self> {
        Alias::new(self, name)
    }

    /// Alias shorthand using `as_`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// users::name.as_("user_name")
    /// ```
    fn as_(self, name: impl Into<CompactString>) -> Alias<Self> {
        Alias::new(self, name)
    }
}

impl<T> AliasDsl for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HttpBackend, HttpBindCollector, HttpQueryBuilder, QueryBuilder as _};
    use crate::expression::{Expression, SelectableExpression, AppearsOnTable};
    use crate::query_source::Table;
    use crate::query_builder::SelectStatement;
    use diesel_clickhouse_types::UInt64;

    // Test columns
    #[derive(Debug, Clone, Copy)]
    struct NameColumn;

    impl Expression for NameColumn {
        type SqlType = diesel_clickhouse_types::CHString;
    }
    impl<T> SelectableExpression<T> for NameColumn {}
    impl<T> AppearsOnTable<T> for NameColumn {}

    impl<DB: Backend> QueryFragment<DB> for NameColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("name");
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
    struct DepartmentColumn;

    impl Expression for DepartmentColumn {
        type SqlType = diesel_clickhouse_types::CHString;
    }
    impl<T> SelectableExpression<T> for DepartmentColumn {}
    impl<T> AppearsOnTable<T> for DepartmentColumn {}

    impl<DB: Backend> QueryFragment<DB> for DepartmentColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("department");
            Ok(())
        }
    }

    // Test table
    #[derive(Debug, Clone, Copy)]
    struct UsersTable;

    impl Table for UsersTable {
        type PrimaryKey = UserIdColumn;
        type AllColumnsSqlType = UInt64;
        type AllColumns = UserIdColumn;

        fn table_name() -> &'static str { "users" }
        fn primary_key() -> Self::PrimaryKey { UserIdColumn }
        fn all_columns() -> Self::AllColumns { UserIdColumn }
    }

    impl crate::query_source::QuerySource for UsersTable {
        type FromClause = Self;
        type DefaultSelection = UserIdColumn;
        fn from_clause(&self) -> Self::FromClause { *self }
        fn default_selection(&self) -> Self::DefaultSelection { UserIdColumn }
    }

    impl<DB: Backend> QueryFragment<DB> for UsersTable {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("users");
            Ok(())
        }
    }

    fn to_sql<T: QueryFragment<HttpBackend>>(fragment: &T) -> String {
        use crate::backend::BindCollector;
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
    fn test_distinct() {
        // Uses SelectStatement's native .distinct() method
        let query = SelectStatement::new(UsersTable).select(NameColumn).distinct();
        let sql = to_sql(&query);
        assert_eq!(sql, "SELECT DISTINCT `name` FROM `users`");
    }

    #[test]
    fn test_distinct_on() {
        // Uses SelectStatement's native .distinct_on() method
        let query = SelectStatement::new(UsersTable)
            .select(NameColumn)
            .distinct_on(DepartmentColumn);
        let sql = to_sql(&query);
        assert_eq!(sql, "SELECT DISTINCT ON (`department`) `name` FROM `users`");
    }

    #[test]
    fn test_limit_by() {
        let query = SelectStatement::new(UsersTable)
            .select(NameColumn)
            .limit_by(3, UserIdColumn);
        let sql = to_sql(&query);
        assert_eq!(sql, "SELECT `name` FROM `users` LIMIT 3 BY `user_id`");
    }

    #[test]
    fn test_limit_offset_by() {
        let query = SelectStatement::new(UsersTable)
            .select(NameColumn)
            .limit_offset_by(3, 2, UserIdColumn);
        let sql = to_sql(&query);
        assert_eq!(sql, "SELECT `name` FROM `users` LIMIT 3 OFFSET 2 BY `user_id`");
    }

    #[test]
    fn test_alias() {
        let expr = NameColumn.alias("user_name");
        let sql = to_sql(&expr);
        assert_eq!(sql, "`name` AS `user_name`");
    }

    #[test]
    fn test_as_shorthand() {
        let expr = NameColumn.as_("n");
        let sql = to_sql(&expr);
        assert_eq!(sql, "`name` AS `n`");
    }

    #[test]
    fn test_alias_with_function() {
        use crate::expression::functions::count_star;
        let expr = count_star().alias("total");
        let sql = to_sql(&expr);
        assert_eq!(sql, "count(*) AS `total`");
    }
}
