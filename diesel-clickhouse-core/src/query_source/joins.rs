//! Join types and traits.

use std::marker::PhantomData;

use crate::expression::Expression;
use crate::backend::Backend;
use crate::query_builder::{QueryFragment, AstPass};
use crate::result::QueryResult;
use super::Table;

/// Join type marker.
pub trait JoinType: Copy + Clone {
    /// The SQL keyword for this join type.
    fn join_sql() -> &'static str;
}

/// INNER JOIN marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct Inner;

impl JoinType for Inner {
    fn join_sql() -> &'static str {
        "INNER JOIN"
    }
}

/// LEFT JOIN marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct Left;

impl JoinType for Left {
    fn join_sql() -> &'static str {
        "LEFT JOIN"
    }
}

/// RIGHT JOIN marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct Right;

impl JoinType for Right {
    fn join_sql() -> &'static str {
        "RIGHT JOIN"
    }
}

/// FULL JOIN marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct Full;

impl JoinType for Full {
    fn join_sql() -> &'static str {
        "FULL JOIN"
    }
}

/// CROSS JOIN marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cross;

impl JoinType for Cross {
    fn join_sql() -> &'static str {
        "CROSS JOIN"
    }
}

/// A joined query source.
#[derive(Debug, Clone, Copy)]
pub struct Join<L, R, Kind: JoinType, On = ()> {
    left: L,
    right: R,
    on: On,
    _kind: PhantomData<Kind>,
}

impl<L, R, Kind: JoinType, On> Join<L, R, Kind, On> {
    /// Create a new join.
    pub fn new(left: L, right: R, on: On) -> Self {
        Self {
            left,
            right,
            on,
            _kind: PhantomData,
        }
    }
}


impl<L, R, Kind, On, DB> QueryFragment<DB> for Join<L, R, Kind, On>
where
    L: QueryFragment<DB>,
    R: QueryFragment<DB>,
    On: QueryFragment<DB>,
    Kind: JoinType,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.left.walk_ast(pass.reborrow())?;
        pass.push_sql(" ");
        pass.push_sql(Kind::join_sql());
        pass.push_sql(" ");
        self.right.walk_ast(pass.reborrow())?;
        pass.push_sql(" ON ");
        self.on.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// Trait for types that can be joined.
pub trait JoinTo<Rhs> {
    /// The ON clause type.
    type OnClause;

    /// Get the ON clause.
    fn on_clause(&self) -> Self::OnClause;
}

/// Extension trait for joining tables.
pub trait JoinDsl: Sized {
    /// Create an INNER JOIN.
    fn inner_join<R>(self, rhs: R) -> Join<Self, R, Inner, <Self as JoinTo<R>>::OnClause>
    where
        Self: JoinTo<R>,
    {
        let on = self.on_clause();
        Join::new(self, rhs, on)
    }

    /// Create a LEFT JOIN.
    fn left_join<R>(self, rhs: R) -> Join<Self, R, Left, <Self as JoinTo<R>>::OnClause>
    where
        Self: JoinTo<R>,
    {
        let on = self.on_clause();
        Join::new(self, rhs, on)
    }

    /// Create a RIGHT JOIN.
    fn right_join<R>(self, rhs: R) -> Join<Self, R, Right, <Self as JoinTo<R>>::OnClause>
    where
        Self: JoinTo<R>,
    {
        let on = self.on_clause();
        Join::new(self, rhs, on)
    }

    /// Create an INNER JOIN with custom ON clause.
    fn inner_join_on<R, On>(self, rhs: R, on: On) -> Join<Self, R, Inner, On>
    where
        On: Expression,
    {
        Join::new(self, rhs, on)
    }

    /// Create a LEFT JOIN with custom ON clause.
    fn left_join_on<R, On>(self, rhs: R, on: On) -> Join<Self, R, Left, On>
    where
        On: Expression,
    {
        Join::new(self, rhs, on)
    }
}

impl<T: Table> JoinDsl for T {}

// =============================================================================
// ClickHouse-specific: ARRAY JOIN
// =============================================================================

/// ARRAY JOIN clause.
#[derive(Debug, Clone, Copy)]
pub struct ArrayJoin<T, A> {
    source: T,
    array: A,
    is_left: bool,
}

impl<T, A> ArrayJoin<T, A> {
    /// Create a new ARRAY JOIN.
    pub fn new(source: T, array: A) -> Self {
        Self {
            source,
            array,
            is_left: false,
        }
    }

    /// Create a new LEFT ARRAY JOIN.
    pub fn left(source: T, array: A) -> Self {
        Self {
            source,
            array,
            is_left: true,
        }
    }
}

impl<T, A, DB> QueryFragment<DB> for ArrayJoin<T, A>
where
    T: QueryFragment<DB>,
    A: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.source.walk_ast(pass.reborrow())?;
        if self.is_left {
            pass.push_sql(" LEFT ARRAY JOIN ");
        } else {
            pass.push_sql(" ARRAY JOIN ");
        }
        self.array.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// Extension trait for ARRAY JOIN.
pub trait ArrayJoinDsl: Sized {
    /// Create an ARRAY JOIN.
    fn array_join<A>(self, array: A) -> ArrayJoin<Self, A>
    where
        A: Expression,
    {
        ArrayJoin::new(self, array)
    }

    /// Create a LEFT ARRAY JOIN (includes empty arrays).
    fn left_array_join<A>(self, array: A) -> ArrayJoin<Self, A>
    where
        A: Expression,
    {
        ArrayJoin::left(self, array)
    }
}

impl<T> ArrayJoinDsl for T {}
