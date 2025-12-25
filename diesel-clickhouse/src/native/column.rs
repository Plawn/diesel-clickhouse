//! Block column serialization traits for INSERT operations.

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

impl IntoBlockColumn for &str {
    type ColumnData = Vec<String>;
    type ColumnValue = String;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        (*self).to_string()
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        self.to_string()
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push((*value).to_string());
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

/// IntoBlockColumn implementation for serde_json::Value.
///
/// JSON values are serialized to strings for insertion. ClickHouse reads them
/// back as JSON columns when the table schema defines the column as JSON type.
#[cfg(feature = "json")]
impl IntoBlockColumn for serde_json::Value {
    type ColumnData = Vec<String>;
    type ColumnValue = String;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        // Use compact serialization; unwrap_or_default handles edge cases
        serde_json::to_string(self).unwrap_or_default()
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        serde_json::to_string(&self).unwrap_or_default()
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(serde_json::to_string(value).unwrap_or_default());
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
        column.push(serde_json::to_string(&value).unwrap_or_default());
    }
}

/// IntoBlockColumn implementation for JsonTyped<T>.
///
/// Typed JSON values are serialized to strings for insertion.
#[cfg(feature = "json")]
impl<T: serde::Serialize + Clone> IntoBlockColumn for diesel_clickhouse_types::JsonTyped<T> {
    type ColumnData = Vec<String>;
    type ColumnValue = String;

    #[inline]
    fn to_column_value(&self) -> Self::ColumnValue {
        serde_json::to_string(&self.0).unwrap_or_default()
    }

    #[inline]
    fn into_column_value(self) -> Self::ColumnValue {
        serde_json::to_string(&self.0).unwrap_or_default()
    }

    #[inline]
    fn push_to_column(value: &Self, column: &mut Self::ColumnData) {
        column.push(serde_json::to_string(&value.0).unwrap_or_default());
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
        column.push(serde_json::to_string(&value.0).unwrap_or_default());
    }
}
