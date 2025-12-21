//! Temporal (Date/Time) SQL types for ClickHouse.

use crate::{SqlType, HasSqlType, FromClickHouse, ToClickHouse, DeserializeError, SerializeError};

/// ClickHouse Date type.
///
/// Stores dates from 1970-01-01 to 2149-06-06 as the number of days since epoch.
/// Uses 2 bytes of storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Date;

impl SqlType for Date {
    fn type_name() -> &'static str { "Date" }
}

/// ClickHouse Date32 type.
///
/// Extended date range from 1900-01-01 to 2299-12-31.
/// Uses 4 bytes of storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Date32;

impl SqlType for Date32 {
    fn type_name() -> &'static str { "Date32" }
}

/// ClickHouse DateTime type.
///
/// Stores Unix timestamp (seconds since 1970-01-01 00:00:00 UTC).
/// Uses 4 bytes of storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateTime;

impl SqlType for DateTime {
    fn type_name() -> &'static str { "DateTime" }
}

/// ClickHouse DateTime64 type with configurable precision.
///
/// Stores timestamp with sub-second precision.
/// - `DateTime64(0)` - seconds
/// - `DateTime64(3)` - milliseconds
/// - `DateTime64(6)` - microseconds
/// - `DateTime64(9)` - nanoseconds
///
/// Uses 8 bytes of storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateTime64<const PRECISION: u8>;

impl<const P: u8> SqlType for DateTime64<P> {
    fn type_name() -> &'static str { "DateTime64" }
}

// =============================================================================
// Chrono integration
// =============================================================================

#[cfg(feature = "chrono")]
mod chrono_impl {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};

    /// Unix epoch date (1970-01-01).
    ///
    /// This is a helper function that returns the Unix epoch as a NaiveDate.
    /// The date 1970-01-01 is always valid, so this will never panic.
    #[inline]
    fn unix_epoch() -> NaiveDate {
        // SAFETY: 1970-01-01 is a valid date, this cannot fail
        NaiveDate::from_ymd_opt(1970, 1, 1)
            .expect("Unix epoch 1970-01-01 is always a valid date")
    }

    impl HasSqlType for NaiveDate {
        type SqlType = Date;
    }

    impl HasSqlType for NaiveDateTime {
        type SqlType = DateTime;
    }

    impl FromClickHouse<Date> for NaiveDate {
        fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
            if value.len() != 2 {
                return Err(DeserializeError::InvalidData(
                    format!("Expected 2 bytes for Date, got {}", value.len())
                ));
            }
            let bytes: [u8; 2] = value.try_into()
                .map_err(|_| DeserializeError::InvalidData("Invalid bytes".into()))?;
            let days = u16::from_le_bytes(bytes) as i64;

            NaiveDate::from_num_days_from_ce_opt(days as i32 + 719163) // Days from 0001-01-01 to 1970-01-01
                .ok_or_else(|| DeserializeError::InvalidData("Invalid date value".into()))
        }
    }

    impl ToClickHouse<Date> for NaiveDate {
        fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
            let days = self.signed_duration_since(unix_epoch()).num_days();
            if days < 0 || days > u16::MAX as i64 {
                return Err(SerializeError::OutOfRange {
                    type_name: "Date".into(),
                    value: self.to_string(),
                });
            }
            out.extend_from_slice(&(days as u16).to_le_bytes());
            Ok(())
        }
    }

    impl FromClickHouse<Date32> for NaiveDate {
        fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
            if value.len() != 4 {
                return Err(DeserializeError::InvalidData(
                    format!("Expected 4 bytes for Date32, got {}", value.len())
                ));
            }
            let bytes: [u8; 4] = value.try_into()
                .map_err(|_| DeserializeError::InvalidData("Invalid bytes".into()))?;
            let days = i32::from_le_bytes(bytes) as i64;

            NaiveDate::from_num_days_from_ce_opt(days as i32 + 719163)
                .ok_or_else(|| DeserializeError::InvalidData("Invalid date32 value".into()))
        }
    }

    impl ToClickHouse<Date32> for NaiveDate {
        fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
            let days = self.signed_duration_since(unix_epoch()).num_days() as i32;
            out.extend_from_slice(&days.to_le_bytes());
            Ok(())
        }
    }

    impl FromClickHouse<DateTime> for NaiveDateTime {
        fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
            if value.len() != 4 {
                return Err(DeserializeError::InvalidData(
                    format!("Expected 4 bytes for DateTime, got {}", value.len())
                ));
            }
            let bytes: [u8; 4] = value.try_into()
                .map_err(|_| DeserializeError::InvalidData("Invalid bytes".into()))?;
            let timestamp = u32::from_le_bytes(bytes) as i64;

            Utc.timestamp_opt(timestamp, 0)
                .single()
                .map(|dt| dt.naive_utc())
                .ok_or_else(|| DeserializeError::InvalidData("Invalid timestamp".into()))
        }
    }

    impl ToClickHouse<DateTime> for NaiveDateTime {
        fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
            let timestamp = self.and_utc().timestamp();
            if timestamp < 0 || timestamp > u32::MAX as i64 {
                return Err(SerializeError::OutOfRange {
                    type_name: "DateTime".into(),
                    value: self.to_string(),
                });
            }
            out.extend_from_slice(&(timestamp as u32).to_le_bytes());
            Ok(())
        }
    }

    impl<const P: u8> FromClickHouse<DateTime64<P>> for NaiveDateTime {
        fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
            if value.len() != 8 {
                return Err(DeserializeError::InvalidData(
                    format!("Expected 8 bytes for DateTime64, got {}", value.len())
                ));
            }
            let bytes: [u8; 8] = value.try_into()
                .map_err(|_| DeserializeError::InvalidData("Invalid bytes".into()))?;
            let ticks = i64::from_le_bytes(bytes);

            let divisor = 10i64.pow(P as u32);
            let seconds = ticks / divisor;
            let subsec_ticks = (ticks % divisor).abs();
            let nanos = (subsec_ticks * 1_000_000_000 / divisor) as u32;

            Utc.timestamp_opt(seconds, nanos)
                .single()
                .map(|dt| dt.naive_utc())
                .ok_or_else(|| DeserializeError::InvalidData("Invalid DateTime64 value".into()))
        }
    }

    impl<const P: u8> ToClickHouse<DateTime64<P>> for NaiveDateTime {
        fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
            let timestamp = self.and_utc().timestamp();
            let nanos = self.and_utc().timestamp_subsec_nanos() as i64;

            let divisor = 10i64.pow(P as u32);
            let subsec_ticks = nanos * divisor / 1_000_000_000;
            let ticks = timestamp * divisor + subsec_ticks;

            out.extend_from_slice(&ticks.to_le_bytes());
            Ok(())
        }
    }
}

