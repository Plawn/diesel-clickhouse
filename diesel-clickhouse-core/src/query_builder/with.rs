//! Common Table Expressions (CTEs) support - WITH clause.
//!
//! This module provides support for SQL WITH clauses (CTEs).
//!
//! # Examples
//!
//! ```rust,ignore
//! use diesel_clickhouse::query_builder::with::*;
//!
//! // Single CTE
//! with_query("active_users", users::table.filter(users::active.eq(true)))
//!     .select(star())
//!     .from_cte("active_users")
//!
//! // Multiple CTEs
//! with_queries()
//!     .cte("active_users", users::table.filter(users::active.eq(true)))
//!     .cte("recent_orders", orders::table.filter(orders::date.gt(yesterday())))
//!     .query(
//!         cte_ref("active_users")
//!             .inner_join_on(cte_ref("recent_orders"), ...)
//!     )
//! ```

// Complex generic types are intentional for type-safe CTE building
#![allow(clippy::type_complexity)]

use crate::backend::Backend;
use crate::result::QueryResult;

use super::{AstPass, QueryFragment};

// =============================================================================
// Single CTE
// =============================================================================

/// A Common Table Expression (CTE).
#[derive(Debug, Clone)]
pub struct Cte<Q> {
    name: String,
    query: Q,
}

impl<Q> Cte<Q> {
    /// Create a new CTE.
    pub fn new(name: impl Into<String>, query: Q) -> Self {
        Self {
            name: name.into(),
            query,
        }
    }

    /// Get the CTE name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl<Q, DB> QueryFragment<DB> for Cte<Q>
where
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_identifier(&self.name);
        pass.push_sql(" AS (");
        self.query.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// WITH clause with single CTE
// =============================================================================

/// A WITH clause containing CTEs followed by a main query.
#[derive(Debug, Clone)]
pub struct WithClause<Ctes, Query> {
    ctes: Ctes,
    query: Query,
}

impl<Ctes, Query> WithClause<Ctes, Query> {
    /// Create a new WITH clause.
    pub fn new(ctes: Ctes, query: Query) -> Self {
        Self { ctes, query }
    }
}

impl<Ctes, Query, DB> QueryFragment<DB> for WithClause<Ctes, Query>
where
    Ctes: CteList<DB>,
    Query: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("WITH ");
        self.ctes.walk_ctes(pass.reborrow())?;
        pass.push_sql(" ");
        self.query.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// =============================================================================
// CTE list trait
// =============================================================================

/// Trait for a list of CTEs.
pub trait CteList<DB: Backend> {
    /// Walk the CTEs, separated by commas.
    fn walk_ctes<'b>(&'b self, pass: AstPass<'_, 'b, DB>) -> QueryResult<()>;
}

// Single CTE
impl<Q, DB> CteList<DB> for Cte<Q>
where
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ctes<'b>(&'b self, pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.walk_ast(pass)
    }
}

// Tuple of 2 CTEs
impl<Q1, Q2, DB> CteList<DB> for (Cte<Q1>, Cte<Q2>)
where
    Q1: QueryFragment<DB>,
    Q2: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ctes<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.0.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.1.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// Tuple of 3 CTEs
impl<Q1, Q2, Q3, DB> CteList<DB> for (Cte<Q1>, Cte<Q2>, Cte<Q3>)
where
    Q1: QueryFragment<DB>,
    Q2: QueryFragment<DB>,
    Q3: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ctes<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.0.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.1.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.2.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// Tuple of 4 CTEs
impl<Q1, Q2, Q3, Q4, DB> CteList<DB> for (Cte<Q1>, Cte<Q2>, Cte<Q3>, Cte<Q4>)
where
    Q1: QueryFragment<DB>,
    Q2: QueryFragment<DB>,
    Q3: QueryFragment<DB>,
    Q4: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ctes<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.0.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.1.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.2.walk_ast(pass.reborrow())?;
        pass.push_sql(", ");
        self.3.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// Vec of CTEs
impl<Q, DB> CteList<DB> for Vec<Cte<Q>>
where
    Q: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ctes<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        let mut first = true;
        for cte in self {
            if !first {
                pass.push_sql(", ");
            }
            first = false;
            cte.walk_ast(pass.reborrow())?;
        }
        Ok(())
    }
}

// =============================================================================
// CTE reference (for use in main query)
// =============================================================================

/// A reference to a CTE by name.
///
/// Used in the FROM clause of the main query.
#[derive(Debug, Clone)]
pub struct CteRef {
    name: String,
}

impl CteRef {
    /// Create a new CTE reference.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Get the CTE name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Create a reference to a CTE.
///
/// Use this in the FROM clause of the main query.
pub fn cte_ref(name: impl Into<String>) -> CteRef {
    CteRef::new(name)
}

impl<DB: Backend> QueryFragment<DB> for CteRef {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_identifier(&self.name);
        Ok(())
    }
}

// =============================================================================
// Builder pattern
// =============================================================================

/// Builder for a single-CTE WITH clause.
#[derive(Debug, Clone)]
pub struct WithQueryBuilder<Q> {
    cte: Cte<Q>,
}

