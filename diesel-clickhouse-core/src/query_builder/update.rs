//! UPDATE statement builder.

use std::marker::PhantomData;

use crate::backend::Backend;
use crate::expression::Expression;
use crate::query_source::Table;
use crate::result::QueryResult;
use super::{QueryFragment, AstPass};

/// An UPDATE statement.
#[derive(Debug, Clone)]
pub struct UpdateStatement<T, U, W = ()> {
    table: PhantomData<T>,
    changeset: U,
    filter: W,
}

impl<T: Table> UpdateStatement<T, (), ()> {
    /// Create a new update statement for a table.
    pub fn new() -> Self {
        Self {
            table: PhantomData,
            changeset: (),
            filter: (),
        }
    }
}

impl<T: Table> Default for UpdateStatement<T, (), ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, U, W> UpdateStatement<T, U, W>
where
    T: Table,
{
    /// Set the values to update.
    pub fn set<C>(self, changeset: C) -> UpdateStatement<T, C, W>
    where
        C: AsChangeset,
    {
        UpdateStatement {
            table: PhantomData,
            changeset,
            filter: self.filter,
        }
    }

    /// Add a filter condition.
    pub fn filter<P>(self, predicate: P) -> UpdateStatement<T, U, P>
    where
        P: Expression,
    {
        UpdateStatement {
            table: PhantomData,
            changeset: self.changeset,
            filter: predicate,
        }
    }
}

impl<T, U, W, DB> QueryFragment<DB> for UpdateStatement<T, U, W>
where
    DB: Backend,
    T: Table + QueryFragment<DB>,
    U: QueryFragment<DB>,
    W: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("ALTER TABLE ");
        T::table_name().walk_ast(pass.reborrow())?;
        pass.push_sql(" UPDATE ");
        self.changeset.walk_ast(pass.reborrow())?;

        // Check if we have a filter (W is not ())
        if std::mem::size_of::<W>() > 0 {
            pass.push_sql(" WHERE ");
            self.filter.walk_ast(pass.reborrow())?;
        }

        Ok(())
    }
}


/// Trait for types that can be used as a changeset in an UPDATE.
#[allow(clippy::wrong_self_convention)] // Intentional: fluent API consumes self
pub trait AsChangeset {
    /// The type of the table this changeset applies to.
    type Target: Table;

    /// The changeset type.
    type Changeset;

    /// Convert to a changeset.
    fn as_changeset(self) -> Self::Changeset;
}

// Implement AsChangeset for Eq expressions (column.eq(value))
impl<C, V, T> AsChangeset for crate::expression::Eq<C, V>
where
    C: crate::query_source::Column<Table = T>,
    T: Table,
{
    type Target = T;
    type Changeset = Self;

    fn as_changeset(self) -> Self::Changeset {
        self
    }
}

// Implement AsChangeset for Assign
impl<C, V, T> AsChangeset for Assign<C, V>
where
    C: crate::query_source::Column<Table = T>,
    T: Table,
{
    type Target = T;
    type Changeset = Self;

    fn as_changeset(self) -> Self::Changeset {
        self
    }
}

// Implement AsChangeset for tuples
macro_rules! impl_as_changeset_tuple {
    ($T0:ident $(, $T:ident)*) => {
        impl<Target, $T0 $(, $T)*> AsChangeset for ($T0, $($T,)*)
        where
            Target: Table,
            $T0: AsChangeset<Target = Target>,
            $($T: AsChangeset<Target = Target>,)*
        {
            type Target = Target;
            type Changeset = Self;

            fn as_changeset(self) -> Self::Changeset {
                self
            }
        }
    };
}

impl_as_changeset_tuple!(A);
impl_as_changeset_tuple!(A, B);
impl_as_changeset_tuple!(A, B, C);
impl_as_changeset_tuple!(A, B, C, D);
impl_as_changeset_tuple!(A, B, C, D, E);
impl_as_changeset_tuple!(A, B, C, D, E, F);
impl_as_changeset_tuple!(A, B, C, D, E, F, G);
impl_as_changeset_tuple!(A, B, C, D, E, F, G, H);

/// A single column assignment.
#[derive(Debug, Clone)]
pub struct Assign<C, V> {
    column: C,
    value: V,
}

impl<C, V> Assign<C, V> {
    /// Create a new assignment.
    pub fn new(column: C, value: V) -> Self {
        Self { column, value }
    }
}

impl<C, V, DB> QueryFragment<DB> for Assign<C, V>
where
    DB: Backend,
    C: QueryFragment<DB>,
    V: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.column.walk_ast(pass.reborrow())?;
        pass.push_sql(" = ");
        self.value.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// Multiple column assignments.
#[derive(Debug, Clone)]
pub struct Assignments<T>(pub T);

// Implement for tuples of assignments
macro_rules! impl_assignments_tuple {
    ($(($idx:tt, $T:ident)),+) => {
        impl<DB: Backend, $($T: QueryFragment<DB>),+> QueryFragment<DB> for Assignments<($($T,)+)> {
            #[allow(unused_assignments)]
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                let mut first = true;
                $(
                    if !first {
                        pass.push_sql(", ");
                    }
                    first = false;
                    (self.0).$idx.walk_ast(pass.reborrow())?;
                )+
                Ok(())
            }
        }
    };
}

impl_assignments_tuple!((0, A));
impl_assignments_tuple!((0, A), (1, B));
impl_assignments_tuple!((0, A), (1, B), (2, C));
impl_assignments_tuple!((0, A), (1, B), (2, C), (3, D));
impl_assignments_tuple!((0, A), (1, B), (2, C), (3, D), (4, E));
impl_assignments_tuple!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F));
impl_assignments_tuple!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G));
impl_assignments_tuple!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G), (7, H));

// Also implement for Vec-like structures for optional updates
impl<DB: Backend, T: QueryFragment<DB>> QueryFragment<DB> for Option<T> {
    fn walk_ast<'b>(&'b self, pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        if let Some(inner) = self {
            inner.walk_ast(pass)?;
        }
        Ok(())
    }
}

/// Helper to create an update statement.
pub fn update<T: Table>(table: T) -> UpdateTarget<T> {
    UpdateTarget { table }
}

/// A target for an UPDATE statement.
pub struct UpdateTarget<T> {
    table: T,
}

impl<T: Table> UpdateTarget<T> {
    /// Filter the rows to update.
    pub fn filter<P>(self, predicate: P) -> FilteredUpdateTarget<T, P>
    where
        P: Expression,
    {
        FilteredUpdateTarget {
            table: self.table,
            filter: predicate,
        }
    }

    /// Set values without filter.
    pub fn set<C>(self, changeset: C) -> UpdateStatement<T, C, ()>
    where
        C: AsChangeset<Target = T>,
    {
        UpdateStatement {
            table: PhantomData,
            changeset,
            filter: (),
        }
    }
}

/// A filtered update target.
#[allow(dead_code)]
pub struct FilteredUpdateTarget<T, P> {
    table: T,
    filter: P,
}

impl<T: Table, P: Expression> FilteredUpdateTarget<T, P> {
    /// Set values to update.
    pub fn set<C>(self, changeset: C) -> UpdateStatement<T, C, P>
    where
        C: AsChangeset<Target = T>,
    {
        UpdateStatement {
            table: PhantomData,
            changeset,
            filter: self.filter,
        }
    }
}
