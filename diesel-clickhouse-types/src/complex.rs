//! Complex SQL types for ClickHouse (Array, Map, Tuple, etc.).

use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;

use crate::{SqlType, HasSqlType, FromClickHouse, ToClickHouse, DeserializeError, SerializeError};

// =============================================================================
// Array type
// =============================================================================

/// ClickHouse Array(T) type.
///
/// Arrays in ClickHouse are 1-indexed and support negative indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Array<T: SqlType>(PhantomData<T>);

impl<T: SqlType> SqlType for Array<T> {
    fn type_name() -> &'static str { "Array" }
    const NULLABLE_ALLOWED: bool = false; // Array cannot be directly Nullable
}

impl<T, U: SqlType> HasSqlType for Vec<T>
where
    T: HasSqlType<SqlType = U>,
{
    type SqlType = Array<U>;
}

// Note: Full Array serialization requires element type knowledge
// This is a simplified implementation

// =============================================================================
// Tuple type
// =============================================================================

/// ClickHouse Tuple type.
///
/// Tuples can have named or positional fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tuple<T>(PhantomData<T>);

impl<T: 'static + Send + Sync> SqlType for Tuple<T> {
    fn type_name() -> &'static str { "Tuple" }
    const NULLABLE_ALLOWED: bool = false;
}

// Tuple implementations for common arities
impl<A: HasSqlType, B: HasSqlType> HasSqlType for (A, B) {
    type SqlType = Tuple<(A::SqlType, B::SqlType)>;
}

impl<A: HasSqlType, B: HasSqlType, C: HasSqlType> HasSqlType for (A, B, C) {
    type SqlType = Tuple<(A::SqlType, B::SqlType, C::SqlType)>;
}

impl<A: HasSqlType, B: HasSqlType, C: HasSqlType, D: HasSqlType> HasSqlType for (A, B, C, D) {
    type SqlType = Tuple<(A::SqlType, B::SqlType, C::SqlType, D::SqlType)>;
}

// =============================================================================
// Map type
// =============================================================================

/// ClickHouse Map(K, V) type.
///
/// Stores key-value pairs. Keys must be String, integer, or other comparable types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Map<K: SqlType, V: SqlType>(PhantomData<(K, V)>);

impl<K: SqlType, V: SqlType> SqlType for Map<K, V> {
    fn type_name() -> &'static str { "Map" }
    const NULLABLE_ALLOWED: bool = false;
}

impl<K, V, KS: SqlType, VS: SqlType> HasSqlType for HashMap<K, V>
where
    K: HasSqlType<SqlType = KS> + Eq + Hash,
    V: HasSqlType<SqlType = VS>,
{
    type SqlType = Map<KS, VS>;
}

// =============================================================================
// Nested type
// =============================================================================

/// ClickHouse Nested type.
///
/// Nested is essentially an array of named tuples, represented as parallel arrays
/// for each field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Nested<T>(PhantomData<T>);

impl<T: 'static + Send + Sync> SqlType for Nested<T> {
    fn type_name() -> &'static str { "Nested" }
    const NULLABLE_ALLOWED: bool = false;
}

// =============================================================================
// LowCardinality type
// =============================================================================

/// ClickHouse LowCardinality(T) type.
///
/// Dictionary-encoded storage for columns with low unique value count.
/// Best used when unique values are <10,000 and ideally <1% of total rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LowCardinality<T: SqlType>(PhantomData<T>);

impl<T: SqlType> SqlType for LowCardinality<T> {
    fn type_name() -> &'static str { "LowCardinality" }
}

// LowCardinality is transparent at the Rust level
// The inner type determines the Rust mapping
impl<T, S: SqlType> FromClickHouse<LowCardinality<S>> for T
where
    T: FromClickHouse<S>,
{
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        T::from_clickhouse(value)
    }
}

impl<T, S: SqlType> ToClickHouse<LowCardinality<S>> for T
where
    T: ToClickHouse<S>,
{
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        T::to_clickhouse(self, out)
    }
}

// =============================================================================
// Enum types
// =============================================================================

/// ClickHouse Enum8 type (up to 256 values, 1 byte storage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Enum8;

impl SqlType for Enum8 {
    fn type_name() -> &'static str { "Enum8" }
}

/// ClickHouse Enum16 type (up to 65,536 values, 2 bytes storage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Enum16;

impl SqlType for Enum16 {
    fn type_name() -> &'static str { "Enum16" }
}

// =============================================================================
// JSON type (ClickHouse 24.10+)
// =============================================================================

