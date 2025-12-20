//! SELECT statement builder.


use crate::backend::Backend;
use crate::expression::Expression;
use crate::result::QueryResult;
use super::{QueryFragment, AstPass};

/// A SELECT statement builder.
#[derive(Debug, Clone, Copy)]
pub struct SelectStatement<From, Select = (), Where = (), OrderBy = (), Limit = (), Offset = (), GroupBy = (), Having = ()> {
    from: From,
    select: Select,
    where_clause: Where,
    order_by: OrderBy,
    limit: Limit,
    offset: Offset,
    group_by: GroupBy,
    having: Having,
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
        }
    }
}

impl<F, S, W, O, L, Of, G, H> SelectStatement<F, S, W, O, L, Of, G, H> {
    /// Set the columns to select.
    pub fn select<NewS>(self, select: NewS) -> SelectStatement<F, NewS, W, O, L, Of, G, H>
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
        }
    }

    /// Add a WHERE clause (replaces existing).
    /// Use `and_filter` to add additional conditions to an existing WHERE clause.
    pub fn filter<P>(self, predicate: P) -> SelectStatement<F, S, P, O, L, Of, G, H>
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
        }
    }

    /// Add an ORDER BY clause.
    pub fn order_by<E>(self, expr: E) -> SelectStatement<F, S, W, E, L, Of, G, H>
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
        }
    }

    /// Add a LIMIT clause.
    pub fn limit(self, limit: i64) -> SelectStatement<F, S, W, O, LimitClause, Of, G, H> {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: LimitClause(limit),
            offset: self.offset,
            group_by: self.group_by,
            having: self.having,
        }
    }

    /// Add an OFFSET clause.
    pub fn offset(self, offset: i64) -> SelectStatement<F, S, W, O, L, OffsetClause, G, H> {
        SelectStatement {
            from: self.from,
            select: self.select,
            where_clause: self.where_clause,
            order_by: self.order_by,
            limit: self.limit,
            offset: OffsetClause(offset),
            group_by: self.group_by,
            having: self.having,
        }
    }

    /// Add a GROUP BY clause.
    pub fn group_by<E>(self, expr: E) -> SelectStatement<F, S, W, O, L, Of, E, H>
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
        }
    }

    /// Add a HAVING clause.
    pub fn having<P>(self, predicate: P) -> SelectStatement<F, S, W, O, L, Of, G, P>
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
        }
    }
}

// Specialized impl for adding to an existing WHERE clause
impl<F, S, W, O, L, Of, G, H> SelectStatement<F, S, W, O, L, Of, G, H>
where
    W: Expression,
{
    /// Add an additional condition to the WHERE clause with AND.
    pub fn and_filter<P>(self, predicate: P) -> SelectStatement<F, S, crate::expression::And<W, P>, O, L, Of, G, H>
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

// QueryFragment implementation
impl<F, S, W, O, L, Of, G, H, DB> QueryFragment<DB> for SelectStatement<F, S, W, O, L, Of, G, H>
where
    F: QueryFragment<DB>,
    S: QueryFragment<DB>,
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

        // Select clause (default to *)
        if std::mem::size_of::<S>() == 0 {
            pass.push_sql("*");
        } else {
            self.select.walk_ast(pass.reborrow())?;
        }

        pass.push_sql(" FROM ");
        self.from.walk_ast(pass.reborrow())?;

        // WHERE clause
        if !self.where_clause.is_empty() {
            pass.push_sql(" WHERE ");
            self.where_clause.walk_ast(pass.reborrow())?;
        }

        // GROUP BY clause
        if !self.group_by.is_empty() {
            pass.push_sql(" GROUP BY ");
            self.group_by.walk_ast(pass.reborrow())?;
        }

        // HAVING clause
        if !self.having.is_empty() {
            pass.push_sql(" HAVING ");
            self.having.walk_ast(pass.reborrow())?;
        }

        // ORDER BY clause
        if !self.order_by.is_empty() {
            pass.push_sql(" ORDER BY ");
            self.order_by.walk_ast(pass.reborrow())?;
        }

        // LIMIT clause
        if !self.limit.is_empty() {
            self.limit.walk_ast(pass.reborrow())?;
        }

        // OFFSET clause
        if !self.offset.is_empty() {
            self.offset.walk_ast(pass.reborrow())?;
        }

        Ok(())
    }
}

impl<DB: Backend> QueryFragment<DB> for LimitClause {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(&format!(" LIMIT {}", self.0));
        Ok(())
    }
}

impl<DB: Backend> QueryFragment<DB> for OffsetClause {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(&format!(" OFFSET {}", self.0));
        Ok(())
    }
}
