//! Complex type examples for diesel-clickhouse.
//!
//! This example demonstrates:
//! - ClickHouse type system and SQL type names
//! - Type serialization/deserialization
//! - Complex types: Arrays, Maps, Tuples, Nullable
//! - Type metadata
//!
//! Run with: cargo run --example complex_types

use diesel_clickhouse::types::*;

fn main() {
    println!("=== ClickHouse Complex Types ===\n");

    // =========================================================================
    // Type Names
    // =========================================================================
    println!("--- SQL Type Names ---\n");

    println!("Integer types:");
    println!("  UInt8:   {}", UInt8::type_name());
    println!("  UInt16:  {}", UInt16::type_name());
    println!("  UInt32:  {}", UInt32::type_name());
    println!("  UInt64:  {}", UInt64::type_name());
    println!("  UInt128: {}", UInt128::type_name());
    println!("  UInt256: {}", UInt256::type_name());
    println!("  Int8:    {}", Int8::type_name());
    println!("  Int16:   {}", Int16::type_name());
    println!("  Int32:   {}", Int32::type_name());
    println!("  Int64:   {}", Int64::type_name());
    println!("  Int128:  {}", Int128::type_name());
    println!("  Int256:  {}", Int256::type_name());
    println!();

    println!("Float types:");
    println!("  Float32: {}", Float32::type_name());
    println!("  Float64: {}", Float64::type_name());
    println!();

    println!("String types:");
    println!("  String:  {}", CHString::type_name());
    println!("  UUID:    {}", UUID::type_name());
    println!("  IPv4:    {}", IPv4::type_name());
    println!("  IPv6:    {}", IPv6::type_name());
    println!();

    println!("Temporal types:");
    println!("  Date:       {}", Date::type_name());
    println!("  Date32:     {}", Date32::type_name());
    println!("  DateTime:   {}", DateTime::type_name());
    println!("  DateTime64: {}", <DateTime64<3>>::type_name());
    println!();

    println!("Complex types:");
    println!("  Array<UInt64>:               {}", <Array<UInt64>>::type_name());
    println!("  Array<String>:               {}", <Array<CHString>>::type_name());
    println!("  Map<String, UInt64>:         {}", <Map<CHString, UInt64>>::type_name());
    println!("  Map<String, String>:         {}", <Map<CHString, CHString>>::type_name());
    println!("  Nullable<UInt64>:            {}", <Nullable<UInt64>>::type_name());
    println!("  Nullable<String>:            {}", <Nullable<CHString>>::type_name());
    println!("  LowCardinality<String>:      {}", <LowCardinality<CHString>>::type_name());
    println!();

    // =========================================================================
    // Type Serialization
    // =========================================================================
    println!("--- Type Serialization ---\n");

    // Integers
    println!("Integers:");
    demo_type::<u8, UInt8>("u8", &42u8);
    demo_type::<u32, UInt32>("u32", &1_000_000u32);
    demo_type::<u64, UInt64>("u64", &9_999_999_999u64);
    demo_type::<i32, Int32>("i32", &-42i32);
    demo_type::<i64, Int64>("i64", &-1_000_000_000i64);
    println!();

    // Floats
    println!("Floats:");
    demo_type::<f32, Float32>("f32", &3.14159f32);
    demo_type::<f64, Float64>("f64", &2.718281828459045f64);
    println!();

    // Boolean
    println!("Boolean:");
    demo_type::<bool, Bool>("true", &true);
    demo_type::<bool, Bool>("false", &false);
    println!();

    // =========================================================================
    // String Types
    // =========================================================================
    println!("--- String Types ---\n");

    demo_string("Hello, ClickHouse!");
    demo_string("Unicode: ");
    demo_string("");
    demo_string("A".repeat(100).as_str());
    println!();

    // =========================================================================
    // Nullable Types
    // =========================================================================
    println!("--- Nullable Types ---\n");

    demo_nullable::<u32, UInt32>("Some(42)", Some(42u32));
    demo_nullable::<u32, UInt32>("None", None);
    demo_nullable::<i64, Int64>("Some(-1000)", Some(-1000i64));
    demo_nullable::<i64, Int64>("None", None);
    println!();

    // =========================================================================
    // Date/Time Types
    // =========================================================================
    println!("--- Date/Time Types ---\n");

    use chrono::{NaiveDate, NaiveDateTime};

    let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
    demo_date(&date);

    let datetime = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
        chrono::NaiveTime::from_hms_opt(14, 30, 0).unwrap(),
    );
    demo_datetime(&datetime);
    println!();

    // =========================================================================
    // 256-bit Integers
    // =========================================================================
    println!("--- 256-bit Integers ---\n");

    let u256_val = U256::from_u128(u128::MAX);
    demo_u256(&u256_val);

    let u256_small = U256::from_u128(42);
    demo_u256(&u256_small);

    let i256_val = I256::from_i128(-1);
    demo_i256(&i256_val);

    let i256_large = I256::from_i128(i128::MAX);
    demo_i256(&i256_large);
    println!();

    // =========================================================================
    // IP Addresses
    // =========================================================================
    println!("--- IP Address Types ---\n");

    use std::net::{Ipv4Addr, Ipv6Addr};

    demo_ipv4(&Ipv4Addr::new(192, 168, 1, 1));
    demo_ipv4(&Ipv4Addr::new(10, 0, 0, 1));
    demo_ipv4(&Ipv4Addr::LOCALHOST);

    demo_ipv6(&Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
    demo_ipv6(&Ipv6Addr::LOCALHOST);
    println!();

    // =========================================================================
    // Type Mapping Reference
    // =========================================================================
    println!("--- Type Mapping Reference ---\n");

    println!("| ClickHouse Type      | Rust Type              | Size    |");
    println!("|----------------------|------------------------|---------|");
    println!("| UInt8                | u8                     | 1 byte  |");
    println!("| UInt16               | u16                    | 2 bytes |");
    println!("| UInt32               | u32                    | 4 bytes |");
    println!("| UInt64               | u64                    | 8 bytes |");
    println!("| UInt128              | u128                   | 16 bytes|");
    println!("| UInt256              | U256                   | 32 bytes|");
    println!("| Int8                 | i8                     | 1 byte  |");
    println!("| Int16                | i16                    | 2 bytes |");
    println!("| Int32                | i32                    | 4 bytes |");
    println!("| Int64                | i64                    | 8 bytes |");
    println!("| Int128               | i128                   | 16 bytes|");
    println!("| Int256               | I256                   | 32 bytes|");
    println!("| Float32              | f32                    | 4 bytes |");
    println!("| Float64              | f64                    | 8 bytes |");
    println!("| Bool                 | bool                   | 1 byte  |");
    println!("| String               | String                 | variable|");
    println!("| FixedString(N)       | [u8; N]                | N bytes |");
    println!("| UUID                 | uuid::Uuid             | 16 bytes|");
    println!("| Date                 | chrono::NaiveDate      | 2 bytes |");
    println!("| Date32               | chrono::NaiveDate      | 4 bytes |");
    println!("| DateTime             | chrono::NaiveDateTime  | 4 bytes |");
    println!("| DateTime64(p)        | chrono::NaiveDateTime  | 8 bytes |");
    println!("| IPv4                 | std::net::Ipv4Addr     | 4 bytes |");
    println!("| IPv6                 | std::net::Ipv6Addr     | 16 bytes|");
    println!("| Array(T)             | Vec<T>                 | variable|");
    println!("| Map(K, V)            | HashMap<K, V>          | variable|");
    println!("| Nullable(T)          | Option<T>              | 1 + T   |");
    println!("| LowCardinality(T)    | T                      | T size  |");
    println!("| Tuple(T1, T2, ...)   | (T1, T2, ...)          | sum     |");
    println!();

    // =========================================================================
    // Usage Patterns
    // =========================================================================
    println!("--- Common Usage Patterns ---\n");

    println!("1. Arrays:");
    println!("   ClickHouse: Array(String)");
    println!("   Rust:       Vec<String>");
    println!("   Example:    vec![\"tag1\".to_string(), \"tag2\".to_string()]");
    println!();

    println!("2. Maps (dictionaries):");
    println!("   ClickHouse: Map(String, String)");
    println!("   Rust:       HashMap<String, String>");
    println!("   Example:    HashMap::from([(\"key\".into(), \"value\".into())])");
    println!();

    println!("3. Nullable fields:");
    println!("   ClickHouse: Nullable(UInt64)");
    println!("   Rust:       Option<u64>");
    println!("   Example:    Some(42) or None");
    println!();

    println!("4. LowCardinality (for low-cardinality strings):");
    println!("   ClickHouse: LowCardinality(String)");
    println!("   Rust:       String (transparent)");
    println!("   Use case:   Country codes, status enums, category names");
    println!();

    println!("5. Nested arrays:");
    println!("   ClickHouse: Array(Array(Float64))");
    println!("   Rust:       Vec<Vec<f64>>");
    println!("   Example:    vec![vec![1.0, 2.0], vec![3.0, 4.0]]");
    println!();

    println!("=== End of Complex Types Examples ===");
}

