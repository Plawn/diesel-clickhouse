//! ClickHouse type string parser.
//!
//! This module provides functionality to parse ClickHouse type strings
//! (e.g., "Nullable(Array(UInt64))") into a structured AST.
//!
//! Based on [ch2rs](https://github.com/ClickHouse/ch2rs) (MIT License).
//! Original authors: ClickHouse Contributors, Paul Loyd <pavelko95@gmail.com>

use std::borrow::Cow;
use std::fmt;

use crate::result::{Error, QueryResult};

/// Represents a ClickHouse SQL type parsed from a type string.
///
/// This enum covers all standard ClickHouse types and can be used for:
/// - Schema introspection (reading from `system.columns`)
/// - Code generation (generating `table!` macros)
/// - Schema validation (comparing declared vs actual types)
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
#[allow(clippy::upper_case_acronyms)]
pub enum ClickHouseSqlType {
    // Integers
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    UInt128,
    UInt256,
    Int8,
    Int16,
    Int32,
    Int64,
    Int128,
    Int256,

    // Boolean
    Bool,

    // Floats
    Float32,
    Float64,

    // Strings
    String,
    FixedString(u32),

    // Date/Time
    Date,
    Date32,
    DateTime(Option<std::string::String>),
    DateTime64(u8, Option<std::string::String>),

    // Network
    IPv4,
    IPv6,

    // UUID
    UUID,

    // Decimal
    Decimal(u8, u8),
    Decimal32(u8),
    Decimal64(u8),
    Decimal128(u8),
    Decimal256(u8),

    // Enums
    Enum8(Vec<(std::string::String, i8)>),
    Enum16(Vec<(std::string::String, i16)>),

    // Complex types
    Array(Box<ClickHouseSqlType>),
    Tuple(Vec<ClickHouseSqlType>),
    Map(Box<ClickHouseSqlType>, Box<ClickHouseSqlType>),
    Nested(Vec<(std::string::String, ClickHouseSqlType)>),

    // Nullable wrapper
    Nullable(Box<ClickHouseSqlType>),

    // LowCardinality wrapper (stored for information, often transparent)
    LowCardinality(Box<ClickHouseSqlType>),

    // JSON type (ClickHouse 22.3+)
    JSON,

    // Object('json') - older JSON syntax
    Object(std::string::String),

    // Unknown/unsupported type (stores the raw string)
    Unknown(std::string::String),
}