impl<Q> WithQueryBuilder<Q> {
    /// Create a new builder with a CTE.
    pub fn new(name: impl Into<String>, query: Q) -> Self {
        Self {
            cte: Cte::new(name, query),
        }
    }

    /// Set the main query and build the WITH clause.
    pub fn query<MainQuery>(self, query: MainQuery) -> WithClause<Cte<Q>, MainQuery> {
        WithClause::new(self.cte, query)
    }
}

/// Start building a WITH clause with a single CTE.
///
/// # Example
///
/// ```rust,ignore
/// with_query("active_users", users::table.filter(users::active.eq(true)))
///     .query(SelectStatement::new(cte_ref("active_users")))
/// ```
pub fn with_query<Q>(name: impl Into<String>, query: Q) -> WithQueryBuilder<Q> {
    WithQueryBuilder::new(name, query)
}

// =============================================================================
// Multi-CTE builder
// =============================================================================

/// Builder for multiple CTEs.
#[derive(Debug, Clone)]
pub struct WithQueriesBuilder<Ctes> {
    ctes: Ctes,
}

impl WithQueriesBuilder<()> {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self { ctes: () }
    }

    /// Add the first CTE.
    pub fn cte<Q>(self, name: impl Into<String>, query: Q) -> WithQueriesBuilder<Cte<Q>> {
        WithQueriesBuilder {
            ctes: Cte::new(name, query),
        }
    }
}

impl Default for WithQueriesBuilder<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Q1> WithQueriesBuilder<Cte<Q1>> {
    /// Add a second CTE.
    pub fn cte<Q2>(self, name: impl Into<String>, query: Q2) -> WithQueriesBuilder<(Cte<Q1>, Cte<Q2>)> {
        WithQueriesBuilder {
            ctes: (self.ctes, Cte::new(name, query)),
        }
    }

    /// Set the main query and build.
    pub fn query<MainQuery>(self, query: MainQuery) -> WithClause<Cte<Q1>, MainQuery> {
        WithClause::new(self.ctes, query)
    }
}

impl<Q1, Q2> WithQueriesBuilder<(Cte<Q1>, Cte<Q2>)> {
    /// Add a third CTE.
    pub fn cte<Q3>(self, name: impl Into<String>, query: Q3) -> WithQueriesBuilder<(Cte<Q1>, Cte<Q2>, Cte<Q3>)> {
        WithQueriesBuilder {
            ctes: (self.ctes.0, self.ctes.1, Cte::new(name, query)),
        }
    }

    /// Set the main query and build.
    pub fn query<MainQuery>(self, query: MainQuery) -> WithClause<(Cte<Q1>, Cte<Q2>), MainQuery> {
        WithClause::new(self.ctes, query)
    }
}

impl<Q1, Q2, Q3> WithQueriesBuilder<(Cte<Q1>, Cte<Q2>, Cte<Q3>)> {
    /// Add a fourth CTE.
    pub fn cte<Q4>(self, name: impl Into<String>, query: Q4) -> WithQueriesBuilder<(Cte<Q1>, Cte<Q2>, Cte<Q3>, Cte<Q4>)> {
        WithQueriesBuilder {
            ctes: (self.ctes.0, self.ctes.1, self.ctes.2, Cte::new(name, query)),
        }
    }

    /// Set the main query and build.
    pub fn query<MainQuery>(self, query: MainQuery) -> WithClause<(Cte<Q1>, Cte<Q2>, Cte<Q3>), MainQuery> {
        WithClause::new(self.ctes, query)
    }
}

impl<Q1, Q2, Q3, Q4> WithQueriesBuilder<(Cte<Q1>, Cte<Q2>, Cte<Q3>, Cte<Q4>)> {
    /// Set the main query and build.
    pub fn query<MainQuery>(self, query: MainQuery) -> WithClause<(Cte<Q1>, Cte<Q2>, Cte<Q3>, Cte<Q4>), MainQuery> {
        WithClause::new(self.ctes, query)
    }
}

/// Start building a WITH clause with multiple CTEs.
///
/// # Example
///
/// ```rust,ignore
/// with_queries()
///     .cte("active_users", users::table.filter(users::active.eq(true)))
///     .cte("recent_orders", orders::table.filter(orders::date.gt(yesterday())))
///     .query(
///         SelectStatement::new(cte_ref("active_users"))
///             .inner_join_on(cte_ref("recent_orders"), ...)
///     )
/// ```
pub fn with_queries() -> WithQueriesBuilder<()> {
    WithQueriesBuilder::new()
}

// =============================================================================
// Dynamic CTE builder (using Vec)
// =============================================================================

/// Builder for dynamically constructed CTEs.
#[derive(Debug, Clone)]
pub struct DynamicWithBuilder<Q> {
    ctes: Vec<Cte<Q>>,
}

impl<Q> DynamicWithBuilder<Q> {
    /// Create a new dynamic builder.
    pub fn new() -> Self {
        Self { ctes: Vec::with_capacity(4) }
    }

