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
/// This trait is automatically implemented by `#[derive(ClickHouseRow)]` and provides
/// optimized deserialization without JSON intermediate conversion.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::{ClickHouseRow, native::FromNativeBlock};
///
/// #[derive(Debug, ClickHouseRow)]
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
/// This is used by `#[derive(ClickHouseRow)]` to extract individual field values.
/// Generic over the column type K to support both Complex and Simple blocks.
///
/// # Performance Note
///
/// This trait returns owned values, which requires allocation for types like `String`.
/// For zero-copy access to string data, use [`BlockValueRef`] or access the block directly:
/// ```rust,ignore
/// let borrowed: &str = block.get(row_idx, "column_name")?;
/// ```
pub trait BlockValue<K: clickhouse_rs::types::ColumnType = Complex>: Sized {
    /// Get a value from the block at the given row and column name.
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self>;
}

/// Zero-copy trait for extracting borrowed values from a Block column.
///
/// This trait allows reading string data without allocation by borrowing
/// directly from the block's internal buffer.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::native::{BlockValueRef, ComplexBlock};
///
/// fn process_rows(block: &ComplexBlock) -> QueryResult<()> {
///     for row_idx in 0..block.row_count() {
///         // Zero-copy string access
///         let name: &str = BlockValueRef::get_value_ref(block, row_idx, "name")?;
///         println!("Name: {}", name);
///     }
///     Ok(())
/// }
/// ```
pub trait BlockValueRef<'a, K: clickhouse_rs::types::ColumnType = Complex>: Sized {
    /// Get a borrowed value from the block at the given row and column name.
    fn get_value_ref(block: &'a Block<K>, row_idx: usize, column: &str) -> QueryResult<Self>;
}

// Implement BlockValueRef for &str (zero-copy string access)
impl<'a, K: clickhouse_rs::types::ColumnType> BlockValueRef<'a, K> for &'a str {
    #[inline]
    fn get_value_ref(block: &'a Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::column_access("&str", column, e))
    }
}

// Implement BlockValueRef for primitive types (they're Copy, so no allocation anyway)
macro_rules! impl_block_value_ref_copy {
    ($ty:ty, $name:literal) => {
        impl<'a, K: clickhouse_rs::types::ColumnType> BlockValueRef<'a, K> for $ty {
            #[inline]
            fn get_value_ref(block: &'a Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
                block.get(row_idx, column)
                    .map_err(|e| Error::column_access($name, column, e))
            }
        }
    };
}

impl_block_value_ref_copy!(u8, "u8");
impl_block_value_ref_copy!(u16, "u16");
impl_block_value_ref_copy!(u32, "u32");
impl_block_value_ref_copy!(u64, "u64");
impl_block_value_ref_copy!(i8, "i8");
impl_block_value_ref_copy!(i16, "i16");
impl_block_value_ref_copy!(i32, "i32");
impl_block_value_ref_copy!(i64, "i64");
impl_block_value_ref_copy!(f32, "f32");
impl_block_value_ref_copy!(f64, "f64");
impl_block_value_ref_copy!(bool, "bool");

// Macro to implement BlockValue for primitive types with generic K
macro_rules! impl_block_value {
    ($ty:ty, $name:literal) => {
        impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for $ty {
            fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
                block.get(row_idx, column)
                    .map_err(|e| Error::column_access($name, column, e))
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
            .map_err(|e| Error::column_access("String", column, e))?;
        Ok(s.to_string())
    }
}

/// BlockValue implementation for `Cow<'static, str>`.
///
/// # Note on Zero-Copy
///
/// Due to the trait design, this implementation produces an owned `Cow::Owned` variant.
/// The string data is copied from the block because the current trait signature doesn't
/// support lifetime parameters that would tie the borrowed data to the block's lifetime.
///
/// For true zero-copy string access, use the block's native `get()` method directly:
/// ```rust,ignore
/// let borrowed: &str = block.get(row_idx, "column_name")?;
/// ```
///
/// # Use Case
///
/// This implementation is useful when you have an API that accepts `Cow<'_, str>` and
/// you want to use it with `FromNativeBlock` derived types. The `Cow` wrapper provides
/// a consistent API even though the data is always owned in this context.
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for Cow<'static, str> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        let s: &str = block.get(row_idx, column)
            .map_err(|e| Error::column_access("Cow<str>", column, e))?;
        Ok(Cow::Owned(s.to_string()))
    }
}

#[cfg(feature = "chrono")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for chrono::DateTime<chrono_tz::Tz> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        block.get(row_idx, column)
            .map_err(|e| Error::column_access("DateTime<Tz>", column, e))
    }
}