impl fmt::Display for ClickHouseSqlType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Simple types
            ClickHouseSqlType::UInt8 => write!(f, "UInt8"),
            ClickHouseSqlType::UInt16 => write!(f, "UInt16"),
            ClickHouseSqlType::UInt32 => write!(f, "UInt32"),
            ClickHouseSqlType::UInt64 => write!(f, "UInt64"),
            ClickHouseSqlType::UInt128 => write!(f, "UInt128"),
            ClickHouseSqlType::UInt256 => write!(f, "UInt256"),
            ClickHouseSqlType::Int8 => write!(f, "Int8"),
            ClickHouseSqlType::Int16 => write!(f, "Int16"),
            ClickHouseSqlType::Int32 => write!(f, "Int32"),
            ClickHouseSqlType::Int64 => write!(f, "Int64"),
            ClickHouseSqlType::Int128 => write!(f, "Int128"),
            ClickHouseSqlType::Int256 => write!(f, "Int256"),
            ClickHouseSqlType::Bool => write!(f, "Bool"),
            ClickHouseSqlType::Float32 => write!(f, "Float32"),
            ClickHouseSqlType::Float64 => write!(f, "Float64"),
            ClickHouseSqlType::String => write!(f, "String"),
            ClickHouseSqlType::FixedString(n) => write!(f, "FixedString({n})"),
            ClickHouseSqlType::Date => write!(f, "Date"),
            ClickHouseSqlType::Date32 => write!(f, "Date32"),
            ClickHouseSqlType::DateTime(None) => write!(f, "DateTime"),
            ClickHouseSqlType::DateTime(Some(tz)) => write!(f, "DateTime('{tz}')"),
            ClickHouseSqlType::DateTime64(prec, None) => write!(f, "DateTime64({prec})"),
            ClickHouseSqlType::DateTime64(prec, Some(tz)) => write!(f, "DateTime64({prec}, '{tz}')"),
            ClickHouseSqlType::IPv4 => write!(f, "IPv4"),
            ClickHouseSqlType::IPv6 => write!(f, "IPv6"),
            ClickHouseSqlType::UUID => write!(f, "UUID"),
            ClickHouseSqlType::Decimal(p, s) => write!(f, "Decimal({p}, {s})"),
            ClickHouseSqlType::Decimal32(s) => write!(f, "Decimal32({s})"),
            ClickHouseSqlType::Decimal64(s) => write!(f, "Decimal64({s})"),
            ClickHouseSqlType::Decimal128(s) => write!(f, "Decimal128({s})"),
            ClickHouseSqlType::Decimal256(s) => write!(f, "Decimal256({s})"),
            ClickHouseSqlType::JSON => write!(f, "JSON"),
            ClickHouseSqlType::Object(variant) => write!(f, "Object('{variant}')"),

            // Enums
            ClickHouseSqlType::Enum8(variants) => {
                write!(f, "Enum8(")?;
                for (i, (name, val)) in variants.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "'{name}' = {val}")?;
                }
                write!(f, ")")
            }
            ClickHouseSqlType::Enum16(variants) => {
                write!(f, "Enum16(")?;
                for (i, (name, val)) in variants.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "'{name}' = {val}")?;
                }
                write!(f, ")")
            }

            // Complex types
            ClickHouseSqlType::Array(inner) => write!(f, "Array({inner})"),
            ClickHouseSqlType::Nullable(inner) => write!(f, "Nullable({inner})"),
            ClickHouseSqlType::LowCardinality(inner) => write!(f, "LowCardinality({inner})"),
            ClickHouseSqlType::Map(k, v) => write!(f, "Map({k}, {v})"),
            ClickHouseSqlType::Tuple(types) => {
                write!(f, "Tuple(")?;
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{t}")?;
                }
                write!(f, ")")
            }
            ClickHouseSqlType::Nested(fields) => {
                write!(f, "Nested(")?;
                for (i, (name, t)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name} {t}")?;
                }
                write!(f, ")")
            }

            ClickHouseSqlType::Unknown(s) => write!(f, "{s}"),
        }
    }
}

/// Column metadata from `system.columns`.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    /// Column name.
    pub name: std::string::String,
    /// Parsed SQL type.
    pub sql_type: ClickHouseSqlType,
    /// Column comment (if any).
    pub comment: std::string::String,
    /// Default expression kind (if any): "DEFAULT", "MATERIALIZED", "ALIAS".
    pub default_kind: std::string::String,
    /// Default expression (if any).
    pub default_expression: std::string::String,
}

/// Table metadata from `system.tables` and `system.columns`.
#[derive(Debug, Clone)]
pub struct TableInfo {
    /// Database name.
    pub database: std::string::String,
    /// Table name.
    pub name: std::string::String,
    /// Table engine (e.g., "MergeTree", "ReplacingMergeTree").
    pub engine: std::string::String,
    /// Columns in order.
    pub columns: Vec<ColumnInfo>,
}

/// Parse a ClickHouse type string into a `ClickHouseSqlType`.
///
/// # Examples
///
/// ```
/// use diesel_clickhouse_core::type_parser::parse_type;
///
/// let ty = parse_type("Nullable(Array(UInt64))").unwrap();
/// assert_eq!(ty.to_string(), "Nullable(Array(UInt64))");
///
/// let ty = parse_type("DateTime64(3, 'UTC')").unwrap();
/// assert_eq!(ty.to_string(), "DateTime64(3, 'UTC')");
/// ```
pub fn parse_type(raw: &str) -> QueryResult<ClickHouseSqlType> {
    let raw = raw.trim();

    // Handle SimpleAggregateFunction - extract the second argument (the actual type)
    let raw = if let Some(args) = extract_inner(raw, "SimpleAggregateFunction") {
        // SimpleAggregateFunction(func, Type) -> Type
        extract_second_arg(args).ok_or_else(|| {
            Error::TypeParseError(Cow::Owned(format!(
                "invalid SimpleAggregateFunction format: {raw}"
            )))
        })?
    } else if let Some(args) = extract_inner(raw, "AggregateFunction") {
        // AggregateFunction(func, Type) -> Type
        extract_second_arg(args).ok_or_else(|| {
            Error::TypeParseError(Cow::Owned(format!("invalid AggregateFunction format: {raw}")))
        })?
    } else {
        raw
    };

    // Parse the type
    parse_type_inner(raw)
}

