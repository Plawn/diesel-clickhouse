//! String SQL types for ClickHouse.

use crate::{SqlType, HasSqlType, FromClickHouse, ToClickHouse, DeserializeError, SerializeError};

/// ClickHouse String type (variable-length UTF-8 string).
///
/// Named `CHString` to avoid conflict with `std::string::String`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CHString;

impl SqlType for CHString {
    fn type_name() -> &'static str { "String" }
}

/// ClickHouse FixedString(N) type (fixed-length byte string).
///
/// Always stores exactly N bytes, padded with null bytes if necessary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedString<const N: usize>;

impl<const N: usize> SqlType for FixedString<N> {
    fn type_name() -> &'static str { "FixedString" }
}

// =============================================================================
// Rust type mappings
// =============================================================================

impl HasSqlType for String {
    type SqlType = CHString;
}

impl HasSqlType for &str {
    type SqlType = CHString;
}

impl<const N: usize> HasSqlType for [u8; N] {
    type SqlType = FixedString<N>;
}

// =============================================================================
// FromClickHouse / ToClickHouse implementations
// =============================================================================

impl FromClickHouse<CHString> for String {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        // ClickHouse strings are length-prefixed in binary format
        // The value here should already be the raw string bytes
        String::from_utf8(value.to_vec())
            .map_err(|e| DeserializeError::Utf8Error(e.utf8_error()))
    }
}

impl ToClickHouse<CHString> for String {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        // Write length as varint, then bytes
        write_varint(self.len() as u64, out);
        out.extend_from_slice(self.as_bytes());
        Ok(())
    }
}

impl ToClickHouse<CHString> for str {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        write_varint(self.len() as u64, out);
        out.extend_from_slice(self.as_bytes());
        Ok(())
    }
}

impl ToClickHouse<CHString> for &str {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        (*self).to_clickhouse(out)
    }
}

impl<const N: usize> FromClickHouse<FixedString<N>> for [u8; N] {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != N {
            return Err(DeserializeError::InvalidData(
                format!("Expected {} bytes for FixedString, got {}", N, value.len())
            ));
        }
        let mut result = [0u8; N];
        result.copy_from_slice(value);
        Ok(result)
    }
}

impl<const N: usize> ToClickHouse<FixedString<N>> for [u8; N] {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        out.extend_from_slice(self);
        Ok(())
    }
}

impl<const N: usize> FromClickHouse<FixedString<N>> for String {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != N {
            return Err(DeserializeError::InvalidData(
                format!("Expected {} bytes for FixedString, got {}", N, value.len())
            ));
        }
        // Trim trailing null bytes
        let end = value.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        String::from_utf8(value[..end].to_vec())
            .map_err(|e| DeserializeError::Utf8Error(e.utf8_error()))
    }
}

// =============================================================================
// UUID type
// =============================================================================

/// ClickHouse UUID type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UUID;

impl SqlType for UUID {
    fn type_name() -> &'static str { "UUID" }
}

#[cfg(feature = "uuid")]
impl HasSqlType for uuid::Uuid {
    type SqlType = UUID;
}

#[cfg(feature = "uuid")]
impl FromClickHouse<UUID> for uuid::Uuid {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != 16 {
            return Err(DeserializeError::InvalidData(
                format!("Expected 16 bytes for UUID, got {}", value.len())
            ));
        }
        // ClickHouse stores UUID in a specific byte order
        let bytes: [u8; 16] = value.try_into()
            .map_err(|_| DeserializeError::InvalidData("Invalid UUID bytes".into()))?;
        Ok(uuid::Uuid::from_bytes(bytes))
    }
}

#[cfg(feature = "uuid")]
impl ToClickHouse<UUID> for uuid::Uuid {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        out.extend_from_slice(self.as_bytes());
        Ok(())
    }
}

// =============================================================================
// IP Address types
// =============================================================================

/// ClickHouse IPv4 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IPv4;

impl SqlType for IPv4 {
    fn type_name() -> &'static str { "IPv4" }
}

/// ClickHouse IPv6 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IPv6;

impl SqlType for IPv6 {
    fn type_name() -> &'static str { "IPv6" }
}

impl HasSqlType for std::net::Ipv4Addr {
    type SqlType = IPv4;
}

impl HasSqlType for std::net::Ipv6Addr {
    type SqlType = IPv6;
}

impl FromClickHouse<IPv4> for std::net::Ipv4Addr {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != 4 {
            return Err(DeserializeError::InvalidData(
                format!("Expected 4 bytes for IPv4, got {}", value.len())
            ));
        }
        let bytes: [u8; 4] = value.try_into()
            .map_err(|_| DeserializeError::InvalidData("Invalid IPv4 bytes".into()))?;
        Ok(std::net::Ipv4Addr::from(bytes))
    }
}

impl ToClickHouse<IPv4> for std::net::Ipv4Addr {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        out.extend_from_slice(&self.octets());
        Ok(())
    }
}

impl FromClickHouse<IPv6> for std::net::Ipv6Addr {
    fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
        if value.len() != 16 {
            return Err(DeserializeError::InvalidData(
                format!("Expected 16 bytes for IPv6, got {}", value.len())
            ));
        }
        let bytes: [u8; 16] = value.try_into()
            .map_err(|_| DeserializeError::InvalidData("Invalid IPv6 bytes".into()))?;
        Ok(std::net::Ipv6Addr::from(bytes))
    }
}

impl ToClickHouse<IPv6> for std::net::Ipv6Addr {
    fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
        out.extend_from_slice(&self.octets());
        Ok(())
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Write a variable-length integer (used for string lengths).
#[inline]
fn write_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Read a variable-length integer.
/// Currently only used in tests for verifying write_varint output.
#[cfg(test)]
fn read_varint(data: &[u8]) -> Result<(u64, usize), DeserializeError> {
    let mut result: u64 = 0;
    let mut shift = 0;
    let mut bytes_read = 0;

    for &byte in data {
        bytes_read += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, bytes_read));
        }
        shift += 7;
        if shift >= 64 {
            return Err(DeserializeError::InvalidData("Varint too long".into()));
        }
    }

    Err(DeserializeError::InvalidData("Incomplete varint".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_roundtrip() {
        let mut buf = Vec::new();
        let s = "Hello, ClickHouse!".to_string();
        <String as ToClickHouse<CHString>>::to_clickhouse(&s, &mut buf).unwrap();

        // Skip the varint length prefix for this test
        let (len, prefix_len) = read_varint(&buf).unwrap();
        assert_eq!(len as usize, s.len());
        let result = <String as FromClickHouse<CHString>>::from_clickhouse(&buf[prefix_len..]).unwrap();
        assert_eq!(result, s);
    }

    #[test]
    fn test_varint() {
        let mut buf = Vec::new();
        write_varint(300, &mut buf);
        let (value, _) = read_varint(&buf).unwrap();
        assert_eq!(value, 300);
    }

    #[test]
    fn test_ipv4() {
        let mut buf = Vec::new();
        let addr = std::net::Ipv4Addr::new(192, 168, 1, 1);
        <std::net::Ipv4Addr as ToClickHouse<IPv4>>::to_clickhouse(&addr, &mut buf).unwrap();
        let result = <std::net::Ipv4Addr as FromClickHouse<IPv4>>::from_clickhouse(&buf).unwrap();
        assert_eq!(result, addr);
    }
}
