//! Integer SQL types for ClickHouse.
//!
//! ClickHouse supports a wide range of integer types from 8-bit to 256-bit,
//! both signed and unsigned.

use crate::{SqlType, HasSqlType, FromClickHouse, ToClickHouse, DeserializeError, SerializeError};

// =============================================================================
// Unsigned Integer Types
// =============================================================================

/// ClickHouse UInt8 type (0 to 255).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UInt8;

impl SqlType for UInt8 {
    fn type_name() -> &'static str { "UInt8" }
}

/// ClickHouse UInt16 type (0 to 65,535).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UInt16;

impl SqlType for UInt16 {
    fn type_name() -> &'static str { "UInt16" }
}

/// ClickHouse UInt32 type (0 to 4,294,967,295).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UInt32;

impl SqlType for UInt32 {
    fn type_name() -> &'static str { "UInt32" }
}

/// ClickHouse UInt64 type (0 to 18,446,744,073,709,551,615).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UInt64;

impl SqlType for UInt64 {
    fn type_name() -> &'static str { "UInt64" }
}

/// ClickHouse UInt128 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UInt128;

impl SqlType for UInt128 {
    fn type_name() -> &'static str { "UInt128" }
}

/// ClickHouse UInt256 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UInt256;

impl SqlType for UInt256 {
    fn type_name() -> &'static str { "UInt256" }
}

// =============================================================================
// Signed Integer Types
// =============================================================================

/// ClickHouse Int8 type (-128 to 127).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Int8;

impl SqlType for Int8 {
    fn type_name() -> &'static str { "Int8" }
}

/// ClickHouse Int16 type (-32,768 to 32,767).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Int16;

impl SqlType for Int16 {
    fn type_name() -> &'static str { "Int16" }
}

/// ClickHouse Int32 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Int32;

impl SqlType for Int32 {
    fn type_name() -> &'static str { "Int32" }
}

/// ClickHouse Int64 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Int64;

impl SqlType for Int64 {
    fn type_name() -> &'static str { "Int64" }
}

/// ClickHouse Int128 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Int128;

impl SqlType for Int128 {
    fn type_name() -> &'static str { "Int128" }
}

/// ClickHouse Int256 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Int256;

impl SqlType for Int256 {
    fn type_name() -> &'static str { "Int256" }
}

// =============================================================================
// Rust type mappings
// =============================================================================

impl HasSqlType for u8 {
    type SqlType = UInt8;
}

impl HasSqlType for u16 {
    type SqlType = UInt16;
}

impl HasSqlType for u32 {
    type SqlType = UInt32;
}

impl HasSqlType for u64 {
    type SqlType = UInt64;
}

impl HasSqlType for u128 {
    type SqlType = UInt128;
}

impl HasSqlType for i8 {
    type SqlType = Int8;
}

impl HasSqlType for i16 {
    type SqlType = Int16;
}

impl HasSqlType for i32 {
    type SqlType = Int32;
}

impl HasSqlType for i64 {
    type SqlType = Int64;
}

impl HasSqlType for i128 {
    type SqlType = Int128;
}

// =============================================================================
// 256-bit integer wrapper types
// =============================================================================

/// Wrapper for ClickHouse UInt256 values.
///
/// Stored as 4 u64 values in little-endian order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct U256(pub [u64; 4]);

impl U256 {
    /// Create a new U256 from a u128 value.
    pub fn from_u128(value: u128) -> Self {
        Self([
            value as u64,
            (value >> 64) as u64,
            0,
            0,
        ])
    }

    /// Try to convert to u128, returning None if the value is too large.
    pub fn to_u128(&self) -> Option<u128> {
        if self.0[2] == 0 && self.0[3] == 0 {
            Some((self.0[1] as u128) << 64 | self.0[0] as u128)
        } else {
            None
        }
    }
}

impl HasSqlType for U256 {
    type SqlType = UInt256;
}

/// Wrapper for ClickHouse Int256 values.
///
/// Stored as 4 u64 values in little-endian order (two's complement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct I256(pub [u64; 4]);

impl I256 {
    /// Create a new I256 from an i128 value.
    pub fn from_i128(value: i128) -> Self {
        let sign_extend = if value < 0 { u64::MAX } else { 0 };
        Self([
            value as u64,
            (value >> 64) as u64,
            sign_extend,
            sign_extend,
        ])
    }

    /// Try to convert to i128, returning None if the value is out of range.
    pub fn to_i128(&self) -> Option<i128> {
        // Check if value fits in i128
        let sign_bit = (self.0[3] >> 63) == 1;
        let expected_extend = if sign_bit { u64::MAX } else { 0 };

        if self.0[2] == expected_extend && self.0[3] == expected_extend {
            Some(((self.0[1] as i128) << 64) | self.0[0] as i128)
        } else {
            None
        }
    }
}