fn parse_type_inner(raw: &str) -> QueryResult<ClickHouseSqlType> {
    let raw = raw.trim();

    // Handle LowCardinality - keep it in the AST for information
    if let Some(inner) = extract_inner(raw, "LowCardinality") {
        return Ok(ClickHouseSqlType::LowCardinality(Box::new(parse_type_inner(
            inner,
        )?)));
    }

    // Try simple (non-parametric) types first
    if let Some(simple) = parse_simple_type(raw) {
        return Ok(simple);
    }

    // Try parametric types (types with parentheses)
    parse_parametric_type(raw)
}

/// Parse simple (non-parametric) ClickHouse types.
///
/// Returns `None` if the type is not a recognized simple type.
fn parse_simple_type(raw: &str) -> Option<ClickHouseSqlType> {
    Some(match raw {
        // Integers
        "UInt8" => ClickHouseSqlType::UInt8,
        "UInt16" => ClickHouseSqlType::UInt16,
        "UInt32" => ClickHouseSqlType::UInt32,
        "UInt64" => ClickHouseSqlType::UInt64,
        "UInt128" => ClickHouseSqlType::UInt128,
        "UInt256" => ClickHouseSqlType::UInt256,
        "Int8" => ClickHouseSqlType::Int8,
        "Int16" => ClickHouseSqlType::Int16,
        "Int32" => ClickHouseSqlType::Int32,
        "Int64" => ClickHouseSqlType::Int64,
        "Int128" => ClickHouseSqlType::Int128,
        "Int256" => ClickHouseSqlType::Int256,

        // Boolean
        "Bool" | "Boolean" => ClickHouseSqlType::Bool,

        // Floats
        "Float32" => ClickHouseSqlType::Float32,
        "Float64" => ClickHouseSqlType::Float64,

        // Strings
        "String" => ClickHouseSqlType::String,

        // Dates
        "Date" => ClickHouseSqlType::Date,
        "Date32" => ClickHouseSqlType::Date32,
        "DateTime" => ClickHouseSqlType::DateTime(None),

        // Network
        "IPv4" => ClickHouseSqlType::IPv4,
        "IPv6" => ClickHouseSqlType::IPv6,

        // UUID
        "UUID" => ClickHouseSqlType::UUID,

        // JSON
        "JSON" => ClickHouseSqlType::JSON,

        _ => return None,
    })
}

/// Parse parametric ClickHouse types (types with parentheses).
fn parse_parametric_type(raw: &str) -> QueryResult<ClickHouseSqlType> {
    // Wrapper types (contain a single inner type)
    if let Some(inner) = extract_inner(raw, "Nullable") {
        return Ok(ClickHouseSqlType::Nullable(Box::new(parse_type_inner(inner)?)));
    }
    if let Some(inner) = extract_inner(raw, "Array") {
        return Ok(ClickHouseSqlType::Array(Box::new(parse_type_inner(inner)?)));
    }

    // DateTime types
    if let Some(inner) = extract_inner(raw, "DateTime64") {
        return parse_datetime64(inner);
    }
    if let Some(inner) = extract_inner(raw, "DateTime") {
        return Ok(ClickHouseSqlType::DateTime(Some(inner.trim_matches('\'').into())));
    }

    // String types
    if let Some(inner) = extract_inner(raw, "FixedString") {
        return parse_fixed_string(inner);
    }

    // Decimal types
    if let Some(inner) = extract_inner(raw, "Decimal256") {
        return parse_decimal_scale(inner, ClickHouseSqlType::Decimal256);
    }
    if let Some(inner) = extract_inner(raw, "Decimal128") {
        return parse_decimal_scale(inner, ClickHouseSqlType::Decimal128);
    }
    if let Some(inner) = extract_inner(raw, "Decimal64") {
        return parse_decimal_scale(inner, ClickHouseSqlType::Decimal64);
    }
    if let Some(inner) = extract_inner(raw, "Decimal32") {
        return parse_decimal_scale(inner, ClickHouseSqlType::Decimal32);
    }
    if let Some(inner) = extract_inner(raw, "Decimal") {
        return parse_decimal(inner, raw);
    }

    // Enum types
    if let Some(inner) = extract_inner(raw, "Enum8") {
        return Ok(ClickHouseSqlType::Enum8(parse_enum_variants::<i8>(inner)?));
    }
    if let Some(inner) = extract_inner(raw, "Enum16") {
        return Ok(ClickHouseSqlType::Enum16(parse_enum_variants::<i16>(inner)?));
    }

    // Collection types
    if let Some(inner) = extract_inner(raw, "Tuple") {
        return parse_tuple(inner);
    }
    if let Some(inner) = extract_inner(raw, "Map") {
        return parse_map(inner, raw);
    }
    if let Some(inner) = extract_inner(raw, "Nested") {
        return Ok(ClickHouseSqlType::Nested(parse_nested_fields(inner)?));
    }

    // Object type
    if let Some(inner) = extract_inner(raw, "Object") {
        return Ok(ClickHouseSqlType::Object(inner.trim_matches('\'').into()));
    }

    // Unknown type - store raw string
    Ok(ClickHouseSqlType::Unknown(raw.into()))
}

