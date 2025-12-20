//! Floating-point SQL types for ClickHouse.

use crate::{SqlType, HasSqlType, FromClickHouse, ToClickHouse, DeserializeError, SerializeError};

/// ClickHouse Float32 type (IEEE 754 single precision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Float32;

impl SqlType for Float32 {
    fn type_name() -> &'static str { "Float32" }
}

/// ClickHouse Float64 type (IEEE 754 double precision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Float64;

impl SqlType for Float64 {
    fn type_name() -> &'static str { "Float64" }
}

// =============================================================================
// Rust type mappings
// =============================================================================

impl HasSqlType for f32 {
    type SqlType = Float32;
}

impl HasSqlType for f64 {
    type SqlType = Float64;
}

// =============================================================================
// FromClickHouse / ToClickHouse implementations
// =============================================================================

impl FromClickHouse<Float32> for f32 {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != 4 {
            return Err(DeserializeError::InvalidData(
                format!("Expected 4 bytes for Float32, got {}", value.len())
            ));
        }
        let bytes: [u8; 4] = value.try_into()
            .map_err(|_| DeserializeError::InvalidData("Invalid byte length".into()))?;
        Ok(f32::from_le_bytes(bytes))
    }
}

impl ToClickHouse<Float32> for f32 {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        out.extend_from_slice(&self.to_le_bytes());
        Ok(())
    }
}

impl FromClickHouse<Float64> for f64 {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != 8 {
            return Err(DeserializeError::InvalidData(
                format!("Expected 8 bytes for Float64, got {}", value.len())
            ));
        }
        let bytes: [u8; 8] = value.try_into()
            .map_err(|_| DeserializeError::InvalidData("Invalid byte length".into()))?;
        Ok(f64::from_le_bytes(bytes))
    }
}

impl ToClickHouse<Float64> for f64 {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        out.extend_from_slice(&self.to_le_bytes());
        Ok(())
    }
}

// =============================================================================
// Decimal types
// =============================================================================

/// ClickHouse Decimal32 type with precision P and scale S.
///
/// Decimal32(S) can store values with up to 9 digits total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decimal32<const S: u8>;

impl<const S: u8> SqlType for Decimal32<S> {
    fn type_name() -> &'static str { "Decimal32" }
}

/// ClickHouse Decimal64 type with scale S.
///
/// Decimal64(S) can store values with up to 18 digits total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decimal64<const S: u8>;

impl<const S: u8> SqlType for Decimal64<S> {
    fn type_name() -> &'static str { "Decimal64" }
}

/// ClickHouse Decimal128 type with scale S.
///
/// Decimal128(S) can store values with up to 38 digits total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decimal128<const S: u8>;

impl<const S: u8> SqlType for Decimal128<S> {
    fn type_name() -> &'static str { "Decimal128" }
}

/// ClickHouse Decimal256 type with scale S.
///
/// Decimal256(S) can store values with up to 76 digits total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decimal256<const S: u8>;

impl<const S: u8> SqlType for Decimal256<S> {
    fn type_name() -> &'static str { "Decimal256" }
}

/// Rust wrapper for Decimal values.
///
/// Stores the unscaled value as i128 along with the scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decimal {
    /// The unscaled integer value.
    pub value: i128,
    /// The number of decimal places.
    pub scale: u8,
}

impl Decimal {
    /// Create a new Decimal with the given value and scale.
    pub fn new(value: i128, scale: u8) -> Self {
        Self { value, scale }
    }

    /// Create a Decimal from a floating-point value.
    pub fn from_f64(value: f64, scale: u8) -> Self {
        let factor = 10i128.pow(scale as u32);
        let scaled = (value * factor as f64).round() as i128;
        Self { value: scaled, scale }
    }

    /// Convert to f64.
    pub fn to_f64(&self) -> f64 {
        let factor = 10f64.powi(self.scale as i32);
        self.value as f64 / factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_float_roundtrip() {
        let mut buf = Vec::new();

        let f32_val: f32 = 3.14159;
        <f32 as ToClickHouse<Float32>>::to_clickhouse(&f32_val, &mut buf).unwrap();
        let result = <f32 as FromClickHouse<Float32>>::from_clickhouse(&buf).unwrap();
        assert!((result - f32_val).abs() < f32::EPSILON);

        buf.clear();

        let f64_val: f64 = 3.141592653589793;
        <f64 as ToClickHouse<Float64>>::to_clickhouse(&f64_val, &mut buf).unwrap();
        let result = <f64 as FromClickHouse<Float64>>::from_clickhouse(&buf).unwrap();
        assert!((result - f64_val).abs() < f64::EPSILON);
    }

    #[test]
    fn test_decimal() {
        let dec = Decimal::from_f64(123.45, 2);
        assert_eq!(dec.value, 12345);
        assert_eq!(dec.scale, 2);
        assert!((dec.to_f64() - 123.45).abs() < 0.001);
    }
}
