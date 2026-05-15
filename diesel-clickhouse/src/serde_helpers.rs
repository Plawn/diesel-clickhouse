#![allow(clippy::unwrap_used)]

//! Serde helpers for automatic chrono type serialization in ClickHouseRow.
//!
//! These modules are auto-injected by the `ClickHouseRow` derive macro when
//! chrono types are detected without an explicit `#[serde(with = "...")]`.

#[cfg(all(feature = "http", feature = "chrono"))]
pub mod naive_datetime {
    use chrono::NaiveDateTime;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde::de::Error as _;
    use serde::ser::Error as _;

    pub fn serialize<S: Serializer>(dt: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error> {
        let ts = dt.and_utc().timestamp();
        let ts_u32 = u32::try_from(ts)
            .map_err(|_| S::Error::custom(format!("{dt} cannot be represented as DateTime")))?;
        ts_u32.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NaiveDateTime, D::Error> {
        let ts: u32 = Deserialize::deserialize(deserializer)?;
        chrono::DateTime::from_timestamp(i64::from(ts), 0)
            .map(|dt| dt.naive_utc())
            .ok_or_else(|| D::Error::custom(format!("{ts} cannot be converted to NaiveDateTime")))
    }
}

#[cfg(all(feature = "http", feature = "chrono"))]
pub mod naive_date {
    use chrono::{Duration, NaiveDate};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde::ser::Error as _;

    const ORIGIN: Option<NaiveDate> = NaiveDate::from_yo_opt(1970, 1);

    pub fn serialize<S: Serializer>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error> {
        let origin = ORIGIN.unwrap();
        if *date < origin {
            return Err(S::Error::custom(format!("{date} cannot be represented as Date")));
        }
        let days = (*date - origin).num_days();
        u16::try_from(days)
            .map_err(|_| S::Error::custom(format!("{date} cannot be represented as Date")))?
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NaiveDate, D::Error> {
        let days: u16 = Deserialize::deserialize(deserializer)?;
        Ok(ORIGIN.unwrap() + Duration::days(i64::from(days)))
    }
}

#[cfg(all(feature = "http", feature = "chrono"))]
pub mod datetime_utc {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde::de::Error as _;
    use serde::ser::Error as _;

    pub fn serialize<S: Serializer>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
        let ts = dt.timestamp();
        u32::try_from(ts)
            .map_err(|_| S::Error::custom(format!("{dt} cannot be represented as DateTime")))?
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Utc>, D::Error> {
        let ts: u32 = Deserialize::deserialize(deserializer)?;
        DateTime::<Utc>::from_timestamp(i64::from(ts), 0)
            .ok_or_else(|| D::Error::custom(format!("{ts} cannot be converted to DateTime<Utc>")))
    }
}