// =============================================================================
// Type-specific parsers
// =============================================================================

/// Parse DateTime64(precision[, timezone]).
fn parse_datetime64(inner: &str) -> QueryResult<ClickHouseSqlType> {
    let (prec_str, tz) = split_first_arg(inner);
    let prec = prec_str.trim().parse::<u8>().map_err(|_| {
        Error::TypeParseError(Cow::Owned(format!("invalid DateTime64 precision: {prec_str}")))
    })?;
    Ok(ClickHouseSqlType::DateTime64(
        prec,
        tz.map(|s| s.trim().trim_matches('\'').into()),
    ))
}

/// Parse FixedString(size).
fn parse_fixed_string(inner: &str) -> QueryResult<ClickHouseSqlType> {
    let n = inner.trim().parse::<u32>().map_err(|_| {
        Error::TypeParseError(Cow::Owned(format!("invalid FixedString size: {inner}")))
    })?;
    Ok(ClickHouseSqlType::FixedString(n))
}

/// Parse DecimalXX(scale) types.
fn parse_decimal_scale<F>(inner: &str, constructor: F) -> QueryResult<ClickHouseSqlType>
where
    F: FnOnce(u8) -> ClickHouseSqlType,
{
    let scale = inner.trim().parse::<u8>().map_err(|_| {
        Error::TypeParseError(Cow::Owned(format!("invalid Decimal scale: {inner}")))
    })?;
    Ok(constructor(scale))
}

/// Parse Decimal(precision, scale).
fn parse_decimal(inner: &str, raw: &str) -> QueryResult<ClickHouseSqlType> {
    let (p, s) = split_two_args(inner).ok_or_else(|| {
        Error::TypeParseError(Cow::Owned(format!("invalid Decimal format: {raw}")))
    })?;
    let precision = p.trim().parse::<u8>().map_err(|_| {
        Error::TypeParseError(Cow::Owned(format!("invalid Decimal precision: {p}")))
    })?;
    let scale = s.trim().parse::<u8>().map_err(|_| {
        Error::TypeParseError(Cow::Owned(format!("invalid Decimal scale: {s}")))
    })?;
    Ok(ClickHouseSqlType::Decimal(precision, scale))
}

/// Parse Tuple(type1, type2, ...).
fn parse_tuple(inner: &str) -> QueryResult<ClickHouseSqlType> {
    let types = split_toplevel_args(inner)
        .into_iter()
        .map(|s| parse_type_inner(s.trim()))
        .collect::<QueryResult<Vec<_>>>()?;
    Ok(ClickHouseSqlType::Tuple(types))
}

/// Parse Map(key_type, value_type).
fn parse_map(inner: &str, raw: &str) -> QueryResult<ClickHouseSqlType> {
    let (key, value) = split_two_args(inner).ok_or_else(|| {
        Error::TypeParseError(Cow::Owned(format!("invalid Map format: {raw}")))
    })?;
    let key_type = parse_type_inner(key.trim())?;
    let value_type = parse_type_inner(value.trim())?;
    Ok(ClickHouseSqlType::Map(Box::new(key_type), Box::new(value_type)))
}

/// Extract content between parentheses for a given wrapper type.
/// e.g., extract_inner("Nullable(UInt64)", "Nullable") -> Some("UInt64")
fn extract_inner<'a>(raw: &'a str, wrapper: &str) -> Option<&'a str> {
    if raw.starts_with(wrapper) {
        let rest = &raw[wrapper.len()..];
        if rest.starts_with('(') && rest.ends_with(')') {
            return Some(&rest[1..rest.len() - 1]);
        }
    }
    None
}

