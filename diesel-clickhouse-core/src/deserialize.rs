//! Deserialization traits for query results.

use std::borrow::Cow;

use crate::result::{QueryResult, Row, Error};
use diesel_clickhouse_types::{SqlType, FromClickHouse};

/// Trait for types that can be constructed from a database row.
pub trait FromRow: Sized {
    /// Build self from a database row.
    fn from_row(row: &dyn Row) -> QueryResult<Self>;
}

/// Trait for types that can be queried with a specific SQL type.
pub trait Queryable<ST: SqlType>: Sized {
    /// The intermediate row type.
    type Row: FromRow;

    /// Build self from the row type.
    fn build(row: Self::Row) -> QueryResult<Self>;
}

// =============================================================================
// Primitive FromRow implementations
// =============================================================================

macro_rules! impl_from_row_primitive {
    ($rust_ty:ty, $sql_ty:ty) => {
        impl FromRow for $rust_ty {
            fn from_row(row: &dyn Row) -> QueryResult<Self> {
                let bytes = row.get_by_index(0)
                    .ok_or_else(|| Error::ColumnNotFound("column 0".into()))?;
                <$rust_ty as FromClickHouse<$sql_ty>>::from_clickhouse(bytes)
                    .map_err(Into::into)
            }
        }
    };
}

use diesel_clickhouse_types::*;

impl_from_row_primitive!(u8, UInt8);
impl_from_row_primitive!(u16, UInt16);
impl_from_row_primitive!(u32, UInt32);
impl_from_row_primitive!(u64, UInt64);
impl_from_row_primitive!(i8, Int8);
impl_from_row_primitive!(i16, Int16);
impl_from_row_primitive!(i32, Int32);
impl_from_row_primitive!(i64, Int64);
impl_from_row_primitive!(f32, Float32);
impl_from_row_primitive!(f64, Float64);
impl_from_row_primitive!(bool, Bool);

impl FromRow for String {
    fn from_row(row: &dyn Row) -> QueryResult<Self> {
        let bytes = row.get_by_index(0)
            .ok_or_else(|| Error::ColumnNotFound(Cow::Borrowed("column 0")))?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| Error::DeserializationError(Cow::Owned(e.to_string())))
    }
}

// =============================================================================
// Tuple FromRow implementations
// =============================================================================

/// A row wrapper that exposes a single column at a given index.
struct IndexedColumnRow<'a> {
    row: &'a dyn Row,
    index: usize,
}

impl<'a> Row for IndexedColumnRow<'a> {
    fn column_count(&self) -> usize {
        1
    }

    fn get_by_index(&self, index: usize) -> Option<&[u8]> {
        if index == 0 {
            self.row.get_by_index(self.index)
        } else {
            None
        }
    }

    fn get_by_name(&self, name: &str) -> Option<&[u8]> {
        // For indexed access, we check if the name matches the column at our index
        if self.row.column_name(self.index) == Some(name) {
            self.row.get_by_index(self.index)
        } else {
            None
        }
    }

    fn column_name(&self, index: usize) -> Option<&str> {
        if index == 0 {
            self.row.column_name(self.index)
        } else {
            None
        }
    }
}

macro_rules! impl_from_row_tuple {
    ($(($idx:tt, $T:ident)),+) => {
        impl<$($T: FromRow),+> FromRow for ($($T,)+) {
            fn from_row(row: &dyn Row) -> QueryResult<Self> {
                Ok((
                    $(
                        {
                            let indexed_row = IndexedColumnRow { row, index: $idx };
                            $T::from_row(&indexed_row)?
                        },
                    )+
                ))
            }
        }
    };
}

impl_from_row_tuple!((0, A));
impl_from_row_tuple!((0, A), (1, B));
impl_from_row_tuple!((0, A), (1, B), (2, C));
impl_from_row_tuple!((0, A), (1, B), (2, C), (3, D));
impl_from_row_tuple!((0, A), (1, B), (2, C), (3, D), (4, E));
impl_from_row_tuple!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F));
impl_from_row_tuple!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G));
impl_from_row_tuple!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G), (7, H));

// =============================================================================
// Option FromRow
// =============================================================================

