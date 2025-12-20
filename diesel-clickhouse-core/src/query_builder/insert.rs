//! INSERT statement builder.

use std::marker::PhantomData;

use crate::backend::Backend;
use crate::query_source::Table;
use crate::result::QueryResult;
use super::{QueryFragment, AstPass};

/// An INSERT statement builder.
#[derive(Debug, Clone)]
pub struct InsertStatement<T: Table, V> {
    _table: PhantomData<T>,
    values: V,
}

impl<T: Table, V> InsertStatement<T, V> {
    /// Create a new INSERT statement.
    pub fn new(values: V) -> Self {
        Self {
            _table: PhantomData,
            values,
        }
    }

    /// Get a reference to the values.
    pub fn values_ref(&self) -> &V {
        &self.values
    }
}

/// Start building an INSERT statement.
pub fn insert_into<T: Table>(_table: T) -> InsertInto<T> {
    InsertInto {
        _table: PhantomData,
    }
}

/// Builder for INSERT statements.
#[derive(Debug, Clone, Copy)]
pub struct InsertInto<T: Table> {
    _table: PhantomData<T>,
}

impl<T: Table> InsertInto<T> {
    /// Specify the values to insert (single row or slice).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Single row
    /// insert_into(users::table).values(&new_user);
    ///
    /// // Multiple rows
    /// insert_into(users::table).values(&[user1, user2, user3]);
    /// ```
    pub fn values<V>(self, values: V) -> InsertStatement<T, V>
    where
        V: InsertValues<T>,
    {
        InsertStatement::new(values)
    }
}

/// Trait for a single insertable row.
pub trait Insertable<T: Table>: Sized {
    /// Get the column names for this insert.
    fn column_names() -> &'static [&'static str];

    /// Write a single row's values (without parentheses).
    fn write_value<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()>;
}

/// Trait for values that can be inserted into a table.
///
/// This is implemented for:
/// - `&R` where `R: Insertable<T>` (single row)
/// - `&[R]` where `R: Insertable<T>` (multiple rows)
/// - `&Vec<R>` where `R: Insertable<T>` (multiple rows)
pub trait InsertValues<T: Table> {
    /// Get the column names for this insert.
    fn column_names(&self) -> &'static [&'static str];

    /// Write the VALUES clause to the query.
    fn write_values<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()>;
}

// Single row insertion (reference to Insertable)
impl<T: Table, R: Insertable<T>> InsertValues<T> for &R {
    fn column_names(&self) -> &'static [&'static str] {
        R::column_names()
    }

    fn write_values<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        pass.push_sql("(");
        (*self).write_value(pass)?;
        pass.push_sql(")");
        Ok(())
    }
}

// Slice of rows insertion
impl<T: Table, R: Insertable<T>> InsertValues<T> for &[R] {
    fn column_names(&self) -> &'static [&'static str] {
        R::column_names()
    }

    fn write_values<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        for (i, row) in self.iter().enumerate() {
            if i > 0 {
                pass.push_sql(", ");
            }
            pass.push_sql("(");
            row.write_value(pass)?;
            pass.push_sql(")");
        }
        Ok(())
    }
}


// QueryFragment for InsertStatement
impl<T, V, DB> QueryFragment<DB> for InsertStatement<T, V>
where
    T: Table,
    V: InsertValues<T>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("INSERT INTO ");
        pass.push_identifier(T::table_name());

        // Column names
        let columns = self.values.column_names();
        if !columns.is_empty() {
            pass.push_sql(" (");
            for (i, col) in columns.iter().enumerate() {
                if i > 0 {
                    pass.push_sql(", ");
                }
                pass.push_identifier(col);
            }
            pass.push_sql(")");
        }

        pass.push_sql(" VALUES ");
        self.values.write_values(&mut pass)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HttpBackend, HttpQueryBuilder, HttpBindCollector, QueryBuilder as _};
    use crate::serialize::WriteSqlValue;
    use crate::expression::{Expression, SelectableExpression};
    use diesel_clickhouse_types::UInt64;

    // Minimal column for testing
    #[derive(Debug, Clone, Copy)]
    struct IdColumn;

    impl Expression for IdColumn {
        type SqlType = UInt64;
    }

    impl<T> SelectableExpression<T> for IdColumn {}
    impl<T> crate::expression::AppearsOnTable<T> for IdColumn {}

    impl<DB: Backend> QueryFragment<DB> for IdColumn {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
            pass.push_identifier("id");
            Ok(())
        }
    }

    // Test table definition
    #[derive(Debug, Clone, Copy)]
    struct TestTable;

    impl Table for TestTable {
        type PrimaryKey = IdColumn;
        type AllColumnsSqlType = UInt64;
        type AllColumns = IdColumn;

        fn table_name() -> &'static str {
            "test_table"
        }

        fn primary_key() -> Self::PrimaryKey {
            IdColumn
        }

        fn all_columns() -> Self::AllColumns {
            IdColumn
        }
    }

    impl crate::query_source::QuerySource for TestTable {
        type FromClause = Self;
        type DefaultSelection = IdColumn;

        fn from_clause(&self) -> Self::FromClause {
            *self
        }

        fn default_selection(&self) -> Self::DefaultSelection {
            IdColumn
        }
    }

    // Test row struct
    #[derive(Debug)]
    struct TestRow {
        id: u64,
        name: String,
    }

    impl Insertable<TestTable> for TestRow {
        fn column_names() -> &'static [&'static str] {
            &["id", "name"]
        }

        fn write_value<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
            self.id.write_sql(pass);
            pass.push_sql(", ");
            self.name.write_sql(pass);
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
    fn test_insert_single_row() {
        let row = TestRow {
            id: 42,
            name: "Alice".to_string(),
        };

        let stmt = insert_into(TestTable).values(&row);
        let sql = to_sql(&stmt);

        assert_eq!(sql, "INSERT INTO `test_table` (`id`, `name`) VALUES (42, 'Alice')");
    }

    #[test]
    fn test_insert_multiple_rows() {
        let rows = vec![
            TestRow { id: 1, name: "Alice".to_string() },
            TestRow { id: 2, name: "Bob".to_string() },
        ];

        let stmt = insert_into(TestTable).values(rows.as_slice());
        let sql = to_sql(&stmt);

        assert_eq!(sql, "INSERT INTO `test_table` (`id`, `name`) VALUES (1, 'Alice'), (2, 'Bob')");
    }

    #[test]
    fn test_insert_escapes_strings() {
        let row = TestRow {
            id: 1,
            name: "O'Brien".to_string(),
        };

        let stmt = insert_into(TestTable).values(&row);
        let sql = to_sql(&stmt);

        assert_eq!(sql, "INSERT INTO `test_table` (`id`, `name`) VALUES (1, 'O''Brien')");
    }
}