// =============================================================================
// time crate integration
// =============================================================================

#[cfg(feature = "time")]
mod time_impl {
    use super::*;
    use time::{Date as TimeDate, PrimitiveDateTime, OffsetDateTime};

    impl FromClickHouse<Date> for TimeDate {
        fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
            if value.len() != 2 {
                return Err(DeserializeError::InvalidData(
                    format!("Expected 2 bytes for Date, got {}", value.len())
                ));
            }
            let bytes: [u8; 2] = value.try_into()
                .map_err(|_| DeserializeError::InvalidData("Invalid bytes".into()))?;
            let days = u16::from_le_bytes(bytes) as i32;

            TimeDate::from_ordinal_date(1970, 1)
                .and_then(|epoch| epoch.checked_add(time::Duration::days(days as i64)))
                .map_err(|_| DeserializeError::InvalidData("Invalid date value".into()))
        }
    }

    impl ToClickHouse<Date> for TimeDate {
        fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
            let epoch = TimeDate::from_ordinal_date(1970, 1)
                .map_err(|_| SerializeError::InvalidValue("Cannot create epoch".into()))?;
            let days = (*self - epoch).whole_days();
            if days < 0 || days > u16::MAX as i64 {
                return Err(SerializeError::OutOfRange {
                    type_name: "Date".into(),
                    value: self.to_string(),
                });
            }
            out.extend_from_slice(&(days as u16).to_le_bytes());
            Ok(())
        }
    }

    impl FromClickHouse<DateTime> for PrimitiveDateTime {
        fn from_clickhouse(value: &[u8]) -> Result<Self, DeserializeError> {
            if value.len() != 4 {
                return Err(DeserializeError::InvalidData(
                    format!("Expected 4 bytes for DateTime, got {}", value.len())
                ));
            }
            let bytes: [u8; 4] = value.try_into()
                .map_err(|_| DeserializeError::InvalidData("Invalid bytes".into()))?;
            let timestamp = u32::from_le_bytes(bytes) as i64;

            OffsetDateTime::from_unix_timestamp(timestamp)
                .map(|dt| PrimitiveDateTime::new(dt.date(), dt.time()))
                .map_err(|_| DeserializeError::InvalidData("Invalid timestamp".into()))
        }
    }

    impl ToClickHouse<DateTime> for PrimitiveDateTime {
        fn to_clickhouse(&self, out: &mut Vec<u8>) -> Result<(), SerializeError> {
            let timestamp = self.assume_utc().unix_timestamp();
            if timestamp < 0 || timestamp > u32::MAX as i64 {
                return Err(SerializeError::OutOfRange {
                    type_name: "DateTime".into(),
                    value: self.to_string(),
                });
            }
            out.extend_from_slice(&(timestamp as u32).to_le_bytes());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "chrono")]
    #[test]
    fn test_date_roundtrip() {
        use chrono::NaiveDate;

        let mut buf = Vec::new();
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        <NaiveDate as ToClickHouse<Date>>::to_clickhouse(&date, &mut buf).unwrap();
        let result = <NaiveDate as FromClickHouse<Date>>::from_clickhouse(&buf).unwrap();
        assert_eq!(result, date);
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn test_datetime_roundtrip() {
        use chrono::NaiveDateTime;

        let mut buf = Vec::new();
        let dt = NaiveDateTime::parse_from_str("2024-06-15 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        <NaiveDateTime as ToClickHouse<DateTime>>::to_clickhouse(&dt, &mut buf).unwrap();
        let result = <NaiveDateTime as FromClickHouse<DateTime>>::from_clickhouse(&buf).unwrap();
        assert_eq!(result, dt);
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn test_datetime64_roundtrip() {
        use chrono::NaiveDateTime;

        let mut buf = Vec::new();
        let dt = NaiveDateTime::parse_from_str("2024-06-15 14:30:00.123", "%Y-%m-%d %H:%M:%S%.3f").unwrap();
        <NaiveDateTime as ToClickHouse<DateTime64<3>>>::to_clickhouse(&dt, &mut buf).unwrap();
        let result = <NaiveDateTime as FromClickHouse<DateTime64<3>>>::from_clickhouse(&buf).unwrap();
        // Allow 1ms tolerance due to precision
        assert!((result - dt).num_milliseconds().abs() <= 1);
    }
}
