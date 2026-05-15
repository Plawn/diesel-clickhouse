//! Compile-time type safety for ClickHouseRow fields.
//!
//! The `ClickHouseFieldType` marker trait is implemented for all Rust types
//! that can be serialized to/from ClickHouse columns. The `ClickHouseRow`
//! derive macro generates a const assertion that each field type implements
//! this trait, catching unsupported types at compile time.

/// Marker trait for Rust types compatible with ClickHouse columns.
///
/// If a field in a `#[derive(ClickHouseRow)]` struct doesn't implement this
/// trait, you'll get a compile error like:
///
/// ```text
/// the trait `ClickHouseFieldType` is not implemented for `MyCustomType`
/// ```
///
/// To fix this, either:
/// 1. Use a supported type (u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, bool, String, etc.)
/// 2. Implement `ClickHouseFieldType` for your custom type
/// 3. Add `#[serde(with = "...")]` to provide custom serialization
pub trait ClickHouseFieldType {}

impl ClickHouseFieldType for u8 {}
impl ClickHouseFieldType for u16 {}
impl ClickHouseFieldType for u32 {}
impl ClickHouseFieldType for u64 {}
impl ClickHouseFieldType for u128 {}
impl ClickHouseFieldType for i8 {}
impl ClickHouseFieldType for i16 {}
impl ClickHouseFieldType for i32 {}
impl ClickHouseFieldType for i64 {}
impl ClickHouseFieldType for i128 {}
impl ClickHouseFieldType for f32 {}
impl ClickHouseFieldType for f64 {}
impl ClickHouseFieldType for bool {}
impl ClickHouseFieldType for String {}
impl<'a> ClickHouseFieldType for &'a str {}

impl<T: ClickHouseFieldType> ClickHouseFieldType for Option<T> {}
impl<T: ClickHouseFieldType> ClickHouseFieldType for Vec<T> {}

impl ClickHouseFieldType for std::net::Ipv4Addr {}
impl ClickHouseFieldType for std::net::Ipv6Addr {}

#[cfg(feature = "uuid")]
impl ClickHouseFieldType for uuid::Uuid {}

#[cfg(feature = "json")]
impl ClickHouseFieldType for serde_json::Value {}

#[cfg(feature = "chrono")]
mod chrono_impls {
    use super::ClickHouseFieldType;

    impl ClickHouseFieldType for chrono::NaiveDateTime {}
    impl ClickHouseFieldType for chrono::NaiveDate {}
    impl<Tz: chrono::TimeZone> ClickHouseFieldType for chrono::DateTime<Tz> {}
    impl ClickHouseFieldType for chrono::Duration {}
}
