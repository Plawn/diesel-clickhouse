//! Column trait and related types.

use diesel_clickhouse_types::SqlType;
use crate::expression::{Expression, SelectableExpression, AppearsOnTable};
use crate::backend::Backend;
use crate::query_builder::{QueryFragment, AstPass};
use crate::result::QueryResult;
use super::Table;

/// Represents a column in a table.
pub trait Column: Expression + SelectableExpression<Self::Table> + Copy + Clone {
    /// The table this column belongs to.
    type Table: Table;

    /// Returns the column name.
    fn column_name() -> &'static str;
}

/// A column reference that can be used in queries.
#[derive(Debug)]
pub struct ColumnRef<T: Table, ST: SqlType> {
    name: &'static str,
    _table: std::marker::PhantomData<T>,
    _sql_type: std::marker::PhantomData<ST>,
}

impl<T: Table, ST: SqlType> Clone for ColumnRef<T, ST> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Table, ST: SqlType> Copy for ColumnRef<T, ST> {}

impl<T: Table, ST: SqlType> ColumnRef<T, ST> {
    /// Create a new column reference.
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            _table: std::marker::PhantomData,
            _sql_type: std::marker::PhantomData,
        }
    }
}

impl<T: Table, ST: SqlType> Expression for ColumnRef<T, ST> {
    type SqlType = ST;
}

impl<T: Table, ST: SqlType> Column for ColumnRef<T, ST> {
    type Table = T;

    fn column_name() -> &'static str {
        // This would need to be fixed with a proper implementation
        ""
    }
}

impl<T: Table, ST: SqlType> SelectableExpression<T> for ColumnRef<T, ST> {}
impl<T: Table, ST: SqlType> AppearsOnTable<T> for ColumnRef<T, ST> {}

impl<T: Table, ST: SqlType, DB: Backend> QueryFragment<DB> for ColumnRef<T, ST> {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_identifier(self.name);
        Ok(())
    }
}

/// A fully qualified column reference (table.column).
#[derive(Debug, Clone, Copy)]
pub struct QualifiedColumn<T: Table, C: Column<Table = T>> {
    table: T,
    column: C,
}

impl<T: Table, C: Column<Table = T>> QualifiedColumn<T, C> {
    /// Create a new qualified column reference.
    pub fn new(table: T, column: C) -> Self {
        Self { table, column }
    }
}

impl<T: Table, C: Column<Table = T>> Expression for QualifiedColumn<T, C> {
    type SqlType = C::SqlType;
}

impl<T: Table, C: Column<Table = T>> SelectableExpression<T> for QualifiedColumn<T, C> {}
impl<T: Table, C: Column<Table = T>> AppearsOnTable<T> for QualifiedColumn<T, C> {}

impl<T: Table, C: Column<Table = T>, DB: Backend> QueryFragment<DB> for QualifiedColumn<T, C> {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_identifier(T::table_name());
        pass.push_sql(".");
        pass.push_identifier(C::column_name());
        Ok(())
    }
}

/// Extension trait for columns.
pub trait ColumnMethods: Column {
    /// Create a fully qualified column reference.
    fn qualified(self) -> QualifiedColumn<Self::Table, Self>
    where
        Self: Sized,
        Self::Table: Default,
    {
        QualifiedColumn::new(Self::Table::default(), self)
    }
}

impl<T: Column> ColumnMethods for T {}
