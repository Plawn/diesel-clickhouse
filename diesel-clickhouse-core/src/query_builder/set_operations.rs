//! Set operations: UNION, INTERSECT, EXCEPT.
//!
//! This module provides SQL set operations for combining query results.
//!
//! # Examples
//!
//! ```rust,ignore
//! use diesel_clickhouse::query_builder::set_operations::*;
//!
//! // UNION (removes duplicates)
//! users::table.select(users::name)
//!     .union(admins::table.select(admins::name))
//!
//! // UNION ALL (keeps duplicates)
//! users::table.select(users::id)
//!     .union_all(archived_users::table.select(archived_users::id))
//!
//! // INTERSECT
//! active::table.select(active::id)
//!     .intersect(premium::table.select(premium::id))
//!
//! // EXCEPT
//! all_users::table.select(all_users::id)
//!     .except(banned::table.select(banned::id))
//! ```

use crate::backend::Backend;
use crate::result::QueryResult;

use super::{AstPass, QueryFragment};

// =============================================================================
// UNION
// =============================================================================

/// A UNION of two queries.
///
/// By default, UNION removes duplicate rows. Use `UnionAll` to keep duplicates.
#[derive(Debug, Clone, Copy)]
pub struct Union<L, R> {
    left: L,
    right: R,
}

impl<L, R> Union<L, R> {
    /// Create a new UNION.
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<L, R, DB> QueryFragment<DB> for Union<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(") UNION (");
        self.right.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// UNION ALL
// =============================================================================

/// A UNION ALL of two queries (keeps duplicates).
#[derive(Debug, Clone, Copy)]
pub struct UnionAll<L, R> {
    left: L,
    right: R,
}

impl<L, R> UnionAll<L, R> {
    /// Create a new UNION ALL.
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<L, R, DB> QueryFragment<DB> for UnionAll<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(") UNION ALL (");
        self.right.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// INTERSECT
// =============================================================================

/// An INTERSECT of two queries.
///
/// Returns rows that appear in both queries.
#[derive(Debug, Clone, Copy)]
pub struct Intersect<L, R> {
    left: L,
    right: R,
}

impl<L, R> Intersect<L, R> {
    /// Create a new INTERSECT.
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<L, R, DB> QueryFragment<DB> for Intersect<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(") INTERSECT (");
        self.right.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// EXCEPT
// =============================================================================

/// An EXCEPT of two queries.
///
/// Returns rows from the first query that don't appear in the second.
#[derive(Debug, Clone, Copy)]
pub struct Except<L, R> {
    left: L,
    right: R,
}

impl<L, R> Except<L, R> {
    /// Create a new EXCEPT.
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<L, R, DB> QueryFragment<DB> for Except<L, R>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(") EXCEPT (");
        self.right.walk_ast(pass.reborrow())?;
        pass.push_sql(")");
        Ok(())
    }
}

// =============================================================================
// Extension trait for set operations
// =============================================================================

/// Extension trait for adding set operations to queries.
pub trait SetOperationsDsl: Sized {
    /// Combine with another query using UNION (removes duplicates).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// users::table.select(users::name)
    ///     .union(admins::table.select(admins::name))
    /// ```
    fn union<R>(self, other: R) -> Union<Self, R> {
        Union::new(self, other)
    }

    /// Combine with another query using UNION ALL (keeps duplicates).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// users::table.select(users::id)
    ///     .union_all(archived_users::table.select(archived_users::id))
    /// ```
    fn union_all<R>(self, other: R) -> UnionAll<Self, R> {
        UnionAll::new(self, other)
    }

    /// Combine with another query using INTERSECT.
    ///
    /// Returns rows that appear in both queries.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// active::table.select(active::id)
    ///     .intersect(premium::table.select(premium::id))
    /// ```
    fn intersect<R>(self, other: R) -> Intersect<Self, R> {
        Intersect::new(self, other)
    }

    /// Combine with another query using EXCEPT.
    ///
    /// Returns rows from this query that don't appear in the other.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// all_users::table.select(all_users::id)
    ///     .except(banned::table.select(banned::id))
    /// ```
    fn except<R>(self, other: R) -> Except<Self, R> {
        Except::new(self, other)
    }
}

// Blanket implementation for all types that implement QueryFragment
impl<T> SetOperationsDsl for T {}

// =============================================================================
// Chaining support - Allow set operations on set operations
// =============================================================================

impl<L, R> Union<L, R> {
    /// Chain another UNION.
    pub fn union<R2>(self, other: R2) -> Union<Self, R2> {
        Union::new(self, other)
    }

    /// Chain a UNION ALL.
    pub fn union_all<R2>(self, other: R2) -> UnionAll<Self, R2> {
        UnionAll::new(self, other)
    }
}

impl<L, R> UnionAll<L, R> {
    /// Chain another UNION.
    pub fn union<R2>(self, other: R2) -> Union<Self, R2> {
        Union::new(self, other)
    }

    /// Chain a UNION ALL.
    pub fn union_all<R2>(self, other: R2) -> UnionAll<Self, R2> {
        UnionAll::new(self, other)
    }
}

impl<L, R> Intersect<L, R> {
    /// Chain another INTERSECT.
    pub fn intersect<R2>(self, other: R2) -> Intersect<Self, R2> {
        Intersect::new(self, other)
    }
}

impl<L, R> Except<L, R> {
    /// Chain another EXCEPT.
    pub fn except<R2>(self, other: R2) -> Except<Self, R2> {
        Except::new(self, other)
    }
}

// =============================================================================
// ORDER BY and LIMIT on set operations
// =============================================================================

/// A set operation with an ORDER BY clause.
#[derive(Debug, Clone, Copy)]
pub struct OrderedSetOperation<S, O> {
    operation: S,
    order_by: O,
}

impl<S, O, DB> QueryFragment<DB> for OrderedSetOperation<S, O>
where
    S: QueryFragment<DB>,
    O: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.operation.walk_ast(pass.reborrow())?;
        pass.push_sql(" ORDER BY ");
        self.order_by.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// A set operation with a LIMIT clause.
#[derive(Debug, Clone, Copy)]
pub struct LimitedSetOperation<S> {
    operation: S,
    limit: i64,
}

impl<S, DB> QueryFragment<DB> for LimitedSetOperation<S>
where
    S: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.operation.walk_ast(pass.reborrow())?;
        pass.push_sql(&format!(" LIMIT {}", self.limit));
        Ok(())
    }
}

