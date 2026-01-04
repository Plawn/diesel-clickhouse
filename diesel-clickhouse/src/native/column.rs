//! Block column serialization traits for INSERT operations.

use std::borrow::Cow;

use clickhouse_rs::Block;

use crate::core::result::QueryResult;

/// Trait for types that can be converted to a native Block for INSERT.
///
/// This trait enables optimized binary INSERT operations using ClickHouse's
/// native Block format instead of generating SQL VALUES text. This provides
/// better performance for bulk inserts.
///
/// This trait is automatically implemented by the `#[row]` attribute macro
/// for types that also derive `Insertable`.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::prelude::*;
/// use diesel_clickhouse::native::ToNativeBlock;
///
/// #[row]
/// #[derive(Debug, Clone, Insertable)]
/// #[diesel_clickhouse(table = users)]
/// struct NewUser {
///     id: u64,
///     name: String,
///     active: bool,
/// }
///
/// // Optimized insert via Block API
/// let users = vec![
///     NewUser { id: 1, name: "Alice".into(), active: true },
///     NewUser { id: 2, name: "Bob".into(), active: false },
/// ];
/// conn.insert_native("users", &users).await?;
/// ```
pub trait ToNativeBlock: Sized {
    /// Column names for this row type.
    fn column_names() -> &'static [&'static str];

    /// Convert a slice of rows to a native Block for efficient INSERT.
    ///
    /// This method borrows the rows, which may require cloning for types
    /// like `String` and `Vec<T>`. For better performance when you have
    /// owned data, use [`rows_into_block`](Self::rows_into_block) instead.
    fn rows_to_block(rows: &[Self]) -> QueryResult<Block>;

    /// Convert owned rows to a native Block, avoiding clones where possible.
    ///
    /// This is more efficient than `rows_to_block` for types containing
    /// `String` or `Vec<T>` fields, as it moves the data instead of cloning.
    ///
    /// The default implementation clones each row and calls `rows_to_block`.
    /// The derive macro generates an optimized version that avoids cloning.
    fn rows_into_block(rows: Vec<Self>) -> QueryResult<Block>
    where
        Self: Clone,
    {
        Self::rows_to_block(&rows)
    }
}

/// Trait for types that can be added as a column to a Block.
///
/// This is implemented for common Rust types that map to ClickHouse types.
pub trait IntoBlockColumn {
    /// The type of the column data vector.
    type ColumnData;

    /// The type of individual values in the column.
    type ColumnValue;

    /// Convert a value to its column representation.
    ///
    /// This enables efficient column construction via `map().collect()`:
    /// ```ignore
    /// let col: Vec<_> = rows.iter().map(|r| r.field.to_column_value()).collect();
    /// ```
    fn to_column_value(&self) -> Self::ColumnValue;

    /// Convert an owned value to its column representation (avoids cloning).
    fn into_column_value(self) -> Self::ColumnValue
    where
        Self: Sized;

    /// Add this value to a column data vector.
    fn push_to_column(value: &Self, column: &mut Self::ColumnData);

    /// Create an empty column data vector.
    fn new_column() -> Self::ColumnData;

    /// Create a column data vector with pre-allocated capacity.
    ///
    /// This is more efficient when the number of rows is known upfront,
    /// as it avoids reallocations during insertion.
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData;

    /// Add the column to a block.
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block;
}

/// Trait for types that can be added as a column to a Block by taking ownership.
///
/// This is an optimization to avoid cloning for types like `String` and `Vec<T>`.
/// Use this trait when you have owned values and want to avoid unnecessary allocations.
///
/// # Example
///
/// ```rust,ignore
/// // With IntoBlockColumn (requires clone):
/// IntoBlockColumn::push_to_column(&my_string, &mut column);
///
/// // With IntoBlockColumnOwned (no clone, takes ownership):
/// IntoBlockColumnOwned::push_to_column_owned(my_string, &mut column);
/// ```
pub trait IntoBlockColumnOwned: IntoBlockColumn + Sized {
    /// Add this value to a column data vector, taking ownership.
    ///
    /// This avoids cloning for types like `String` and `Vec<T>`.
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData);
}