impl HasSqlType for I256 {
    type SqlType = Int256;
}

// =============================================================================
// FromClickHouse / ToClickHouse implementations
// =============================================================================

macro_rules! impl_integer_conversion {
    ($rust_ty:ty, $sql_ty:ty) => {
        impl FromClickHouse<$sql_ty> for $rust_ty {
            fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
                if value.len() != std::mem::size_of::<$rust_ty>() {
                    return Err(DeserializeError::InvalidData(
                        format!("Expected {} bytes, got {}", std::mem::size_of::<$rust_ty>(), value.len())
                    ));
                }
                let bytes: [u8; std::mem::size_of::<$rust_ty>()] = value.try_into()
                    .map_err(|_| DeserializeError::InvalidData("Invalid byte length".into()))?;
                Ok(<$rust_ty>::from_le_bytes(bytes))
            }
        }

        impl ToClickHouse<$sql_ty> for $rust_ty {
            fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
                out.extend_from_slice(&self.to_le_bytes());
                Ok(())
            }
        }
    };
}

impl_integer_conversion!(u8, UInt8);
impl_integer_conversion!(u16, UInt16);
impl_integer_conversion!(u32, UInt32);
impl_integer_conversion!(u64, UInt64);
impl_integer_conversion!(u128, UInt128);
impl_integer_conversion!(i8, Int8);
impl_integer_conversion!(i16, Int16);
impl_integer_conversion!(i32, Int32);
impl_integer_conversion!(i64, Int64);
impl_integer_conversion!(i128, Int128);

impl FromClickHouse<UInt256> for U256 {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != 32 {
            return Err(DeserializeError::InvalidData(
                format!("Expected 32 bytes for UInt256, got {}", value.len())
            ));
        }
        let mut result = [0u64; 4];
        for i in 0..4 {
            let bytes: [u8; 8] = value[i*8..(i+1)*8].try_into()
                .map_err(|_| DeserializeError::InvalidData("Invalid byte slice".into()))?;
            result[i] = u64::from_le_bytes(bytes);
        }
        Ok(U256(result))
    }
}

impl ToClickHouse<UInt256> for U256 {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        for val in &self.0 {
            out.extend_from_slice(&val.to_le_bytes());
        }
        Ok(())
    }
}

impl FromClickHouse<Int256> for I256 {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != 32 {
            return Err(DeserializeError::InvalidData(
                format!("Expected 32 bytes for Int256, got {}", value.len())
            ));
        }
        let mut result = [0u64; 4];
        for i in 0..4 {
            let bytes: [u8; 8] = value[i*8..(i+1)*8].try_into()
                .map_err(|_| DeserializeError::InvalidData("Invalid byte slice".into()))?;
            result[i] = u64::from_le_bytes(bytes);
        }
        Ok(I256(result))
    }
}

impl ToClickHouse<Int256> for I256 {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        for val in &self.0 {
            out.extend_from_slice(&val.to_le_bytes());
        }
        Ok(())
    }
}

// =============================================================================
// Boolean type (stored as UInt8 in ClickHouse)
// =============================================================================

/// ClickHouse Bool type (alias for UInt8, stored as 0 or 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bool;

impl SqlType for Bool {
    fn type_name() -> &'static str { "Bool" }
}

impl HasSqlType for bool {
    type SqlType = Bool;
}

impl FromClickHouse<Bool> for bool {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != 1 {
            return Err(DeserializeError::InvalidData(
                format!("Expected 1 byte for Bool, got {}", value.len())
            ));
        }
        Ok(value[0] != 0)
    }
}

impl ToClickHouse<Bool> for bool {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        out.push(if *self { 1 } else { 0 });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u256_from_u128() {
        let val = U256::from_u128(u128::MAX);
        assert_eq!(val.to_u128(), Some(u128::MAX));
    }

    #[test]
    fn test_i256_from_i128() {
        let val = I256::from_i128(-1);
        assert_eq!(val.to_i128(), Some(-1));

        let val = I256::from_i128(i128::MIN);
        assert_eq!(val.to_i128(), Some(i128::MIN));
    }

    #[test]
    fn test_bool_roundtrip() {
        let mut buf = Vec::new();
        <bool as ToClickHouse<Bool>>::to_clickhouse(&true, &mut buf).unwrap();
        assert_eq!(<bool as FromClickHouse<Bool>>::from_clickhouse(&buf).unwrap(), true);

        buf.clear();
        <bool as ToClickHouse<Bool>>::to_clickhouse(&false, &mut buf).unwrap();
        assert_eq!(<bool as FromClickHouse<Bool>>::from_clickhouse(&buf).unwrap(), false);
    }
}