/// ClickHouse JSON type (ClickHouse 24.10+).
///
/// This type supports the new native JSON type in ClickHouse which stores
/// JSON as typed subcolumns with efficient binary storage.
///
/// # Serialization
///
/// JSON is serialized as a String using ClickHouse settings:
/// - HTTP: `output_format_binary_write_json_as_string=1`
/// - Native: `output_format_native_write_json_as_string=1`
///
/// This approach is recommended by ClickHouse for non-C++ clients due to
/// TypeId instability in the native binary format.
///
/// # Usage
///
/// Two approaches are supported:
///
/// 1. **Untyped**: Use `serde_json::Value` for flexible JSON handling
/// 2. **Typed**: Use `JsonTyped<T>` wrapper for compile-time type safety
///
/// ```rust,ignore
/// use diesel_clickhouse::types::{Json, JsonTyped};
///
/// // Untyped - flexible JSON
/// #[derive(ClickHouseRow)]
/// struct Event {
///     id: u64,
///     data: serde_json::Value,  // Any JSON
/// }
///
/// // Typed - compile-time checked
/// #[derive(Serialize, Deserialize)]
/// struct UserPrefs {
///     theme: String,
///     notifications: bool,
/// }
///
/// #[derive(ClickHouseRow)]
/// struct User {
///     id: u64,
///     preferences: JsonTyped<UserPrefs>,
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Json;

impl SqlType for Json {
    fn type_name() -> &'static str { "JSON" }
}

/// Generic typed JSON wrapper for custom types.
///
/// Use this when you want to deserialize JSON columns directly to Rust structs.
/// The inner type must implement `Serialize` and `DeserializeOwned`.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::types::JsonTyped;
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Debug, Serialize, Deserialize)]
/// struct UserPrefs {
///     theme: String,
///     notifications: bool,
/// }
///
/// #[derive(ClickHouseRow)]
/// struct User {
///     id: u64,
///     preferences: JsonTyped<UserPrefs>,
/// }
///
/// // Access the inner value
/// let user: User = conn.first(users::table).await?;
/// println!("Theme: {}", user.preferences.theme);
/// ```
#[cfg(feature = "json")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonTyped<T>(pub T);

#[cfg(feature = "json")]
impl<T> JsonTyped<T> {
    /// Create a new JsonTyped wrapper.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Consume self and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

#[cfg(feature = "json")]
impl<T> std::ops::Deref for JsonTyped<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[cfg(feature = "json")]
impl<T> std::ops::DerefMut for JsonTyped<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

#[cfg(feature = "json")]
impl<T> From<T> for JsonTyped<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

// HasSqlType implementations for JSON types
#[cfg(feature = "json")]
impl HasSqlType for serde_json::Value {
    type SqlType = Json;
}

#[cfg(feature = "json")]
impl<T: 'static + Send + Sync> HasSqlType for JsonTyped<T> {
    type SqlType = Json;
}

// Serde implementations for JsonTyped
//
// IMPORTANT: For RowBinary format used by the clickhouse crate, JSON columns
// must be serialized as a JSON string, not as a nested struct. This ensures
// the value is treated as a single column rather than flattening struct fields.
#[cfg(feature = "json")]
impl<T: serde::Serialize> serde::Serialize for JsonTyped<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize the inner value to a JSON string
        let json_string = serde_json::to_string(&self.0)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&json_string)
    }
}

#[cfg(feature = "json")]
impl<'de, T: serde::de::DeserializeOwned> serde::Deserialize<'de> for JsonTyped<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize from a JSON string
        let json_string = String::deserialize(deserializer)?;
        let value = serde_json::from_str(&json_string)
            .map_err(serde::de::Error::custom)?;
        Ok(JsonTyped(value))
    }
}

// =============================================================================
// SimpleAggregateFunction and AggregateFunction
// =============================================================================

/// ClickHouse SimpleAggregateFunction type.
///
/// Used in AggregatingMergeTree for pre-aggregated columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimpleAggregateFunction<F, T: SqlType>(PhantomData<(F, T)>);

impl<F: 'static + Send + Sync, T: SqlType> SqlType for SimpleAggregateFunction<F, T> {
    fn type_name() -> &'static str { "SimpleAggregateFunction" }
}

/// ClickHouse AggregateFunction type.
///
/// Used in AggregatingMergeTree for complex aggregation states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AggregateFunction<F, T: SqlType>(PhantomData<(F, T)>);

impl<F: 'static + Send + Sync, T: SqlType> SqlType for AggregateFunction<F, T> {
    fn type_name() -> &'static str { "AggregateFunction" }
}

// =============================================================================
// Aggregate function markers
// =============================================================================

/// Marker for sum() aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Sum;

/// Marker for count() aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Count;

/// Marker for min() aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Min;

/// Marker for max() aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Max;

/// Marker for avg() aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Avg;

/// Marker for any() aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct Any;

/// Marker for anyLast() aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct AnyLast;

/// Marker for groupArray() aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct GroupArray;

/// Marker for groupUniqArray() aggregate function.
#[derive(Debug, Clone, Copy)]
pub struct GroupUniqArray;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CHString;

    #[test]
    fn test_type_names() {
        assert_eq!(Array::<crate::UInt64>::type_name(), "Array");
        assert_eq!(Map::<CHString, crate::UInt64>::type_name(), "Map");
        assert_eq!(LowCardinality::<CHString>::type_name(), "LowCardinality");
        assert_eq!(Tuple::<(crate::UInt64, CHString)>::type_name(), "Tuple");
    }
}
