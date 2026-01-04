//! SELECT statement builder.

// Complex generic types are intentional for type-safe query building
#![allow(clippy::type_complexity)]

use crate::backend::Backend;
use crate::expression::Expression;
use crate::query_source::{Join, Inner, Left, Right, JoinTo, ArrayJoin, Table, JoinType};
use crate::result::QueryResult;
use super::{QueryFragment, AstPass, QueryOutputType};

// =============================================================================
// DISTINCT type markers
// =============================================================================

/// Marker type for no DISTINCT modifier.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoDistinct;

/// Marker type for DISTINCT modifier.
#[derive(Debug, Clone, Copy, Default)]
pub struct WithDistinct;

/// Marker type for DISTINCT ON modifier (ClickHouse-specific).
#[derive(Debug, Clone, Copy)]
pub struct WithDistinctOn<E> {
    on_expr: E,
}

impl<E> WithDistinctOn<E> {
    /// Create a new DISTINCT ON marker with the given expression.
    pub fn new(on_expr: E) -> Self {
        Self { on_expr }
    }
}

/// Trait for DISTINCT clause types.
pub trait DistinctClause {
    /// Whether this is a DISTINCT clause.
    fn is_distinct(&self) -> bool;
}

impl DistinctClause for NoDistinct {
    #[inline]
    fn is_distinct(&self) -> bool {
        false
    }
}

impl DistinctClause for WithDistinct {
    #[inline]
    fn is_distinct(&self) -> bool {
        true
    }
}

impl<E> DistinctClause for WithDistinctOn<E> {
    #[inline]
    fn is_distinct(&self) -> bool {
        true
    }
}

/// A SELECT statement builder.
///
/// # Type Parameters
///
/// - `From`: The source table or join
/// - `Select`: Selected columns (or `()` for `*`)
/// - `Where`: WHERE clause (or `()` for none)
/// - `OrderBy`: ORDER BY clause (or `()` for none)
/// - `Limit`: LIMIT clause (or `()` for none)
/// - `Offset`: OFFSET clause (or `()` for none)
/// - `GroupBy`: GROUP BY clause (or `()` for none)
/// - `Having`: HAVING clause (or `()` for none)
/// - `Distinct`: DISTINCT modifier (`NoDistinct`, `WithDistinct`, or `WithDistinctOn<E>`)
#[derive(Debug, Clone, Copy)]
pub struct SelectStatement<From, Select = (), Where = (), OrderBy = (), Limit = (), Offset = (), GroupBy = (), Having = (), Distinct = NoDistinct> {
    from: From,
    select: Select,
    where_clause: Where,
    order_by: OrderBy,
    limit: Limit,
    offset: Offset,
    group_by: GroupBy,
    having: Having,
    distinct: Distinct,
}

impl<F> SelectStatement<F> {
    /// Create a new SELECT statement from a table.
    pub fn new(from: F) -> Self {
        SelectStatement {
            from,
            select: (),
            where_clause: (),
            order_by: (),
            limit: (),
            offset: (),
            group_by: (),
            having: (),
            distinct: NoDistinct,
        }
    }
}

