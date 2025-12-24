//! Block deserialization traits and implementations.

use std::borrow::Cow;

use clickhouse_rs::{Block, types::Complex};

use crate::core::result::{Error, QueryResult};

/// Type alias for the complex block type used by FromNativeBlock
pub type ComplexBlock = Block<Complex>;

/// Type alias for the simple block type (used in streaming)
pub type SimpleBlock = Block<clickhouse_rs::types::Simple>;

/// Trait for types that can be deserialized directly from a Native Block row.
///
/// This trait is automatically implemented by `#[derive(Row)]` and provides
/// optimized deserialization without JSON intermediate conversion.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::{Row, native::FromNativeBlock};
///
/// #[derive(Debug, Row)]
/// struct User {
///     id: u64,
///     name: String,
/// }
///
/// // FromNativeBlock is auto-implemented, allowing direct Block deserialization
/// ```
pub trait FromNativeBlock: Sized {
    /// Deserialize a row from a Native Block at the given index.
    fn from_block_row(
        block: &ComplexBlock,
        row_idx: usize,
    ) -> QueryResult<Self>;
}

/// Trait for deserializing from any block type (Simple or Complex).
/// This is automatically implemented for types that implement FromNativeBlock
/// and have field types that implement BlockValue for all K.
pub trait FromAnyBlock: Sized {
    /// Deserialize a row from a block of any type at the given index.
    fn from_any_block<K: clickhouse_rs::types::ColumnType>(
        block: &Block<K>,
        row_idx: usize,
    ) -> QueryResult<Self>;
}

/// Helper trait for extracting typed values from a Block column.
///
/// This is used by the `#[derive(Row)]` macro to extract individual field values.
/// Generic over the column type K to support both Complex and Simple blocks.
pub trait BlockValue<K: clickhouse_rs::types::ColumnType = Complex>: Sized {
    /// Get a value from the block at the given row and column name.
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self>;
}

// Macro to implement BlockValue for primitive types with generic K
macro_rules! impl_block_value {
    ($ty:ty, $name:literal) => {
        impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for $ty {
            fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
                block.get(row_idx, column)
                    .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get {} column '{}': {}", $name, column, e))))
            }
        }
    };
}

// Implement BlockValue for common types (generic over K)
impl_block_value!(u8, "u8");
impl_block_value!(u16, "u16");
impl_block_value!(u32, "u32");
impl_block_value!(u64, "u64");
impl_block_value!(i8, "i8");
impl_block_value!(i16, "i16");
impl_block_value!(i32, "i32");
impl_block_value!(i64, "i64");
impl_block_value!(f32, "f32");
impl_block_value!(f64, "f64");
impl_block_value!(bool, "bool");

impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for String {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        let s: &str = block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get String column '{}': {}", column, e))))?;
        Ok(s.to_string())
    }
}

#[cfg(feature = "chrono")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for chrono::DateTime<chrono_tz::Tz> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get DateTime column '{}': {}", column, e))))
    }
}

#[cfg(feature = "chrono")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for chrono::DateTime<chrono::FixedOffset> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        // Get as DateTime<Tz> and convert to FixedOffset
        let dt: chrono::DateTime<chrono_tz::Tz> = block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get DateTime column '{}': {}", column, e))))?;
        Ok(dt.fixed_offset())
    }
}

#[cfg(feature = "chrono")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for chrono::DateTime<chrono::Utc> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        // Get as DateTime<Tz> and convert to Utc
        let dt: chrono::DateTime<chrono_tz::Tz> = block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get DateTime column '{}': {}", column, e))))?;
        Ok(dt.with_timezone(&chrono::Utc))
    }
}

impl<K: clickhouse_rs::types::ColumnType, T: BlockValue<K>> BlockValue<K> for Option<T> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        // Try to get the value, return None only if it's actually NULL
        match T::get_value(block, row_idx, column) {
            Ok(v) => Ok(Some(v)),
            Err(Error::DeserializationError(ref msg)) if msg.contains("Nullable") => Ok(None),
            Err(e) => Err(e), // Propagate real errors instead of masking them
        }
    }
}

impl<K: clickhouse_rs::types::ColumnType, T> BlockValue<K> for Vec<T>
where
    for<'a> Vec<T>: clickhouse_rs::types::FromSql<'a>,
{
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::DeserializationError(Cow::Owned(format!("Failed to get Vec column '{}': {}", column, e))))
    }
}

/// Convert a native Block to a Vec using FromNativeBlock trait (optimized).
pub fn block_to_vec_optimized<T: FromNativeBlock>(block: &ComplexBlock) -> QueryResult<Vec<T>> {
    let row_count = block.row_count();
    let mut results = Vec::with_capacity(row_count);

    for row_idx in 0..row_count {
        results.push(T::from_block_row(block, row_idx)?);
    }

    Ok(results)
}