// Macro to implement IntoBlockColumn and IntoBlockColumnOwned for primitive Copy types
macro_rules! impl_into_block_column_primitive {
    ($($ty:ty),*) => {
        $(
            impl IntoBlockColumn for $ty {
                type ColumnData = Vec<$ty>;
                type ColumnValue = $ty;

                #[inline]
                fn to_column_value(&self) -> Self::ColumnValue {
                    *self
                }

                #[inline]
                fn into_column_value(self) -> Self::ColumnValue {
                    self
                }

                #[inline]
                fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
                    column.push(*value);
                }

                #[inline]
                fn new_column() -> Self::ColumnData {
                    Vec::new()
                }

                #[inline]
                fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
                    Vec::with_capacity(capacity)
                }

                #[inline]
                fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
                    block.column(name, data)
                }
            }

            impl IntoBlockColumnOwned for $ty {
                #[inline]
                fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
                    column.push(value);
                }
            }
        )*
    };
}

// Bool is natively supported by clickhouse-rs as Vec<bool>
impl_into_block_column_primitive!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, bool);

/// IntoBlockColumn implementation for `String`.
///
/// # Performance Note
///
/// Methods taking `&self` (`to_column_value`, `push_to_column`) require cloning
/// the String. For bulk INSERT operations with owned data, prefer:
///
/// - `conn.insert_native_owned(table, rows)` - takes ownership, avoids clones
/// - `ToNativeBlock::rows_into_block(rows)` - uses `IntoBlockColumnOwned` trait
///
/// The `IntoBlockColumnOwned` trait provides `push_to_column_owned` which
/// moves the String instead of cloning.
impl IntoBlockColumn for String {
    type ColumnData = Vec<String>;
    type ColumnValue = String;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        self.clone()
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        self
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(value.clone());
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumnOwned for String {
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        column.push(value);
    }
}

/// IntoBlockColumn implementation for `&str`.
///
/// # Performance Note
///
/// Each `&str` value requires allocation to create an owned `String` for the block.
/// This is unavoidable because ClickHouse's native protocol requires owned data.
///
/// For better performance with INSERT operations:
/// - Use `String` fields in your insert struct when you already have owned data
/// - Use [`IntoBlockColumnOwned::push_to_column_owned`] when moving owned strings
/// - Pre-allocate with [`IntoBlockColumn::new_column_with_capacity`] to avoid Vec reallocations
impl IntoBlockColumn for &str {
    type ColumnData = Vec<String>;
    type ColumnValue = String;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        String::from(*self)
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        String::from(self)
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(String::from(*value));
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

/// IntoBlockColumn implementation for `Cow<str>`.
///
/// This implementation is optimized to avoid allocation when the `Cow` is
/// already owned (`Cow::Owned`). Use this when you have mixed borrowed and
/// owned string data.
///
/// # Performance
///
/// - `Cow::Owned(s)` → moves `s` directly, no allocation
/// - `Cow::Borrowed(s)` → allocates a new `String`
impl IntoBlockColumn for Cow<'_, str> {
    type ColumnData = Vec<String>;
    type ColumnValue = String;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        // Use to_string() instead of clone().into_owned() to avoid
        // cloning the inner String for Cow::Owned variant
        self.to_string()
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        self.into_owned()
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        // Use to_string() instead of clone().into_owned() to avoid
        // cloning the inner String for Cow::Owned variant
        column.push(value.to_string());
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl IntoBlockColumnOwned for Cow<'_, str> {
    #[inline]
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        // Zero-allocation for Cow::Owned, one allocation for Cow::Borrowed
        column.push(value.into_owned());
    }
}