impl<F, S, W, O, L, Of, G, H, D> SelectStatement<F, S, W, O, L, Of, G, H, D> {
    /// Set the columns to select.
    pub fn select<NewS>(self, select: NewS) -> SelectStatement<F, NewS, W, O, L, Of, G, H, D>
    where
        NewS: Expression,
    {
        SelectStatement {
            from: self.from,
            select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Add a WHERE clause.
    ///
    /// **Note:** Calling `.filter()` twice replaces the first condition.
    /// To combine conditions, use `.filter(a).filter(b)` after this fix,
    /// or use `.filter(a.and(b))`.
    ///
    /// ```rust,ignore
    /// // Two conditions with AND:
    /// users::table
    ///     .filter(users::active.eq(true).and(users::age.gt(18)))
    /// ```
    pub fn filter<P>(self, predicate: P) -> SelectStatement<F, S, P, O, L, Of, G, H, D>
    where
        P: Expression,
    {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: predicate,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Add an ORDER BY clause.
    pub fn order_by<E>(self, expr: E) -> SelectStatement<F, S, W, E, L, Of, G, H, D>
    where
        E: Expression,
    {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: self.where_clause,
            order_by: expr,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Add a LIMIT clause.
    pub fn limit(self, limit: i64) -> SelectStatement<F, S, W, O, LimitClause, Of, G, H, D> {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: LimitClause(limit),
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Add an OFFSET clause.
    pub fn offset(self, offset: i64) -> SelectStatement<F, S, W, O, L, OffsetClause, G, H, D> {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: OffsetClause(offset),
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Add a GROUP BY clause.
    pub fn group_by<E>(self, expr: E) -> SelectStatement<F, S, W, O, L, Of, E, H, D>
    where
        E: Expression,
    {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: expr,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Add a HAVING clause.
    pub fn having<P>(self, predicate: P) -> SelectStatement<F, S, W, O, L, Of, G, P, D>
    where
        P: Expression,
    {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: predicate,
            distinct: self.distinct,
        }
    }

    /// Apply DISTINCT modifier to the query.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// users::table.select(users::name).distinct()
    /// // Generates: SELECT DISTINCT `name` FROM `users`
    /// ```
    pub fn distinct(self) -> SelectStatement<F, S, W, O, L, Of, G, H, WithDistinct> {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: WithDistinct,
        }
    }

    /// Apply DISTINCT ON modifier to the query (ClickHouse-specific).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// users::table
    ///     .select((users::name, users::age))
    ///     .distinct_on(users::department)
    /// // Generates: SELECT DISTINCT ON (`department`) `name`, `age` FROM `users`
    /// ```
    pub fn distinct_on<E>(self, on_expr: E) -> SelectStatement<F, S, W, O, L, Of, G, H, WithDistinctOn<E>>
    where
        E: Expression,
    {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: WithDistinctOn::new(on_expr),
        }
    }

    /// Create an INNER JOIN with another table.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// users::table
    ///     .inner_join(posts::table)
    ///     .select((users::id, posts::title))
    /// ```
    pub fn inner_join<R>(self, rhs: R) -> SelectStatement<Join<F, R, Inner, <F as JoinTo<R>>::OnClause>, S, W, O, L, Of, G, H, D>
    where
        F: JoinTo<R>,
    {
        let on = self.from.on_clause();
        SelectStatement {
            from: Join::new(self.from, rhs, on),
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Create a LEFT JOIN with another table.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// users::table
    ///     .left_join(posts::table)
    ///     .select((users::id, posts::title.nullable()))
    /// ```
    pub fn left_join<R>(self, rhs: R) -> SelectStatement<Join<F, R, Left, <F as JoinTo<R>>::OnClause>, S, W, O, L, Of, G, H, D>
    where
        F: JoinTo<R>,
    {
        let on = self.from.on_clause();
        SelectStatement {
            from: Join::new(self.from, rhs, on),
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Create a RIGHT JOIN with another table.
    pub fn right_join<R>(self, rhs: R) -> SelectStatement<Join<F, R, Right, <F as JoinTo<R>>::OnClause>, S, W, O, L, Of, G, H, D>
    where
        F: JoinTo<R>,
    {
        let on = self.from.on_clause();
        SelectStatement {
            from: Join::new(self.from, rhs, on),
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Create an INNER JOIN with a custom ON clause.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// users::table
    ///     .inner_join_on(posts::table, users::id.eq(posts::user_id))
    ///     .select((users::id, posts::title))
    /// ```
    pub fn inner_join_on<R, On>(self, rhs: R, on: On) -> SelectStatement<Join<F, R, Inner, On>, S, W, O, L, Of, G, H, D>
    where
        On: Expression,
    {
        SelectStatement {
            from: Join::new(self.from, rhs, on),
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Create a LEFT JOIN with a custom ON clause.
    pub fn left_join_on<R, On>(self, rhs: R, on: On) -> SelectStatement<Join<F, R, Left, On>, S, W, O, L, Of, G, H, D>
    where
        On: Expression,
    {
        SelectStatement {
            from: Join::new(self.from, rhs, on),
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Create an ARRAY JOIN (ClickHouse-specific).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// events::table
    ///     .array_join(events::tags)
    ///     .select((events::id, events::tags))
    /// ```
    pub fn array_join<A>(self, array: A) -> SelectStatement<ArrayJoin<F, A>, S, W, O, L, Of, G, H, D>
    where
        A: Expression,
    {
        SelectStatement {
            from: ArrayJoin::new(self.from, array),
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }

    /// Create a LEFT ARRAY JOIN (includes rows with empty arrays).
    pub fn left_array_join<A>(self, array: A) -> SelectStatement<ArrayJoin<F, A>, S, W, O, L, Of, G, H, D>
    where
        A: Expression,
    {
        SelectStatement {
            from: ArrayJoin::left(self.from, array),
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }
}

// Specialized impl for adding to an existing WHERE clause
impl<F, S, W, O, L, Of, G, H, D> SelectStatement<F, S, W, O, L, Of, G, H, D>
where
    W: Expression,
{
    /// Add an additional condition to the WHERE clause with AND.
    ///
    /// Use this to chain multiple conditions:
    /// ```rust,ignore
    /// users::table
    ///     .filter(users::active.eq(true))
    ///     .and_filter(users::age.gt(18))
    /// // Generates: WHERE active = true AND age > 18
    /// ```
    pub fn and_filter<P>(self, predicate: P) -> SelectStatement<F, S, crate::expression::And<W, P>, O, L, Of, G, H, D>
    where
        P: Expression,
    {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: crate::expression::And {
                left: self.where_clause,
                right: predicate,
            },
            order_by: self.order_by,
            limit: self.limit,
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
            distinct: self.distinct,
        }
    }
}

/// LIMIT clause.
#[derive(Debug, Clone, Copy)]
pub struct LimitClause(pub i64);

/// OFFSET clause.
#[derive(Debug, Clone, Copy)]
pub struct OffsetClause(pub i64);

// Marker trait for "no clause"
trait IsEmpty {
    fn is_empty(&self) -> bool;
}

impl IsEmpty for () {
    fn is_empty(&self) -> bool {
        true
    }
}

impl<T> IsEmpty for T
where
    T: Expression,
{
    fn is_empty(&self) -> bool {
        false
    }
}

impl IsEmpty for LimitClause {
    fn is_empty(&self) -> bool {
        false
    }
}

impl IsEmpty for OffsetClause {
    fn is_empty(&self) -> bool {
        false
    }
}

// =============================================================================
// QueryFragment implementations for SelectStatement
// =============================================================================

/// Helper function to generate common SELECT parts after the initial SELECT keyword.
fn write_select_body<'b, F, S, W, O, L, Of, G, H, DB>(
    stmt: &'b SelectStatement<F, S, W, O, L, Of, G, H, impl DistinctClause>,
    mut pass: AstPass<'_, 'b, DB>,
) -> QueryResult<()>
where
    F: QueryFragment<DB>,
    S: QueryFragment<DB> + IsEmpty,
    W: QueryFragment<DB> + IsEmpty,
    O: QueryFragment<DB> + IsEmpty,
    L: QueryFragment<DB> + IsEmpty,
    Of: QueryFragment<DB> + IsEmpty,
    G: QueryFragment<DB> + IsEmpty,
    H: QueryFragment<DB> + IsEmpty,
    DB: Backend,
{
    // Select clause (default to *)
    if stmt.select.is_empty() {
        pass.push_sql("*");
    } else {
        stmt.select.walk_ast(pass.reborrow())?;
    }

    pass.push_sql(" FROM ");
    stmt.from.walk_ast(pass.reborrow())?;

    // WHERE clause
    if !stmt.where_clause.is_empty() {
        pass.push_sql(" WHERE ");
        stmt.where_clause.walk_ast(pass.reborrow())?;
    }

    // GROUP BY clause
    if !stmt.group_by.is_empty() {
        pass.push_sql(" GROUP BY ");
        stmt.group_by.walk_ast(pass.reborrow())?;
    }

    // HAVING clause
    if !stmt.having.is_empty() {
        pass.push_sql(" HAVING ");
        stmt.having.walk_ast(pass.reborrow())?;
    }

    // ORDER BY clause
    if !stmt.order_by.is_empty() {
        pass.push_sql(" ORDER BY ");
        stmt.order_by.walk_ast(pass.reborrow())?;
    }

    // LIMIT clause
    if !stmt.limit.is_empty() {
        stmt.limit.walk_ast(pass.reborrow())?;
    }

    // OFFSET clause
    if !stmt.offset.is_empty() {
        stmt.offset.walk_ast(pass.reborrow())?;
    }

    Ok(())
}

// QueryFragment for SELECT (no DISTINCT)
impl<F, S, W, O, L, Of, G, H, DB> QueryFragment<DB> for SelectStatement<F, S, W, O, L, Of, G, H, NoDistinct>
where
    F: QueryFragment<DB>,
    S: QueryFragment<DB> + IsEmpty,
    W: QueryFragment<DB> + IsEmpty,
    O: QueryFragment<DB> + IsEmpty,
    L: QueryFragment<DB> + IsEmpty,
    Of: QueryFragment<DB> + IsEmpty,
    G: QueryFragment<DB> + IsEmpty,
    H: QueryFragment<DB> + IsEmpty,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("SELECT ");
        write_select_body(self, pass)
    }
}

// QueryFragment for SELECT DISTINCT
impl<F, S, W, O, L, Of, G, H, DB> QueryFragment<DB> for SelectStatement<F, S, W, O, L, Of, G, H, WithDistinct>
where
    F: QueryFragment<DB>,
    S: QueryFragment<DB> + IsEmpty,
    W: QueryFragment<DB> + IsEmpty,
    O: QueryFragment<DB> + IsEmpty,
    L: QueryFragment<DB> + IsEmpty,
    Of: QueryFragment<DB> + IsEmpty,
    G: QueryFragment<DB> + IsEmpty,
    H: QueryFragment<DB> + IsEmpty,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("SELECT DISTINCT ");
        write_select_body(self, pass)
    }
}

// QueryFragment for SELECT DISTINCT ON (ClickHouse-specific)
impl<F, S, W, O, L, Of, G, H, E, DB> QueryFragment<DB> for SelectStatement<F, S, W, O, L, Of, G, H, WithDistinctOn<E>>
where
    F: QueryFragment<DB>,
    S: QueryFragment<DB> + IsEmpty,
    W: QueryFragment<DB> + IsEmpty,
    O: QueryFragment<DB> + IsEmpty,
    L: QueryFragment<DB> + IsEmpty,
    Of: QueryFragment<DB> + IsEmpty,
    G: QueryFragment<DB> + IsEmpty,
    H: QueryFragment<DB> + IsEmpty,
    E: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("SELECT DISTINCT ON (");
        self.distinct.on_expr.walk_ast(pass.reborrow())?;
        pass.push_sql(") ");
        write_select_body(self, pass)
    }
}

impl<DB: Backend> QueryFragment<DB> for LimitClause {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(" LIMIT ");
        pass.push_bindable(&self.0)?;
        Ok(())
    }
}

impl<DB: Backend> QueryFragment<DB> for OffsetClause {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(" OFFSET ");
        pass.push_bindable(&self.0)?;
        Ok(())
    }
}

// =============================================================================
// QueryOutputType implementations
// =============================================================================

/// Marker trait for types that provide a table's AllColumnsSqlType.
/// This is used to handle SELECT * (S = ()) differently from explicit selection.
pub trait HasAllColumnsSqlType {
    /// The SQL type of all columns combined.
    type AllColumnsSqlType: diesel_clickhouse_types::SqlType;
}

impl<T: Table> HasAllColumnsSqlType for T {
    type AllColumnsSqlType = T::AllColumnsSqlType;
}

/// Implementation for joins - combines both tables' column types.
impl<L, R, Kind, On> HasAllColumnsSqlType for Join<L, R, Kind, On>
where
    L: HasAllColumnsSqlType,
    R: HasAllColumnsSqlType,
    Kind: JoinType,
{
    type AllColumnsSqlType = (L::AllColumnsSqlType, R::AllColumnsSqlType);
}

/// Implementation for array joins - same as the base table.
impl<F, A> HasAllColumnsSqlType for ArrayJoin<F, A>
where
    F: HasAllColumnsSqlType,
{
    type AllColumnsSqlType = F::AllColumnsSqlType;
}

/// Implementation for tables directly (users::table).
/// When a table is used directly, the output type is all columns.
impl<T: Table> QueryOutputType for T {
    type SqlType = T::AllColumnsSqlType;
}

/// Implementation for SELECT with explicit columns (S: Expression).
/// The output type is the SQL type of the selected expression.
impl<F, S, W, O, L, Of, G, H, D> QueryOutputType for SelectStatement<F, S, W, O, L, Of, G, H, D>
where
    S: Expression,
{
    type SqlType = S::SqlType;
}

/// Implementation for SELECT * (S = ()).
/// The output type is all columns of the source.
impl<F, W, O, L, Of, G, H, D> QueryOutputType for SelectStatement<F, (), W, O, L, Of, G, H, D>
where
    F: HasAllColumnsSqlType,
{
    type SqlType = F::AllColumnsSqlType;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HttpBackend, HttpQueryBuilder, HttpBindCollector, QueryBuilder as _};
    use crate::expression::{Expression, SelectableExpression, AppearsOnTable};
    use crate::query_source::Table;
    use diesel_clickhouse_types::UInt64;

    // Simple column for testing
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

    // Test table: posts
    #[derive(Debug, Clone, Copy)]
    struct PostsTable;

    impl Table for PostsTable {
        type PrimaryKey = IdColumn;
        type AllColumnsSqlType = UInt64;
        type AllColumns = IdColumn;

        fn table_name() -> &'static str { "posts" }
        fn primary_key() -> Self::PrimaryKey { IdColumn }
        fn all_columns() -> Self::AllColumns { IdColumn }
    }

    impl crate::query_source::QuerySource for PostsTable {
        type FromClause = Self;
        type DefaultSelection = IdColumn;
        fn from_clause(&self) -> Self::FromClause { *self }
        fn default_selection(&self) -> Self::DefaultSelection { IdColumn }
    }

    impl<DB: Backend> QueryFragment<DB> for PostsTable {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("posts");
            Ok(())
        }
    }

    // Simple join condition
    #[derive(Debug, Clone, Copy)]
    struct JoinCondition;

    impl Expression for JoinCondition {
        type SqlType = diesel_clickhouse_types::Bool;
    }

    impl<DB: Backend> QueryFragment<DB> for JoinCondition {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_sql("users.id = posts.user_id");
            Ok(())
        }
    }

    fn to_sql<T: QueryFragment<HttpBackend>>(fragment: &T) -> String {
        let mut builder = HttpQueryBuilder::default();
        let mut collector = HttpBindCollector::default();
        let pass = AstPass::<HttpBackend>::new(&mut builder, &mut collector);
        fragment.walk_ast(pass).unwrap();
        builder.finish()
    }

    #[test]
    fn test_inner_join_on() {
        let stmt = SelectStatement::new(UsersTable)
            .inner_join_on(PostsTable, JoinCondition);
        let sql = to_sql(&stmt);

        assert_eq!(sql, "SELECT * FROM `users` INNER JOIN `posts` ON users.id = posts.user_id");
    }

    #[test]
    fn test_left_join_on() {
        let stmt = SelectStatement::new(UsersTable)
            .left_join_on(PostsTable, JoinCondition);
        let sql = to_sql(&stmt);

        assert_eq!(sql, "SELECT * FROM `users` LEFT JOIN `posts` ON users.id = posts.user_id");
    }

    #[test]
    fn test_join_with_filter() {
        let stmt = SelectStatement::new(UsersTable)
            .inner_join_on(PostsTable, JoinCondition)
            .filter(IdColumn);
        let sql = to_sql(&stmt);

        assert_eq!(sql, "SELECT * FROM `users` INNER JOIN `posts` ON users.id = posts.user_id WHERE `id`");
    }

    // =========================================================================
    // DISTINCT tests (type-level)
    // =========================================================================

    #[test]
    fn test_distinct_type_level() {
        let stmt = SelectStatement::new(UsersTable)
            .select(IdColumn)
            .distinct();
        let sql = to_sql(&stmt);

        assert_eq!(sql, "SELECT DISTINCT `id` FROM `users`");
    }

    #[test]
    fn test_distinct_on_type_level() {
        // Create a department column for DISTINCT ON
        #[derive(Debug, Clone, Copy)]
        struct DeptColumn;
        impl Expression for DeptColumn {
            type SqlType = diesel_clickhouse_types::CHString;
        }
        impl<T> SelectableExpression<T> for DeptColumn {}
        impl<T> AppearsOnTable<T> for DeptColumn {}
        impl<DB: Backend> QueryFragment<DB> for DeptColumn {
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                pass.push_identifier("department");
                Ok(())
            }
        }

        let stmt = SelectStatement::new(UsersTable)
            .select(IdColumn)
            .distinct_on(DeptColumn);
        let sql = to_sql(&stmt);

        assert_eq!(sql, "SELECT DISTINCT ON (`department`) `id` FROM `users`");
    }

    #[test]
    fn test_distinct_with_filter() {
        let stmt = SelectStatement::new(UsersTable)
            .select(IdColumn)
            .distinct()
            .filter(IdColumn);
        let sql = to_sql(&stmt);

        assert_eq!(sql, "SELECT DISTINCT `id` FROM `users` WHERE `id`");
    }

    #[test]
    fn test_distinct_with_order_by() {
        let stmt = SelectStatement::new(UsersTable)
            .select(IdColumn)
            .distinct()
            .order_by(IdColumn);
        let sql = to_sql(&stmt);

        assert_eq!(sql, "SELECT DISTINCT `id` FROM `users` ORDER BY `id`");
    }
}
