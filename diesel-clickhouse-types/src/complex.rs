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
// JSON type (experimental in ClickHouse)
// =============================================================================

/// ClickHouse JSON type (experimental).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Json;

impl SqlType for Json {
    fn type_name() -> &'static str { "JSON" }
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
