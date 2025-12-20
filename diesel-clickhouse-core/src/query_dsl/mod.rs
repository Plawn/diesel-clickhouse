//! Query DSL traits for building queries.

use crate::expression::Expression;
use crate::query_source::{Table, QuerySource};
use crate::query_builder::{SelectStatement, ClickHouseQueryExt, Final, Prewhere, Sample, QueryFragment};
use crate::result::QueryResult;
use crate::backend::Backend;

/// The main query DSL trait for building SELECT statements.
pub trait QueryDsl: Sized {
    /// Select specific columns.
    fn select<S>(self, selection: S) -> SelectStatement<Self, S>
    where
        S: Expression,
        Self: QuerySource,
    {
        SelectStatement::new(self).select(selection)
    }

    /// Add a WHERE clause filter.
    fn filter<P>(self, predicate: P) -> SelectStatement<Self, (), P>
    where
        P: Expression,
        Self: QuerySource,
    {
        SelectStatement::new(self).filter(predicate)
    }

    /// Add an ORDER BY clause.
    fn order_by<O>(self, order: O) -> SelectStatement<Self, (), (), O>
    where
        O: Expression,
        Self: QuerySource,
    {
        SelectStatement::new(self).order_by(order)
    }

    /// Limit the number of results.
    fn limit(self, limit: i64) -> SelectStatement<Self, (), (), (), crate::query_builder::LimitClause>
    where
        Self: QuerySource,
    {
        SelectStatement::new(self).limit(limit)
    }

    /// Skip a number of results.
    fn offset(self, offset: i64) -> SelectStatement<Self, (), (), (), (), crate::query_builder::OffsetClause>
    where
        Self: QuerySource,
    {
        SelectStatement::new(self).offset(offset)
    }

    /// Group by columns.
    fn group_by<G>(self, group: G) -> SelectStatement<Self, (), (), (), (), (), G>
    where
        G: Expression,
        Self: QuerySource,
    {
        SelectStatement::new(self).group_by(group)
    }

    /// Find a single record by primary key.
    ///
    /// This creates a filter on the primary key columns and limits to 1 result.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // For a table with primary key `id`:
    /// users::table.find(42)  // SELECT * FROM users WHERE id = 42 LIMIT 1
    /// ```
    fn find<PK>(self, pk: PK) -> FindStatement<Self, Self::PrimaryKey, PK>
    where
        Self: Table,
    {
        FindStatement {
            table: self,
            primary_key: Self::primary_key(),
            pk_value: pk,
        }
    }
}

/// A SELECT statement filtered by primary key.
#[derive(Debug, Clone, Copy)]
pub struct FindStatement<T, PKCol, PK> {
    table: T,
    primary_key: PKCol,
    pk_value: PK,
}

impl<T, PKCol, PK> Expression for FindStatement<T, PKCol, PK>
where
    T: Table,
{
    type SqlType = T::AllColumnsSqlType;
}

impl<T, PKCol, PK, DB> QueryFragment<DB> for FindStatement<T, PKCol, PK>
where
    T: Table + QueryFragment<DB>,
    PKCol: QueryFragment<DB>,
    PK: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: crate::query_builder::AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("SELECT * FROM ");
        self.table.walk_ast(pass.reborrow())?;
        pass.push_sql(" WHERE ");
        self.primary_key.walk_ast(pass.reborrow())?;
        pass.push_sql(" = ");
        self.pk_value.walk_ast(pass.reborrow())?;
        pass.push_sql(" LIMIT 1");
        Ok(())
    }
}

// Blanket implementation for all tables
impl<T: Table> QueryDsl for T {}

/// ClickHouse-specific query DSL extensions.
pub trait ClickHouseQueryDsl: QueryDsl + Sized {
    /// Apply FINAL modifier for deduplication on MergeTree tables.
    fn final_(self) -> Final<Self> {
        ClickHouseQueryExt::final_(self)
    }

    /// Apply PREWHERE for optimized pre-filtering.
    fn prewhere<P>(self, predicate: P) -> Prewhere<Self, P>
    where
        P: Expression,
    {
        ClickHouseQueryExt::prewhere(self, predicate)
    }

    /// Apply SAMPLE for approximate queries.
    fn sample(self, ratio: f64) -> Sample<Self> {
        ClickHouseQueryExt::sample(self, ratio)
    }

    /// Apply SAMPLE with OFFSET.
    fn sample_with_offset(self, ratio: f64, offset: f64) -> Sample<Self> {
        ClickHouseQueryExt::sample_with_offset(self, ratio, offset)
    }
}

// Blanket implementation
impl<T: QueryDsl> ClickHouseQueryDsl for T {}

/// Query execution DSL (async).
#[async_trait::async_trait]
pub trait RunQueryDsl<Conn>: Sized {
    /// Execute and load all results.
    async fn load<U>(self, conn: &mut Conn) -> QueryResult<Vec<U>>
    where
        U: crate::deserialize::FromRow + Send;

    /// Execute and get the first result.
    async fn first<U>(self, conn: &mut Conn) -> QueryResult<U>
    where
        U: crate::deserialize::FromRow + Send;

    /// Execute and get an optional result.
    async fn get_result<U>(self, conn: &mut Conn) -> QueryResult<Option<U>>
    where
        U: crate::deserialize::FromRow + Send;

    /// Execute the query (for INSERT/UPDATE/DELETE).
    async fn execute(self, conn: &mut Conn) -> QueryResult<usize>;
}

/// Extension trait for first_or method.
pub trait FirstOr<Conn>: RunQueryDsl<Conn> {
    /// Get the first result or a default value.
    async fn first_or<U>(self, conn: &mut Conn, default: U) -> QueryResult<U>
    where
        U: crate::deserialize::FromRow + Send,
    {
        match self.get_result(conn).await? {
            Some(value) => Ok(value),
            None => Ok(default),
        }
    }
}

impl<T, Conn> FirstOr<Conn> for T where T: RunQueryDsl<Conn> {}

/// Optional filter helper.
pub trait OptionalFilter: Sized {
    /// Apply a filter only if the value is Some.
    fn optional_filter<V, F, P>(self, value: Option<V>, f: F) -> Self
    where
        F: FnOnce(V) -> P,
        P: Expression;
}
