//! Serialization traits for inserting data.

use crate::result::QueryResult;
use diesel_clickhouse_types::{SqlType, ToClickHouse};

/// Trait for types that can be serialized as a row for insertion.
pub trait ToRow {
    /// Get the column names for this row type.
    fn column_names() -> &'static [&'static str];

    /// Serialize the values to bytes.
    fn to_row_bytes(&self, out: &mut Vec<u8>) -> QueryResult<()>;
}

/// Trait for types that can be inserted into a table.
pub trait Insertable<T> {
    /// The values type for this insertable.
    type Values: ToRow;

    /// Convert to values.
    fn into_values(self) -> Self::Values;
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