// =============================================================================
// Helper Functions
// =============================================================================

fn demo_type<T, ST>(name: &str, value: &T)
where
    T: ToClickHouse<ST> + FromClickHouse<ST> + std::fmt::Debug + PartialEq,
    ST: SqlType,
{
    let mut buf = Vec::new();
    <T as ToClickHouse<ST>>::to_clickhouse(value, &mut buf).unwrap();
    let decoded = <T as FromClickHouse<ST>>::from_clickhouse(&buf).unwrap();

    let status = if &decoded == value { "OK" } else { "MISMATCH" };
    println!(
        "  {:12} {:>20?} -> {:2} bytes -> {:>20?} [{}]",
        name,
        value,
        buf.len(),
        decoded,
        status
    );
}

fn demo_string(value: &str) {
    let s = value.to_string();
    let mut buf = Vec::new();
    <String as ToClickHouse<CHString>>::to_clickhouse(&s, &mut buf).unwrap();

    // Determine varint prefix length
    let prefix_len = if value.len() < 128 { 1 } else if value.len() < 16384 { 2 } else { 3 };
    let decoded = <String as FromClickHouse<CHString>>::from_clickhouse(&buf[prefix_len..]).unwrap();

    let display = if value.is_empty() {
        "(empty)".to_string()
    } else if value.len() > 20 {
        format!("{}... ({} chars)", &value[..20], value.len())
    } else {
        value.to_string()
    };

    let status = if decoded == value { "OK" } else { "MISMATCH" };
    println!(
        "  {:30} -> {:3} bytes (prefix: {}) [{}]",
        display,
        buf.len(),
        prefix_len,
        status
    );
}