/// Extension trait for ORDER BY and LIMIT on set operations.
pub trait SetOperationModifiers: Sized {
    /// Add an ORDER BY clause to a set operation.
    fn order_by<O>(self, order: O) -> OrderedSetOperation<Self, O> {
        OrderedSetOperation {
            operation: self,
            order_by: order,
        }
    }

    /// Add a LIMIT clause to a set operation.
    fn limit(self, limit: i64) -> LimitedSetOperation<Self> {
        LimitedSetOperation {
            operation: self,
            limit,
        }
    }
}

impl<L, R> SetOperationModifiers for Union<L, R> {}
impl<L, R> SetOperationModifiers for UnionAll<L, R> {}
impl<L, R> SetOperationModifiers for Intersect<L, R> {}
impl<L, R> SetOperationModifiers for Except<L, R> {}
impl<S, O> SetOperationModifiers for OrderedSetOperation<S, O> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HttpBackend, HttpBindCollector, HttpQueryBuilder, QueryBuilder as _};
    use crate::expression::{Expression, SelectableExpression, AppearsOnTable};
    use crate::query_source::Table;
    use crate::query_builder::SelectStatement;
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

    // Test table: admins
    #[derive(Debug, Clone, Copy)]
    struct AdminsTable;

    impl Table for AdminsTable {
        type PrimaryKey = IdColumn;
        type AllColumnsSqlType = UInt64;
        type AllColumns = IdColumn;

        fn table_name() -> &'static str { "admins" }
        fn primary_key() -> Self::PrimaryKey { IdColumn }
        fn all_columns() -> Self::AllColumns { IdColumn }
    }

    impl crate::query_source::QuerySource for AdminsTable {
        type FromClause = Self;
        type DefaultSelection = IdColumn;
        fn from_clause(&self) -> Self::FromClause { *self }
        fn default_selection(&self) -> Self::DefaultSelection { IdColumn }
    }

    impl<DB: Backend> QueryFragment<DB> for AdminsTable {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("admins");
            Ok(())
        }
    }

    fn to_sql<T: QueryFragment<HttpBackend>>(fragment: &T) -> String {
        let mut builder = HttpQueryBuilder::default();
        let mut collector = HttpBindCollector::default();
        let pass = AstPass::<HttpBackend>::new(&mut builder, &mut collector);
        fragment.walk_ast(pass).ok();
        builder.finish()
    }

    #[test]
    fn test_union() {
        let q1 = SelectStatement::new(UsersTable).select(IdColumn);
        let q2 = SelectStatement::new(AdminsTable).select(IdColumn);

        let union = q1.union(q2);
        let sql = to_sql(&union);

        assert_eq!(sql, "(SELECT `id` FROM `users`) UNION (SELECT `id` FROM `admins`)");
    }

    #[test]
    fn test_union_all() {
        let q1 = SelectStatement::new(UsersTable).select(IdColumn);
        let q2 = SelectStatement::new(AdminsTable).select(IdColumn);

        let union = q1.union_all(q2);
        let sql = to_sql(&union);

        assert_eq!(sql, "(SELECT `id` FROM `users`) UNION ALL (SELECT `id` FROM `admins`)");
    }

    #[test]
    fn test_intersect() {
        let q1 = SelectStatement::new(UsersTable).select(IdColumn);
        let q2 = SelectStatement::new(AdminsTable).select(IdColumn);

        let intersect = q1.intersect(q2);
        let sql = to_sql(&intersect);

        assert_eq!(sql, "(SELECT `id` FROM `users`) INTERSECT (SELECT `id` FROM `admins`)");
    }

    #[test]
    fn test_except() {
        let q1 = SelectStatement::new(UsersTable).select(IdColumn);
        let q2 = SelectStatement::new(AdminsTable).select(IdColumn);

        let except = q1.except(q2);
        let sql = to_sql(&except);

        assert_eq!(sql, "(SELECT `id` FROM `users`) EXCEPT (SELECT `id` FROM `admins`)");
    }

    #[test]
    fn test_union_with_limit() {
        let q1 = SelectStatement::new(UsersTable).select(IdColumn);
        let q2 = SelectStatement::new(AdminsTable).select(IdColumn);

        let union = q1.union(q2).limit(10);
        let sql = to_sql(&union);

        assert_eq!(sql, "(SELECT `id` FROM `users`) UNION (SELECT `id` FROM `admins`) LIMIT 10");
    }

    #[test]
    fn test_chained_union() {
        let q1 = SelectStatement::new(UsersTable).select(IdColumn);
        let q2 = SelectStatement::new(AdminsTable).select(IdColumn);
        let q3 = SelectStatement::new(UsersTable).select(IdColumn);

        let union = q1.union(q2).union(q3);
        let sql = to_sql(&union);

        assert_eq!(
            sql,
            "((SELECT `id` FROM `users`) UNION (SELECT `id` FROM `admins`)) UNION (SELECT `id` FROM `users`)"
        );
    }
}