/// Split on the first comma, respecting nested parentheses.
/// Returns (first_arg, Some(rest)) or (whole, None).
fn split_first_arg(s: &str) -> (&str, Option<&str>) {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                return (&s[..i], Some(&s[i + 1..]));
            }
            _ => {}
        }
    }
    (s, None)
}

/// Split into exactly two arguments.
fn split_two_args(s: &str) -> Option<(&str, &str)> {
    let (first, rest) = split_first_arg(s);
    rest.map(|r| (first, r.trim()))
}

/// Extract the second argument from "func, Type" or "func, Type, ...".
fn extract_second_arg(s: &str) -> Option<&str> {
    let (_, rest) = split_first_arg(s);
    rest.map(|r| r.trim())
}

/// Split all top-level arguments (respecting nested parens).
fn split_toplevel_args(s: &str) -> Vec<&str> {
    // Pre-allocate based on comma count estimate
    let estimated_count = s.bytes().filter(|&b| b == b',').count() + 1;
    let mut result = Vec::with_capacity(estimated_count);
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    if start < s.len() {
        result.push(&s[start..]);
    }

    result
}

/// Parse enum variants: 'K' = v, 'K2' = v2
fn parse_enum_variants<T>(raw: &str) -> QueryResult<Vec<(std::string::String, T)>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    // Split on ", " but handle quoted strings
    let parts = split_enum_parts(raw);
    // Pre-allocate based on known parts count
    let mut result = Vec::with_capacity(parts.len());

    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Parse 'Name' = Value
        let (name, value) = part.split_once(" = ").ok_or_else(|| {
            Error::TypeParseError(Cow::Owned(format!("invalid enum variant format: {part}")))
        })?;

        // Remove quotes from name
        let name = name.trim().trim_matches('\'');

        // Parse value
        let value: T = value.trim().parse().map_err(|e| {
            Error::TypeParseError(Cow::Owned(format!("invalid enum value '{value}': {e}")))
        })?;

        result.push((name.into(), value));
    }

    Ok(result)
}

/// Split enum parts, handling quoted strings with commas inside.
fn split_enum_parts(s: &str) -> Vec<&str> {
    // Pre-allocate based on comma count estimate
    let estimated_count = s.bytes().filter(|&b| b == b',').count() + 1;
    let mut result = Vec::with_capacity(estimated_count);
    let mut in_quote = false;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '\'' => in_quote = !in_quote,
            ',' if !in_quote => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    if start < s.len() {
        result.push(&s[start..]);
    }

    result
}

/// Parse Nested fields: "name Type, name2 Type2"
fn parse_nested_fields(s: &str) -> QueryResult<Vec<(std::string::String, ClickHouseSqlType)>> {
    let parts = split_toplevel_args(s);
    // Pre-allocate based on known parts count
    let mut result = Vec::with_capacity(parts.len());

    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Split "name Type" - find first space not inside parens
        let (name, type_str) = split_name_type(part).ok_or_else(|| {
            Error::TypeParseError(Cow::Owned(format!("invalid Nested field format: {part}")))
        })?;

        let ty = parse_type_inner(type_str)?;
        result.push((name.into(), ty));
    }

    Ok(result)
}

/// Split "name Type" into (name, type_str).
fn split_name_type(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ' ' if depth == 0 => {
                return Some((&s[..i], &s[i + 1..]));
            }
            _ => {}
        }
    }
    None
}

/// Helper to join iterator items with a separator, writing directly to output.
/// Avoids intermediate Vec allocation compared to `.collect::<Vec<_>>().join(sep)`.
fn join_to_string<I, F>(iter: I, sep: &str, mut to_string: F) -> std::string::String
where
    I: IntoIterator,
    F: FnMut(I::Item) -> std::string::String,
{
    let mut iter = iter.into_iter();
    let mut result = match iter.next() {
        Some(first) => to_string(first),
        None => return std::string::String::new(),
    };
    for item in iter {
        result.push_str(sep);
        result.push_str(&to_string(item));
    }
    result
}

impl ClickHouseSqlType {
    /// Returns true if this type is nullable.
    pub fn is_nullable(&self) -> bool {
        matches!(self, ClickHouseSqlType::Nullable(_))
    }

    /// Returns the inner type if nullable, otherwise returns self.
    pub fn inner_type(&self) -> &ClickHouseSqlType {
        match self {
            ClickHouseSqlType::Nullable(inner) => inner,
            ClickHouseSqlType::LowCardinality(inner) => inner.inner_type(),
            _ => self,
        }
    }