fn demo_nullable<T, ST>(name: &str, value: Option<T>)
where
    T: ToClickHouse<ST> + FromClickHouse<ST> + std::fmt::Debug + PartialEq + Clone,
    ST: SqlType,
{
    let mut buf = Vec::new();
    <Option<T> as ToClickHouse<Nullable<ST>>>::to_clickhouse(&value, &mut buf).unwrap();
    let decoded = <Option<T> as FromClickHouse<Nullable<ST>>>::from_clickhouse(&buf).unwrap();

    let status = if decoded == value { "OK" } else { "MISMATCH" };
    println!(
        "  {:20} {:>15?} -> {:2} bytes -> {:>15?} [{}]",
        name,
        value,
        buf.len(),
        decoded,
        status
    );
}

fn demo_date(date: &chrono::NaiveDate) {
    let mut buf = Vec::new();
    <chrono::NaiveDate as ToClickHouse<Date>>::to_clickhouse(date, &mut buf).unwrap();
    let decoded = <chrono::NaiveDate as FromClickHouse<Date>>::from_clickhouse(&buf).unwrap();

    let status = if &decoded == date { "OK" } else { "MISMATCH" };
    println!(
        "  Date:     {} -> {} bytes -> {} [{}]",
        date,
        buf.len(),
        decoded,
        status
    );
}

fn demo_datetime(datetime: &chrono::NaiveDateTime) {
    let mut buf = Vec::new();
    <chrono::NaiveDateTime as ToClickHouse<DateTime>>::to_clickhouse(datetime, &mut buf).unwrap();
    let decoded = <chrono::NaiveDateTime as FromClickHouse<DateTime>>::from_clickhouse(&buf).unwrap();

    let status = if &decoded == datetime { "OK" } else { "MISMATCH" };
    println!(
        "  DateTime: {} -> {} bytes -> {} [{}]",
        datetime,
        buf.len(),
        decoded,
        status
    );
}

fn demo_u256(value: &U256) {
    let mut buf = Vec::new();
    <U256 as ToClickHouse<UInt256>>::to_clickhouse(value, &mut buf).unwrap();
    let decoded = <U256 as FromClickHouse<UInt256>>::from_clickhouse(&buf).unwrap();

    let status = if &decoded == value { "OK" } else { "MISMATCH" };
    println!(
        "  U256: {:?} -> {} bytes [{}]",
        value.to_u128().map(|v| format!("{}", v)).unwrap_or_else(|| "overflow".to_string()),
        buf.len(),
        status
    );
}

fn demo_i256(value: &I256) {
    let mut buf = Vec::new();
    <I256 as ToClickHouse<Int256>>::to_clickhouse(value, &mut buf).unwrap();
    let decoded = <I256 as FromClickHouse<Int256>>::from_clickhouse(&buf).unwrap();

    let status = if &decoded == value { "OK" } else { "MISMATCH" };
    println!(
        "  I256: {:?} -> {} bytes [{}]",
        value.to_i128().map(|v| format!("{}", v)).unwrap_or_else(|| "overflow".to_string()),
        buf.len(),
        status
    );
}

fn demo_ipv4(addr: &std::net::Ipv4Addr) {
    let mut buf = Vec::new();
    <std::net::Ipv4Addr as ToClickHouse<IPv4>>::to_clickhouse(addr, &mut buf).unwrap();
    let decoded = <std::net::Ipv4Addr as FromClickHouse<IPv4>>::from_clickhouse(&buf).unwrap();

    let status = if &decoded == addr { "OK" } else { "MISMATCH" };
    println!(
        "  IPv4: {:15} -> {} bytes -> {:15} [{}]",
        addr,
        buf.len(),
        decoded,
        status
    );
}

fn demo_ipv6(addr: &std::net::Ipv6Addr) {
    let mut buf = Vec::new();
    <std::net::Ipv6Addr as ToClickHouse<IPv6>>::to_clickhouse(addr, &mut buf).unwrap();
    let decoded = <std::net::Ipv6Addr as FromClickHouse<IPv6>>::from_clickhouse(&buf).unwrap();

    let status = if &decoded == addr { "OK" } else { "MISMATCH" };
    println!(
        "  IPv6: {} -> {} bytes [{}]",
        addr,
        buf.len(),
        status
    );
}