    /// Add a CTE.
    pub fn cte(mut self, name: impl Into<String>, query: Q) -> Self {
        self.ctes.push(Cte::new(name, query));
        self
    }

    /// Set the main query and build.
    pub fn query<MainQuery>(self, query: MainQuery) -> WithClause<Vec<Cte<Q>>, MainQuery> {
        WithClause::new(self.ctes, query)
    }
}

impl<Q> Default for DynamicWithBuilder<Q> {
    fn default() -> Self {
        Self::new()
    }
}

/// Start building a WITH clause with a dynamic number of CTEs.
///
/// Use this when the number of CTEs is not known at compile time.
pub fn dynamic_with<Q>() -> DynamicWithBuilder<Q> {
    DynamicWithBuilder::new()
}

// =============================================================================
// Extension trait for queries
// =============================================================================

/// Extension trait for wrapping a query in a WITH clause.
#[allow(clippy::wrong_self_convention)] // Intentional: fluent API consumes self
pub trait WithDsl: Sized {
    /// Wrap this query in a WITH clause as a CTE.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let main_query = users::table
    ///     .filter(users::active.eq(true))
    ///     .as_cte("active_users")
    ///     .query(SelectStatement::new(cte_ref("active_users")));
    /// ```
    fn as_cte(self, name: impl Into<String>) -> WithQueryBuilder<Self> {
        WithQueryBuilder::new(name, self)
    }
}

// Blanket implementation
impl<T> WithDsl for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BindCollector, HttpBackend, HttpBindCollector, HttpQueryBuilder, QueryBuilder as _};
    use crate::expression::{Bound, Expression, SelectableExpression, AppearsOnTable, Eq, Gt};
    use crate::query_source::Table;
    use crate::query_builder::SelectStatement;
    use diesel_clickhouse_types::{Bool, UInt64};

    // Test columns
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
    struct ActiveColumn;

    impl Expression for ActiveColumn {
        type SqlType = Bool;
    }
    impl<T> SelectableExpression<T> for ActiveColumn {}
    impl<T> AppearsOnTable<T> for ActiveColumn {}

    impl<DB: Backend> QueryFragment<DB> for ActiveColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("active");
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
    fn test_single_cte() {
        let cte_query = SelectStatement::new(UsersTable)
            .select(IdColumn)
            .filter(Eq {
                left: ActiveColumn,
                right: Bound::<_, Bool>::new(true),
            });

        let main_query = SelectStatement::new(cte_ref("active_users"));

        let with_clause = with_query("active_users", cte_query)
            .query(main_query);

        let sql = to_sql(&with_clause);
        assert_eq!(
            sql,
            "WITH `active_users` AS (SELECT `id` FROM `users` WHERE `active` = true) SELECT * FROM `active_users`"
        );
    }

    #[test]
    fn test_two_ctes() {
        let users_cte = SelectStatement::new(UsersTable)
            .filter(Eq {
                left: ActiveColumn,
                right: Bound::<_, Bool>::new(true),
            });

        let orders_cte = SelectStatement::new(OrdersTable)
            .filter(Gt {
                left: AmountColumn,
                right: Bound::<_, UInt64>::new(100u64),
            });

        let main_query = SelectStatement::new(cte_ref("active_users"));

        let with_clause = with_queries()
            .cte("active_users", users_cte)
            .cte("high_value_orders", orders_cte)
            .query(main_query);

        let sql = to_sql(&with_clause);
        assert_eq!(
            sql,
            "WITH `active_users` AS (SELECT * FROM `users` WHERE `active` = true), `high_value_orders` AS (SELECT * FROM `orders` WHERE `amount` > 100) SELECT * FROM `active_users`"
        );
    }

    #[test]
    fn test_cte_ref() {
        let cte = cte_ref("my_cte");
        let sql = to_sql(&cte);
        assert_eq!(sql, "`my_cte`");
    }

    #[test]
    fn test_as_cte_extension() {
        let query = SelectStatement::new(UsersTable)
            .filter(Eq {
                left: ActiveColumn,
                right: Bound::<_, Bool>::new(true),
            })
            .as_cte("active_users")
            .query(SelectStatement::new(cte_ref("active_users")));

        let sql = to_sql(&query);
        assert_eq!(
            sql,
            "WITH `active_users` AS (SELECT * FROM `users` WHERE `active` = true) SELECT * FROM `active_users`"
        );
    }

    #[test]
    fn test_dynamic_with() {
        // Note: dynamic_with requires all CTEs to have the exact same query type.
        // For mixed query types, use with_queries() builder instead.
        let users_cte1 = SelectStatement::new(UsersTable);
        let users_cte2 = SelectStatement::new(UsersTable);

        let with_clause = dynamic_with()
            .cte("users_cte1", users_cte1)
            .cte("users_cte2", users_cte2)
            .query(SelectStatement::new(cte_ref("users_cte1")));

        let sql = to_sql(&with_clause);
        assert_eq!(
            sql,
            "WITH `users_cte1` AS (SELECT * FROM `users`), `users_cte2` AS (SELECT * FROM `users`) SELECT * FROM `users_cte1`"
        );
    }
}
