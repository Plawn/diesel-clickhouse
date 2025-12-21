// Deny unwrap/expect in library code to prevent panics
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow in tests
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]

//! SQL type system for diesel-clickhouse
//!
//! This crate provides the type system that maps ClickHouse SQL types to Rust types.
//! Each ClickHouse type has a corresponding marker type that is used for compile-time
//! type checking.

use std::fmt;

pub mod integers;
pub mod floats;
pub mod strings;
pub mod temporal;
pub mod complex;
pub mod nullable;

// Re-export all types
pub use integers::*;
pub use floats::*;
pub use strings::*;
pub use temporal::*;
pub use complex::*;
pub use nullable::*;

/// Marker trait for all SQL types.
///
/// This trait is implemented by all ClickHouse SQL type markers and provides
/// compile-time type information for the query builder.
pub trait SqlType: 'static + Send + Sync {
    /// The name of this type as it appears in ClickHouse SQL.
    fn type_name() -> &'static str;

    /// Whether this type can be nullable.
    /// Some types like Array cannot directly be Nullable.
    const NULLABLE_ALLOWED: bool = true;
}

/// Trait for types that have a corresponding ClickHouse type.
pub trait HasSqlType {
    /// The ClickHouse SQL type marker.
    type SqlType: SqlType;
}

/// Trait for deserializing values from ClickHouse.
pub trait FromClickHouse<ST: SqlType>: Sized {
    /// Deserialize from a raw ClickHouse value.
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError>;
}

/// Trait for serializing values to ClickHouse format.
pub trait ToClickHouse<ST: SqlType> {
    /// Serialize to ClickHouse format.
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError>;
}

/// Error during deserialization from ClickHouse.
#[derive(Debug, thiserror::Error)]
pub enum DeserializeError {
    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Null value for non-nullable type")]
    UnexpectedNull,

    #[error("UTF-8 error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Error during serialization to ClickHouse.
#[derive(Debug, thiserror::Error)]
pub enum SerializeError {
    #[error("Invalid value: {0}")]
    InvalidValue(String),

    #[error("Value out of range for type {type_name}: {value}")]
    OutOfRange { type_name: String, value: String },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Type metadata for runtime type information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMetadata {
    /// The ClickHouse type name.
    pub name: String,
    /// Whether this type is nullable.
    pub nullable: bool,
    /// Nested type parameters (for Array, Map, etc.).
    pub parameters: Vec<TypeMetadata>,
}

impl TypeMetadata {
    /// Create metadata for a simple type.
    pub fn simple(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nullable: false,
            parameters: Vec::new(),
        }
    }

    /// Create metadata for a nullable type.
    pub fn nullable(inner: TypeMetadata) -> Self {
        Self {
            name: format!("Nullable({})", inner.name),
            nullable: true,
            parameters: vec![inner],
        }
    }

    /// Create metadata for a parameterized type.
    pub fn parameterized(name: impl Into<String>, params: Vec<TypeMetadata>) -> Self {
        let name = name.into();
        let params_str = params.iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Self {
            name: format!("{}({})", name, params_str),
            nullable: false,
            parameters: params,
        }
    }
}

impl fmt::Display for TypeMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

// =============================================================================
// Tuple SqlType implementations
// =============================================================================

macro_rules! impl_sql_type_tuple {
    ($(($idx:tt, $T:ident)),+) => {
        impl<$($T: SqlType),+> SqlType for ($($T,)+) {
            fn type_name() -> &'static str {
                "Tuple"
            }
        }
    };
}

impl_sql_type_tuple!((0, A));
impl_sql_type_tuple!((0, A), (1, B));
impl_sql_type_tuple!((0, A), (1, B), (2, C));
impl_sql_type_tuple!((0, A), (1, B), (2, C), (3, D));
impl_sql_type_tuple!((0, A), (1, B), (2, C), (3, D), (4, E));
impl_sql_type_tuple!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F));
impl_sql_type_tuple!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G));
impl_sql_type_tuple!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G), (7, H));