#[cfg(feature = "chrono")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for chrono::DateTime<chrono::FixedOffset> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        // Get as DateTime<Tz> and convert to FixedOffset
        let dt: chrono::DateTime<chrono_tz::Tz> = block.get(row_idx, column)
            .map_err(|e| Error::column_access("DateTime<FixedOffset>", column, e))?;
        Ok(dt.fixed_offset())
    }
}

#[cfg(feature = "chrono")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for chrono::DateTime<chrono::Utc> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        // Get as DateTime<Tz> and convert to Utc
        let dt: chrono::DateTime<chrono_tz::Tz> = block.get(row_idx, column)
            .map_err(|e| Error::column_access("DateTime<Utc>", column, e))?;
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
            .map_err(|e| Error::column_access("Vec", column, e))
    }
}

// =============================================================================
// JSON type support (ClickHouse 24.10+)
// =============================================================================

/// BlockValue implementation for serde_json::Value.
///
/// JSON columns are read as strings (with `output_format_native_write_json_as_string=1`)
/// and then parsed into serde_json::Value.
#[cfg(feature = "json")]
impl<K: clickhouse_rs::types::ColumnType> BlockValue<K> for serde_json::Value {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        let json_str: &str = block.get(row_idx, column)
            .map_err(|e| Error::column_access("JSON", column, e))?;
        serde_json::from_str(json_str)
            .map_err(|e| Error::column_access("JSON (parse)", column, e))
    }
}

/// BlockValue implementation for JsonTyped<T>.
///
/// JSON columns are read as strings and deserialized directly to the target type T.
#[cfg(feature = "json")]
impl<K: clickhouse_rs::types::ColumnType, T: serde::de::DeserializeOwned> BlockValue<K> for diesel_clickhouse_types::JsonTyped<T> {
    fn get_value(block: &Block<K>, row_idx: usize, column: &str) -> QueryResult<Self> {
        let json_str: &str = block.get(row_idx, column)
            .map_err(|e| Error::column_access("JsonTyped", column, e))?;
        let inner: T = serde_json::from_str(json_str)
            .map_err(|e| Error::column_access("JsonTyped (parse)", column, e))?;
        Ok(diesel_clickhouse_types::JsonTyped(inner))
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

/// Process block rows with zero-copy string access.
///
/// This function allows iterating over rows and accessing string columns
/// without allocating new `String` objects. The callback receives the block
/// and row index, allowing use of [`BlockValueRef`] for zero-copy access.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::native::{for_each_row_ref, BlockValueRef, ComplexBlock};
///
/// let count = for_each_row_ref(&block, |block, row_idx| {
///     // Zero-copy string access - no allocation!
///     let name: &str = BlockValueRef::get_value_ref(block, row_idx, "name")?;
///     let id: u64 = BlockValueRef::get_value_ref(block, row_idx, "id")?;
///
///     println!("User {}: {}", id, name);
///     Ok(())
/// })?;
/// ```
///
/// # Performance
///
/// This is significantly faster than `block_to_vec_optimized` when you only
/// need to read data once (e.g., for aggregation, filtering, or streaming).
/// Use this when:
/// - You don't need to store the data after processing
/// - You're working with large string columns
/// - Memory allocation is a bottleneck
#[inline]
pub fn for_each_row_ref<K, F>(block: &Block<K>, mut callback: F) -> QueryResult<usize>
where
    K: clickhouse_rs::types::ColumnType,
    F: FnMut(&Block<K>, usize) -> QueryResult<()>,
{
    let row_count = block.row_count();
    for row_idx in 0..row_count {
        callback(block, row_idx)?;
    }
    Ok(row_count)
}

/// Process block rows with a closure that can borrow from the block.
///
/// Similar to [`for_each_row_ref`], but the callback can return a value
/// that will be collected into a Vec. This is useful when you need to
/// transform data while still benefiting from zero-copy string access
/// during the transformation.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::native::{map_rows_ref, BlockValueRef};
///
/// // Extract just IDs from a block, reading strings without allocation
/// let ids: Vec<u64> = map_rows_ref(&block, |block, row_idx| {
///     let name: &str = BlockValueRef::get_value_ref(block, row_idx, "name")?;
///     if name.starts_with("admin_") {
///         let id: u64 = BlockValueRef::get_value_ref(block, row_idx, "id")?;
///         Ok(Some(id))
///     } else {
///         Ok(None)
///     }
/// })?;
/// ```
#[inline]
pub fn map_rows_ref<K, T, F>(block: &Block<K>, mut callback: F) -> QueryResult<Vec<T>>
where
    K: clickhouse_rs::types::ColumnType,
    F: FnMut(&Block<K>, usize) -> QueryResult<Option<T>>,
{
    let row_count = block.row_count();
    let mut results = Vec::with_capacity(row_count);

    for row_idx in 0..row_count {
        if let Some(value) = callback(block, row_idx)? {
            results.push(value);
        }
    }

    Ok(results)
}
