//! Nullable type wrapper for ClickHouse.

use std::marker::PhantomData;

use crate::{SqlType, HasSqlType, FromClickHouse, ToClickHouse, DeserializeError, SerializeError};

/// ClickHouse Nullable(T) type.
///
/// Wraps any type to allow NULL values. Note that Nullable columns have
/// a 2-3x performance overhead compared to non-nullable columns.
///
/// # Important
///
/// Not all types can be made Nullable:
/// - Array, Map, Tuple, Nested cannot be directly Nullable
/// - However, their elements can be Nullable
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Nullable<T: SqlType>(PhantomData<T>);

impl<T: SqlType> SqlType for Nullable<T> {
    fn type_name() -> &'static str { "Nullable" }
}

impl<T, S: SqlType> HasSqlType for Option<T>
where
    T: HasSqlType<SqlType = S>,
    S: SqlType,
{
    type SqlType = Nullable<S>;
}

impl<T, S: SqlType> FromClickHouse<Nullable<S>> for Option<T>
where
    T: FromClickHouse<S>,
{
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        // In ClickHouse binary format, Nullable values are prefixed with a null flag
        // 1 = NULL, 0 = non-NULL
        if value.is_empty() {
            return Ok(None);
        }

        if value[0] == 1 {
            Ok(None)
        } else {
            T::from_clickhouse(&value[1..]).map(Some)
        }
    }
}

impl<T, S: SqlType> ToClickHouse<Nullable<S>> for Option<T>
where
    T: ToClickHouse<S>,
{
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        match self {
            None => {
                out.push(1); // NULL flag
                Ok(())
            }
            Some(value) => {
                out.push(0); // NOT NULL flag
                value.to_clickhouse(out)
            }
        }
    }
}

/// Trait for expressions that can be made nullable.
pub trait IntoNullable {
    /// The nullable version of this type.
    type Nullable;

    /// Convert to nullable.
    fn nullable(self) -> Self::Nullable;
}

/// Trait for nullable expressions that can be assumed non-null.
///
/// # Warning
///
/// Using `assume_not_null()` on a NULL value will cause a panic or
/// undefined behavior. Only use this when you're certain the value
/// is not NULL.
pub trait AssumeNotNull {
    /// The non-nullable version of this type.
    type NotNull;

    /// Assume the value is not null.
    ///
    /// # Panics
    ///
    /// May panic if the value is actually NULL.
    fn assume_not_null(self) -> Self::NotNull;
}

impl<T> IntoNullable for T
where
    T: HasSqlType,
    T::SqlType: SqlType,
{
    type Nullable = Option<T>;

    fn nullable(self) -> Self::Nullable {
        Some(self)
    }
}

impl<T> AssumeNotNull for Option<T> {
    type NotNull = T;

    fn assume_not_null(self) -> Self::NotNull {
        self.expect("assume_not_null called on NULL value")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UInt32;

    #[test]
    fn test_nullable_some_roundtrip() {
        let mut buf = Vec::new();
        let value: Option<u32> = Some(42);
        <Option<u32> as ToClickHouse<Nullable<UInt32>>>::to_clickhouse(&value, &mut buf).unwrap();

        // Should be: [0 (not null), 42, 0, 0, 0] for u32 LE
        assert_eq!(buf[0], 0);

        let result = <Option<u32> as FromClickHouse<Nullable<UInt32>>>::from_clickhouse(&buf).unwrap();
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_nullable_none_roundtrip() {
        let mut buf = Vec::new();
        let value: Option<u32> = None;
        <Option<u32> as ToClickHouse<Nullable<UInt32>>>::to_clickhouse(&value, &mut buf).unwrap();

        // Should be: [1 (null)]
        assert_eq!(buf, vec![1]);

        let result = <Option<u32> as FromClickHouse<Nullable<UInt32>>>::from_clickhouse(&buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_into_nullable() {
        let value: u32 = 42;
        let nullable = value.nullable();
        assert_eq!(nullable, Some(42u32));
    }

    #[test]
    fn test_assume_not_null() {
        let value: Option<u32> = Some(42);
        assert_eq!(value.assume_not_null(), 42);
    }

    #[test]
    #[should_panic(expected = "assume_not_null called on NULL value")]
    fn test_assume_not_null_panics() {
        let value: Option<u32> = None;
        value.assume_not_null();
    }
}