    /// Unwrap LowCardinality wrapper if present.
    pub fn unwrap_low_cardinality(&self) -> &ClickHouseSqlType {
        match self {
            ClickHouseSqlType::LowCardinality(inner) => inner.unwrap_low_cardinality(),
            _ => self,
        }
    }

    /// Returns the Rust type name for code generation.
    pub fn rust_type(&self) -> std::string::String {
        match self {
            ClickHouseSqlType::UInt8 => "u8".into(),
            ClickHouseSqlType::UInt16 => "u16".into(),
            ClickHouseSqlType::UInt32 => "u32".into(),
            ClickHouseSqlType::UInt64 => "u64".into(),
            ClickHouseSqlType::UInt128 => "u128".into(),
            ClickHouseSqlType::UInt256 => "U256".into(),
            ClickHouseSqlType::Int8 => "i8".into(),
            ClickHouseSqlType::Int16 => "i16".into(),
            ClickHouseSqlType::Int32 => "i32".into(),
            ClickHouseSqlType::Int64 => "i64".into(),
            ClickHouseSqlType::Int128 => "i128".into(),
            ClickHouseSqlType::Int256 => "I256".into(),
            ClickHouseSqlType::Bool => "bool".into(),
            ClickHouseSqlType::Float32 => "f32".into(),
            ClickHouseSqlType::Float64 => "f64".into(),
            ClickHouseSqlType::String => "String".into(),
            ClickHouseSqlType::FixedString(_) => "String".into(),
            ClickHouseSqlType::Date => "u16".into(), // days since epoch
            ClickHouseSqlType::Date32 => "i32".into(),
            ClickHouseSqlType::DateTime(_) => "u32".into(), // seconds since epoch
            ClickHouseSqlType::DateTime64(_, _) => "i64".into(),
            ClickHouseSqlType::IPv4 => "std::net::Ipv4Addr".into(),
            ClickHouseSqlType::IPv6 => "std::net::Ipv6Addr".into(),
            ClickHouseSqlType::UUID => "uuid::Uuid".into(),
            ClickHouseSqlType::Decimal(_, _)
            | ClickHouseSqlType::Decimal32(_)
            | ClickHouseSqlType::Decimal64(_)
            | ClickHouseSqlType::Decimal128(_)
            | ClickHouseSqlType::Decimal256(_) => "String".into(), // or custom Decimal type
            ClickHouseSqlType::Enum8(_) => "i8".into(),
            ClickHouseSqlType::Enum16(_) => "i16".into(),
            ClickHouseSqlType::Array(inner) => format!("Vec<{}>", inner.rust_type()),
            ClickHouseSqlType::Nullable(inner) => format!("Option<{}>", inner.rust_type()),
            ClickHouseSqlType::LowCardinality(inner) => inner.rust_type(),
            ClickHouseSqlType::Map(k, v) => {
                format!("std::collections::HashMap<{}, {}>", k.rust_type(), v.rust_type())
            }
            ClickHouseSqlType::Tuple(types) => {
                let inner = join_to_string(types.iter(), ", ", |t| t.rust_type());
                format!("({})", inner)
            }
            ClickHouseSqlType::Nested(_) => "Vec</* nested struct */>".into(),
            ClickHouseSqlType::JSON | ClickHouseSqlType::Object(_) => "serde_json::Value".into(),
            ClickHouseSqlType::Unknown(s) => format!("/* unknown: {s} */"),
        }
    }

