//! Serialization traits for inserting data.

use crate::backend::Backend;
use crate::query_builder::AstPass;
use crate::result::QueryResult;
use diesel_clickhouse_types::{SqlType, ToClickHouse};

/// Trait for types that can be serialized as a row for insertion.
pub trait ToRow {
    /// Get the column names for this row type.
    fn column_names() -> &'static [&'static str];

    /// Serialize the values to bytes.
    fn to_row_bytes(&self, out: &mut Vec<u8>) -> QueryResult<()>;
}

/// A single value that can be bound as a parameter.
pub trait ToSql<ST: SqlType> {
    /// Serialize to SQL format.
    fn to_sql(&self, out: &mut Vec<u8>) -> QueryResult<()>;
}

// Blanket implementation for types that implement ToClickHouse
impl<T, ST> ToSql<ST> for T
where
    ST: SqlType,
    T: ToClickHouse<ST>,
{
    fn to_sql(&self, out: &mut Vec<u8>) -> QueryResult<()> {
        self.to_clickhouse(out).map_err(Into::into)
    }
}

// =============================================================================
// SQL Value Writing (for INSERT statements)
// =============================================================================

/// Trait for writing SQL literal values.
pub trait WriteSqlValue {
    /// Write the SQL literal representation to the AST pass.
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>);
}

/// Write a value as SQL literal to an AstPass.
pub fn write_sql_value<T: WriteSqlValue, DB: Backend>(value: &T, pass: &mut AstPass<'_, '_, DB>) {
    value.write_sql(pass);
}

/// Write a value as bytes (for binary protocol).
pub fn write_sql_bytes<T>(_value: &T, _out: &mut Vec<u8>) {
    // Binary serialization - not used for SQL generation
}

// Implement WriteSqlValue for common types
impl WriteSqlValue for u8 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for u16 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for u32 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for u64 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for i8 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for i16 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for i32 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for i64 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for f32 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for f64 {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(&self.to_string());
    }
}

impl WriteSqlValue for bool {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql(if *self { "true" } else { "false" });
    }
}

impl WriteSqlValue for String {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql("'");
        pass.push_sql(&self.replace('\'', "''"));
        pass.push_sql("'");
    }
}

impl WriteSqlValue for str {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql("'");
        pass.push_sql(&self.replace('\'', "''"));
        pass.push_sql("'");
    }
}

impl WriteSqlValue for &str {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        pass.push_sql("'");
        pass.push_sql(&self.replace('\'', "''"));
        pass.push_sql("'");
    }
}

impl<T: WriteSqlValue> WriteSqlValue for Option<T> {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        match self {
            Some(value) => value.write_sql(pass),
            None => pass.push_sql("NULL"),
        }
    }
}

impl<T: WriteSqlValue> WriteSqlValue for &T {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) {
        (*self).write_sql(pass);
    }
}

/// A collection of values for a single row.
#[derive(Debug, Default)]
pub struct RowValues {
    columns: Vec<&'static str>,
    values: Vec<Vec<u8>>,
}

impl RowValues {
    /// Create a new row values collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a value.
    pub fn add<T, ST>(&mut self, column: &'static str, value: &T) -> QueryResult<()>
    where
        ST: SqlType,
        T: ToSql<ST>,
    {
        let mut bytes = Vec::new();
        value.to_sql(&mut bytes)?;
        self.columns.push(column);
        self.values.push(bytes);
        Ok(())
    }

    /// Get the column names.
    pub fn columns(&self) -> &[&'static str] {
        &self.columns
    }

    /// Get the values.
    pub fn values(&self) -> &[Vec<u8>] {
        &self.values
    }
}

// =============================================================================
// Primitive ToRow implementations
// =============================================================================

macro_rules! impl_to_row_primitive {
    ($rust_ty:ty, $sql_ty:ty) => {
        impl ToRow for $rust_ty {
            fn column_names() -> &'static [&'static str] {
                &["value"]
            }

            fn to_row_bytes(&self, out: &mut Vec<u8>) -> QueryResult<()> {
                <Self as ToSql<$sql_ty>>::to_sql(self, out)
            }
        }
    };
}

use diesel_clickhouse_types::*;

impl_to_row_primitive!(u8, UInt8);
impl_to_row_primitive!(u16, UInt16);
impl_to_row_primitive!(u32, UInt32);
impl_to_row_primitive!(u64, UInt64);
impl_to_row_primitive!(i8, Int8);
impl_to_row_primitive!(i16, Int16);
impl_to_row_primitive!(i32, Int32);
impl_to_row_primitive!(i64, Int64);
impl_to_row_primitive!(f32, Float32);
impl_to_row_primitive!(f64, Float64);
impl_to_row_primitive!(bool, Bool);
impl_to_row_primitive!(String, CHString);

// =============================================================================
// Tuple ToRow implementations
// =============================================================================

macro_rules! impl_to_row_tuple {
    ($(($idx:tt, $T:ident, $col:literal)),+) => {
        impl<$($T: ToRow),+> ToRow for ($($T,)+) {
            fn column_names() -> &'static [&'static str] {
                &[$($col),+]
            }

            fn to_row_bytes(&self, out: &mut Vec<u8>) -> QueryResult<()> {
                $(
                    self.$idx.to_row_bytes(out)?;
                )+
                Ok(())
            }
        }
    };
}

impl_to_row_tuple!((0, A, "0"));
impl_to_row_tuple!((0, A, "0"), (1, B, "1"));
impl_to_row_tuple!((0, A, "0"), (1, B, "1"), (2, C, "2"));
impl_to_row_tuple!((0, A, "0"), (1, B, "1"), (2, C, "2"), (3, D, "3"));

// =============================================================================
// Option ToRow
// =============================================================================

impl<T: ToRow> ToRow for Option<T> {
    fn column_names() -> &'static [&'static str] {
        T::column_names()
    }

    fn to_row_bytes(&self, out: &mut Vec<u8>) -> QueryResult<()> {
        match self {
            Some(value) => {
                out.push(0); // NOT NULL flag
                value.to_row_bytes(out)
            }
            None => {
                out.push(1); // NULL flag
                Ok(())
            }
        }
    }
}