impl<T: FromRow> FromRow for Option<T> {
    fn from_row(row: &dyn Row) -> QueryResult<Self> {
        match T::from_row(row) {
            Ok(value) => Ok(Some(value)),
            Err(Error::DeserializationError(msg)) if msg.contains("NULL") => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// =============================================================================
// Named field deserialization helper
// =============================================================================

/// Helper for deserializing named fields from a row.
pub struct RowFields<'a> {
    row: &'a dyn Row,
}

impl<'a> RowFields<'a> {
    /// Create a new row fields helper.
    pub fn new(row: &'a dyn Row) -> Self {
        Self { row }
    }

    /// Get a field by name.
    pub fn get<T: FromRow>(&self, name: &str) -> QueryResult<T> {
        let bytes = self.row.get_by_name(name)
            .ok_or_else(|| Error::ColumnNotFound(Cow::Owned(name.to_string())))?;

        // Create a single-column row for the value
        let single_row = SingleValueRow(bytes);
        T::from_row(&single_row)
    }

    /// Get an optional field by name.
    pub fn get_optional<T: FromRow>(&self, name: &str) -> QueryResult<Option<T>> {
        match self.row.get_by_name(name) {
            Some(bytes) => {
                let single_row = SingleValueRow(bytes);
                T::from_row(&single_row).map(Some)
            }
            None => Ok(None),
        }
    }
}

/// A row with a single value (for internal use).
struct SingleValueRow<'a>(&'a [u8]);

impl<'a> Row for SingleValueRow<'a> {
    fn column_count(&self) -> usize {
        1
    }

    fn get_by_index(&self, index: usize) -> Option<&[u8]> {
        if index == 0 {
            Some(self.0)
        } else {
            None
        }
    }

    fn get_by_name(&self, _name: &str) -> Option<&[u8]> {
        Some(self.0)
    }

    fn column_name(&self, index: usize) -> Option<&str> {
        if index == 0 {
            Some("value")
        } else {
            None
        }
    }
}

// =============================================================================
// Queryable<ST> implementations for primitives
// =============================================================================

/// Macro to implement Queryable for primitive types.
macro_rules! impl_queryable_primitive {
    ($rust_ty:ty, $sql_ty:ty) => {
        impl Queryable<$sql_ty> for $rust_ty {
            type Row = $rust_ty;

            fn build(row: Self::Row) -> QueryResult<Self> {
                Ok(row)
            }
        }
    };
}

impl_queryable_primitive!(u8, UInt8);
impl_queryable_primitive!(u16, UInt16);
impl_queryable_primitive!(u32, UInt32);
impl_queryable_primitive!(u64, UInt64);
impl_queryable_primitive!(i8, Int8);
impl_queryable_primitive!(i16, Int16);
impl_queryable_primitive!(i32, Int32);
impl_queryable_primitive!(i64, Int64);
impl_queryable_primitive!(f32, Float32);
impl_queryable_primitive!(f64, Float64);
impl_queryable_primitive!(bool, Bool);
impl_queryable_primitive!(String, CHString);

// =============================================================================
// Queryable<ST> implementations for tuples
// =============================================================================

/// Macro to implement Queryable for tuples.
macro_rules! impl_queryable_tuple {
    ($(($T:ident, $ST:ident)),+) => {
        impl<$($T, $ST),+> Queryable<($($ST,)+)> for ($($T,)+)
        where
            $($T: Queryable<$ST>,)+
            $($ST: SqlType,)+
        {
            type Row = ($($T::Row,)+);

            fn build(row: Self::Row) -> QueryResult<Self> {
                #[allow(non_snake_case)]
                let ($($T,)+) = row;
                Ok(($($T::build($T)?,)+))
            }
        }
    };
}

impl_queryable_tuple!((A, SA));
impl_queryable_tuple!((A, SA), (B, SB));
impl_queryable_tuple!((A, SA), (B, SB), (C, SC));
impl_queryable_tuple!((A, SA), (B, SB), (C, SC), (D, SD));
impl_queryable_tuple!((A, SA), (B, SB), (C, SC), (D, SD), (E, SE));
impl_queryable_tuple!((A, SA), (B, SB), (C, SC), (D, SD), (E, SE), (F, SF));
impl_queryable_tuple!((A, SA), (B, SB), (C, SC), (D, SD), (E, SE), (F, SF), (G, SG));
impl_queryable_tuple!((A, SA), (B, SB), (C, SC), (D, SD), (E, SE), (F, SF), (G, SG), (H, SH));

// =============================================================================
// Queryable<Nullable<ST>> for Option<T>
// =============================================================================

impl<T, ST> Queryable<Nullable<ST>> for Option<T>
where
    T: Queryable<ST>,
    ST: SqlType,
{
    type Row = Option<T::Row>;

    fn build(row: Self::Row) -> QueryResult<Self> {
        match row {
            Some(r) => T::build(r).map(Some),
            None => Ok(None),
        }
    }
}