    /// Returns the diesel-clickhouse-types type name for code generation.
    pub fn diesel_type(&self) -> std::string::String {
        match self {
            ClickHouseSqlType::UInt8 => "UInt8".into(),
            ClickHouseSqlType::UInt16 => "UInt16".into(),
            ClickHouseSqlType::UInt32 => "UInt32".into(),
            ClickHouseSqlType::UInt64 => "UInt64".into(),
            ClickHouseSqlType::UInt128 => "UInt128".into(),
            ClickHouseSqlType::UInt256 => "UInt256".into(),
            ClickHouseSqlType::Int8 => "Int8".into(),
            ClickHouseSqlType::Int16 => "Int16".into(),
            ClickHouseSqlType::Int32 => "Int32".into(),
            ClickHouseSqlType::Int64 => "Int64".into(),
            ClickHouseSqlType::Int128 => "Int128".into(),
            ClickHouseSqlType::Int256 => "Int256".into(),
            ClickHouseSqlType::Bool => "Bool".into(),
            ClickHouseSqlType::Float32 => "Float32".into(),
            ClickHouseSqlType::Float64 => "Float64".into(),
            ClickHouseSqlType::String => "CHString".into(),
            ClickHouseSqlType::FixedString(n) => format!("FixedString<{n}>"),
            ClickHouseSqlType::Date => "Date".into(),
            ClickHouseSqlType::Date32 => "Date32".into(),
            ClickHouseSqlType::DateTime(_) => "DateTime".into(),
            ClickHouseSqlType::DateTime64(prec, _) => format!("DateTime64<{prec}>"),
            ClickHouseSqlType::IPv4 => "IPv4".into(),
            ClickHouseSqlType::IPv6 => "IPv6".into(),
            ClickHouseSqlType::UUID => "UUID".into(),
            ClickHouseSqlType::Decimal(p, s) => format!("Decimal<{p}, {s}>"),
            ClickHouseSqlType::Decimal32(s) => format!("Decimal32<{s}>"),
            ClickHouseSqlType::Decimal64(s) => format!("Decimal64<{s}>"),
            ClickHouseSqlType::Decimal128(s) => format!("Decimal128<{s}>"),
            ClickHouseSqlType::Decimal256(s) => format!("Decimal256<{s}>"),
            ClickHouseSqlType::Enum8(_) => "Enum8".into(),
            ClickHouseSqlType::Enum16(_) => "Enum16".into(),
            ClickHouseSqlType::Array(inner) => format!("Array<{}>", inner.diesel_type()),
            ClickHouseSqlType::Nullable(inner) => format!("Nullable<{}>", inner.diesel_type()),
            ClickHouseSqlType::LowCardinality(inner) => {
                format!("LowCardinality<{}>", inner.diesel_type())
            }
            ClickHouseSqlType::Map(k, v) => {
                format!("Map<{}, {}>", k.diesel_type(), v.diesel_type())
            }
            ClickHouseSqlType::Tuple(types) => {
                let inner = join_to_string(types.iter(), ", ", |t| t.diesel_type());
                format!("Tuple<({})>", inner)
            }
            ClickHouseSqlType::Nested(fields) => {
                let inner = join_to_string(fields.iter(), ", ", |(_, t)| t.diesel_type());
                format!("Nested<({})>", inner)
            }
            ClickHouseSqlType::JSON | ClickHouseSqlType::Object(_) => "JSON".into(),
            ClickHouseSqlType::Unknown(s) => format!("/* unknown: {s} */"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_types() {
        assert_eq!(parse_type("UInt8").unwrap(), ClickHouseSqlType::UInt8);
        assert_eq!(parse_type("UInt64").unwrap(), ClickHouseSqlType::UInt64);
        assert_eq!(parse_type("Int32").unwrap(), ClickHouseSqlType::Int32);
        assert_eq!(parse_type("Float64").unwrap(), ClickHouseSqlType::Float64);
        assert_eq!(parse_type("String").unwrap(), ClickHouseSqlType::String);
        assert_eq!(parse_type("Bool").unwrap(), ClickHouseSqlType::Bool);
        assert_eq!(parse_type("Date").unwrap(), ClickHouseSqlType::Date);
        assert_eq!(parse_type("UUID").unwrap(), ClickHouseSqlType::UUID);
        assert_eq!(parse_type("IPv4").unwrap(), ClickHouseSqlType::IPv4);
    }

    #[test]
    fn test_parse_nullable() {
        assert_eq!(
            parse_type("Nullable(UInt64)").unwrap(),
            ClickHouseSqlType::Nullable(Box::new(ClickHouseSqlType::UInt64))
        );
        assert_eq!(
            parse_type("Nullable(String)").unwrap(),
            ClickHouseSqlType::Nullable(Box::new(ClickHouseSqlType::String))
        );
    }

    #[test]
    fn test_parse_array() {
        assert_eq!(
            parse_type("Array(UInt32)").unwrap(),
            ClickHouseSqlType::Array(Box::new(ClickHouseSqlType::UInt32))
        );
        assert_eq!(
            parse_type("Array(Nullable(String))").unwrap(),
            ClickHouseSqlType::Array(Box::new(ClickHouseSqlType::Nullable(Box::new(
                ClickHouseSqlType::String
            ))))
        );
    }

    #[test]
    fn test_parse_datetime() {
        assert_eq!(
            parse_type("DateTime").unwrap(),
            ClickHouseSqlType::DateTime(None)
        );
        assert_eq!(
            parse_type("DateTime('UTC')").unwrap(),
            ClickHouseSqlType::DateTime(Some("UTC".into()))
        );
        assert_eq!(
            parse_type("DateTime64(3)").unwrap(),
            ClickHouseSqlType::DateTime64(3, None)
        );
        assert_eq!(
            parse_type("DateTime64(3, 'UTC')").unwrap(),
            ClickHouseSqlType::DateTime64(3, Some("UTC".into()))
        );
    }

    #[test]
    fn test_parse_decimal() {
        assert_eq!(
            parse_type("Decimal(18, 4)").unwrap(),
            ClickHouseSqlType::Decimal(18, 4)
        );
        assert_eq!(
            parse_type("Decimal64(4)").unwrap(),
            ClickHouseSqlType::Decimal64(4)
        );
    }

    #[test]
    fn test_parse_fixed_string() {
        assert_eq!(
            parse_type("FixedString(32)").unwrap(),
            ClickHouseSqlType::FixedString(32)
        );
    }

    #[test]
    fn test_parse_map() {
        assert_eq!(
            parse_type("Map(String, UInt64)").unwrap(),
            ClickHouseSqlType::Map(
                Box::new(ClickHouseSqlType::String),
                Box::new(ClickHouseSqlType::UInt64)
            )
        );
    }

    #[test]
    fn test_parse_tuple() {
        assert_eq!(
            parse_type("Tuple(UInt64, String, Float32)").unwrap(),
            ClickHouseSqlType::Tuple(vec![
                ClickHouseSqlType::UInt64,
                ClickHouseSqlType::String,
                ClickHouseSqlType::Float32,
            ])
        );
    }

    #[test]
    fn test_parse_enum8() {
        let result = parse_type("Enum8('a' = 1, 'b' = 2)").unwrap();
        match result {
            ClickHouseSqlType::Enum8(variants) => {
                assert_eq!(variants.len(), 2);
                assert_eq!(variants[0], ("a".into(), 1));
                assert_eq!(variants[1], ("b".into(), 2));
            }
            _ => panic!("expected Enum8"),
        }
    }

    #[test]
    fn test_parse_low_cardinality() {
        assert_eq!(
            parse_type("LowCardinality(String)").unwrap(),
            ClickHouseSqlType::LowCardinality(Box::new(ClickHouseSqlType::String))
        );
    }

    #[test]
    fn test_parse_simple_aggregate_function() {
        // SimpleAggregateFunction(sum, UInt64) should extract UInt64
        assert_eq!(
            parse_type("SimpleAggregateFunction(sum, UInt64)").unwrap(),
            ClickHouseSqlType::UInt64
        );
    }

    #[test]
    fn test_parse_nested_complex() {
        // Complex nested type
        let result = parse_type("Array(Tuple(String, Nullable(UInt64)))").unwrap();
        assert_eq!(
            result,
            ClickHouseSqlType::Array(Box::new(ClickHouseSqlType::Tuple(vec![
                ClickHouseSqlType::String,
                ClickHouseSqlType::Nullable(Box::new(ClickHouseSqlType::UInt64)),
            ])))
        );
    }

    #[test]
    fn test_display_roundtrip() {
        let types = [
            "UInt64",
            "Nullable(String)",
            "Array(UInt32)",
            "Map(String, UInt64)",
            "DateTime64(3, 'UTC')",
            "Tuple(UInt64, String)",
        ];

        for type_str in types {
            let parsed = parse_type(type_str).unwrap();
            assert_eq!(parsed.to_string(), type_str, "roundtrip failed for {type_str}");
        }
    }

    #[test]
    fn test_rust_type() {
        assert_eq!(parse_type("UInt64").unwrap().rust_type(), "u64");
        assert_eq!(parse_type("Nullable(String)").unwrap().rust_type(), "Option<String>");
        assert_eq!(parse_type("Array(UInt32)").unwrap().rust_type(), "Vec<u32>");
    }

    #[test]
    fn test_diesel_type() {
        assert_eq!(parse_type("UInt64").unwrap().diesel_type(), "UInt64");
        assert_eq!(parse_type("Nullable(String)").unwrap().diesel_type(), "Nullable<CHString>");
        assert_eq!(parse_type("Array(UInt32)").unwrap().diesel_type(), "Array<UInt32>");
    }
}