// Vec<T> for Array columns
impl<T: Clone> IntoBlockColumn for Vec<T>
where
    Vec<Vec<T>>: clickhouse_rs::types::column::ColumnFrom,
{
    type ColumnData = Vec<Vec<T>>;
    type ColumnValue = Vec<T>;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        self.clone()
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        self
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(value.clone());
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

impl<T: Clone> IntoBlockColumnOwned for Vec<T>
where
    Vec<Vec<T>>: clickhouse_rs::types::column::ColumnFrom,
{
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        column.push(value);
    }
}

// DateTime types - convert to the format clickhouse-rs expects
#[cfg(feature = "chrono")]
impl IntoBlockColumn for chrono::DateTime<chrono_tz::Tz> {
    type ColumnData = Vec<chrono::DateTime<chrono_tz::Tz>>;
    type ColumnValue = chrono::DateTime<chrono_tz::Tz>;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        *self
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        self
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(*value);
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumnOwned for chrono::DateTime<chrono_tz::Tz> {
    #[inline]
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        column.push(value);
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumn for chrono::DateTime<chrono::FixedOffset> {
    // clickhouse-rs expects DateTime<Tz>, so we convert
    type ColumnData = Vec<chrono::DateTime<chrono_tz::Tz>>;
    type ColumnValue = chrono::DateTime<chrono_tz::Tz>;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        use chrono::TimeZone;
        let utc = self.with_timezone(&chrono::Utc);
        chrono_tz::UTC.from_utc_datetime(&utc.naive_utc())
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        use chrono::TimeZone;
        let utc = self.with_timezone(&chrono::Utc);
        chrono_tz::UTC.from_utc_datetime(&utc.naive_utc())
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(value.to_column_value());
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumnOwned for chrono::DateTime<chrono::FixedOffset> {
    #[inline]
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        use chrono::TimeZone;
        let utc = value.with_timezone(&chrono::Utc);
        let tz_dt = chrono_tz::UTC.from_utc_datetime(&utc.naive_utc());
        column.push(tz_dt);
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumn for chrono::DateTime<chrono::Utc> {
    type ColumnData = Vec<chrono::DateTime<chrono_tz::Tz>>;
    type ColumnValue = chrono::DateTime<chrono_tz::Tz>;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        use chrono::TimeZone;
        chrono_tz::UTC.from_utc_datetime(&self.naive_utc())
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        use chrono::TimeZone;
        chrono_tz::UTC.from_utc_datetime(&self.naive_utc())
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(value.to_column_value());
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumnOwned for chrono::DateTime<chrono::Utc> {
    #[inline]
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        use chrono::TimeZone;
        let tz_dt = chrono_tz::UTC.from_utc_datetime(&value.naive_utc());
        column.push(tz_dt);
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumn for chrono::NaiveDateTime {
    type ColumnData = Vec<chrono::DateTime<chrono_tz::Tz>>;
    type ColumnValue = chrono::DateTime<chrono_tz::Tz>;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        use chrono::TimeZone;
        chrono_tz::UTC.from_utc_datetime(self)
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        use chrono::TimeZone;
        chrono_tz::UTC.from_utc_datetime(&self)
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(value.to_column_value());
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

#[cfg(feature = "chrono")]
impl IntoBlockColumnOwned for chrono::NaiveDateTime {
    #[inline]
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        use chrono::TimeZone;
        let tz_dt = chrono_tz::UTC.from_utc_datetime(&value);
        column.push(tz_dt);
    }
}

// =============================================================================
// JSON type support (ClickHouse 24.10+)
// =============================================================================

/// Serialize a JSON value to string, handling errors appropriately.
///
/// For `serde_json::Value`, serialization can only fail if the value contains
/// infinity or NaN numbers, which is impossible since `Value` doesn't accept them.
/// We handle the error case defensively but it should never occur in practice.
#[cfg(feature = "json")]
#[inline]
fn serialize_json_value(value: &serde_json::Value) -> String {
    // serde_json::Value serialization can only fail for infinity/NaN numbers,
    // which Value doesn't support. This match is defensive.
    match serde_json::to_string(value) {
        Ok(s) => s,
        Err(e) => {
            // This branch should be unreachable for valid serde_json::Value
            #[cfg(feature = "tracing")]
            tracing::error!(
                error = %e,
                "Failed to serialize serde_json::Value - this should not happen"
            );
            debug_assert!(false, "serde_json::Value serialization failed: {}", e);
            String::new()
        }
    }
}

/// IntoBlockColumn implementation for serde_json::Value.
///
/// JSON values are serialized to strings for insertion. ClickHouse reads them
/// back as JSON columns when the table schema defines the column as JSON type.
///
/// # Error Handling
///
/// Serialization of `serde_json::Value` should never fail since the type only
/// allows valid JSON values. In the extremely unlikely event of a failure,
/// an empty string is returned and an error is logged (if tracing is enabled).
#[cfg(feature = "json")]
impl IntoBlockColumn for serde_json::Value {
    type ColumnData = Vec<String>;
    type ColumnValue = String;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        serialize_json_value(self)
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        serialize_json_value(&self)
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(serialize_json_value(value));
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

#[cfg(feature = "json")]
impl IntoBlockColumnOwned for serde_json::Value {
    #[inline]
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        column.push(serialize_json_value(&value));
    }
}

/// Serialize a typed JSON value to string, handling errors appropriately.
///
/// Unlike `serde_json::Value`, typed JSON serialization can fail if the
/// `Serialize` implementation of `T` returns an error. Errors are logged
/// (if tracing is enabled) and trigger a debug assertion.
#[cfg(feature = "json")]
#[inline]
fn serialize_json_typed<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_string(value) {
        Ok(s) => s,
        Err(e) => {
            // Log the error for visibility - this indicates a bug in the Serialize impl
            #[cfg(feature = "tracing")]
            tracing::error!(
                error = %e,
                type_name = std::any::type_name::<T>(),
                "Failed to serialize JsonTyped value - check the Serialize implementation"
            );
            debug_assert!(
                false,
                "JsonTyped<{}> serialization failed: {}. \
                 This indicates a bug in the Serialize implementation.",
                std::any::type_name::<T>(),
                e
            );
            String::new()
        }
    }
}

/// IntoBlockColumn implementation for JsonTyped<T>.
///
/// Typed JSON values are serialized to strings for insertion.
///
/// # Error Handling
///
/// Serialization can fail if `T`'s `Serialize` implementation returns an error.
/// In such cases:
/// - A debug assertion is triggered (panics in debug builds)
/// - An error is logged if the `tracing` feature is enabled
/// - An empty string is inserted as a fallback
///
/// This behavior ensures that production systems don't crash on serialization
/// errors, while making bugs visible during development.
#[cfg(feature = "json")]
impl<T: serde::Serialize + Clone> IntoBlockColumn for diesel_clickhouse_types::JsonTyped<T> {
    type ColumnData = Vec<String>;
    type ColumnValue = String;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        serialize_json_typed(&self.0)
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        serialize_json_typed(&self.0)
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(serialize_json_typed(&value.0));
    }

    #[inline]
    fn new_column() -> Self::ColumnData {
        Vec::new()
    }

    #[inline]
    fn new_column_with_capacity(capacity: usize) -> Self::ColumnData {
        Vec::with_capacity(capacity)
    }

    #[inline]
    fn add_column_to_block(block: Block, name: &str, data: Self::ColumnData) -> Block {
        block.column(name, data)
    }
}

#[cfg(feature = "json")]
impl<T: serde::Serialize + Clone> IntoBlockColumnOwned for diesel_clickhouse_types::JsonTyped<T> {
    #[inline]
    fn push_to_column_owned(value: Self, column: &mut Self::ColumnData) {
        column.push(serialize_json_typed(&value.0));
    }
}
