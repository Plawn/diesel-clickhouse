//! Backend abstraction for ClickHouse connections.
//!
//! diesel-clickhouse supports two protocols for connecting to ClickHouse:
//!
//! - **HTTP** (`HttpBackend`): Uses the ClickHouse HTTP interface via the `clickhouse` crate.
//!   Simpler to set up, better TLS support, works through proxies.
//!
//! - **Native** (`NativeBackend`): Uses the native ClickHouse protocol via `klickhouse`.
//!   Higher performance for large data transfers, lower overhead.
//!
//! The [`Backend`] trait abstracts over these, allowing generic code to work
//! with either protocol.

use std::fmt::Debug;
use serde::Serialize;
use smallvec::SmallVec;

// =============================================================================
// Bindable Value - Type-erased value for native parameter binding
// =============================================================================

/// A type-erased value that can be bound to a clickhouse Query.
///
/// This enum holds the actual typed value, allowing us to call `.bind()` with
/// the correct type at execution time. This enables proper parameter binding
/// and query plan caching on the ClickHouse server.
#[derive(Debug, Clone)]
pub enum BindableValue {
    // Unsigned integers
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    // Signed integers
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    // Floats
    F32(f32),
    F64(f64),
    // Boolean
    Bool(bool),
    // String (owned for lifetime safety)
    String(String),
}

impl BindableValue {
    /// Get the ClickHouse type name for this value.
    pub fn type_name(&self) -> &'static str {
        match self {
            BindableValue::U8(_) => "UInt8",
            BindableValue::U16(_) => "UInt16",
            BindableValue::U32(_) => "UInt32",
            BindableValue::U64(_) => "UInt64",
            BindableValue::I8(_) => "Int8",
            BindableValue::I16(_) => "Int16",
            BindableValue::I32(_) => "Int32",
            BindableValue::I64(_) => "Int64",
            BindableValue::F32(_) => "Float32",
            BindableValue::F64(_) => "Float64",
            BindableValue::Bool(_) => "Bool",
            BindableValue::String(_) => "String",
        }
    }

    /// Get the SQL literal representation (for debugging/logging).
    pub fn sql_literal(&self) -> String {
        let mut buf = String::new();
        self.write_sql_literal(&mut buf);
        buf
    }

    /// Write the SQL literal representation directly to a buffer.
    ///
    /// This avoids allocations by writing directly to the output buffer
    /// instead of creating intermediate strings.
    #[inline]
    pub fn write_sql_literal(&self, buf: &mut String) {
        match self {
            BindableValue::U8(v) => {
                let mut tmp = itoa::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::U16(v) => {
                let mut tmp = itoa::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::U32(v) => {
                let mut tmp = itoa::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::U64(v) => {
                let mut tmp = itoa::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::I8(v) => {
                let mut tmp = itoa::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::I16(v) => {
                let mut tmp = itoa::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::I32(v) => {
                let mut tmp = itoa::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::I64(v) => {
                let mut tmp = itoa::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::F32(v) => {
                let mut tmp = ryu::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::F64(v) => {
                let mut tmp = ryu::Buffer::new();
                buf.push_str(tmp.format(*v));
            }
            BindableValue::Bool(v) => {
                buf.push_str(if *v { "true" } else { "false" });
            }
            BindableValue::String(v) => {
                buf.push('\'');
                // Escape single quotes by doubling them
                for ch in v.chars() {
                    if ch == '\'' {
                        buf.push_str("''");
                    } else {
                        buf.push(ch);
                    }
                }
                buf.push('\'');
            }
        }
    }
}

// Implement Serialize for BindableValue so it can be used with clickhouse's .bind()
impl Serialize for BindableValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            BindableValue::U8(v) => serializer.serialize_u8(*v),
            BindableValue::U16(v) => serializer.serialize_u16(*v),
            BindableValue::U32(v) => serializer.serialize_u32(*v),
            BindableValue::U64(v) => serializer.serialize_u64(*v),
            BindableValue::I8(v) => serializer.serialize_i8(*v),
            BindableValue::I16(v) => serializer.serialize_i16(*v),
            BindableValue::I32(v) => serializer.serialize_i32(*v),
            BindableValue::I64(v) => serializer.serialize_i64(*v),
            BindableValue::F32(v) => serializer.serialize_f32(*v),
            BindableValue::F64(v) => serializer.serialize_f64(*v),
            BindableValue::Bool(v) => serializer.serialize_bool(*v),
            BindableValue::String(v) => serializer.serialize_str(v),
        }
    }
}

/// Trait for converting a value to BindableValue.
pub trait ToBindableValue {
    fn to_bindable_value(&self) -> BindableValue;
}

macro_rules! impl_to_bindable {
    ($($t:ty => $variant:ident),*) => {
        $(
            impl ToBindableValue for $t {
                fn to_bindable_value(&self) -> BindableValue {
                    BindableValue::$variant(*self)
                }
            }
        )*
    };
}

impl_to_bindable!(
    u8 => U8, u16 => U16, u32 => U32, u64 => U64,
    i8 => I8, i16 => I16, i32 => I32, i64 => I64,
    f32 => F32, f64 => F64, bool => Bool
);

impl ToBindableValue for String {
    fn to_bindable_value(&self) -> BindableValue {
        BindableValue::String(self.clone())
    }
}

impl ToBindableValue for str {
    fn to_bindable_value(&self) -> BindableValue {
        BindableValue::String(self.to_owned())
    }
}

impl ToBindableValue for &str {
    fn to_bindable_value(&self) -> BindableValue {
        BindableValue::String((*self).to_owned())
    }
}

/// Core backend trait for ClickHouse connections.
///
/// This trait abstracts over the HTTP and Native protocols, allowing
/// generic query building and execution code.
pub trait Backend: Sized + Send + Sync + Debug + Clone + Copy + 'static {
    /// The raw value type returned by this backend.
    type RawValue<'a>: Debug;

    /// The bind collector type for this backend.
    type BindCollector<'a>: BindCollector<'a, Self>;

    /// The query builder type for this backend.
    type QueryBuilder: QueryBuilder;

    /// Returns the backend name for debugging.
    fn name() -> &'static str;
}

/// Trait for collecting bound parameters.
pub trait BindCollector<'a, DB: Backend>: Default {
    /// Push a bindable value for native parameter binding.
    fn push_bindable_value(&mut self, value: BindableValue) -> Result<(), crate::result::Error>;

    /// Get the collected bindable values for native binding.
    fn bindable_values(&self) -> &[BindableValue];
}

/// Trait for building SQL query strings.
pub trait QueryBuilder: Default {
    /// Push a SQL fragment.
    fn push_sql(&mut self, sql: &str);

    /// Push an identifier (table or column name).
    fn push_identifier(&mut self, identifier: &str);

    /// Push a bind marker for a parameter.
    fn push_bind_param(&mut self);

    /// Finish building and return the SQL string.
    fn finish(self) -> String;

    /// Get the current SQL string (for debugging).
    fn sql(&self) -> &str;
}

// =============================================================================
// Common helpers to avoid duplication
// =============================================================================

/// Default capacity for SQL query strings.
const DEFAULT_SQL_CAPACITY: usize = 256;

/// Push an escaped identifier to a SQL string.
/// ClickHouse uses backticks for identifiers, and backticks within
/// identifiers are escaped by doubling them.
#[inline]
fn push_escaped_identifier(sql: &mut String, identifier: &str) {
    sql.push('`');
    if identifier.contains('`') {
        for c in identifier.chars() {
            if c == '`' {
                sql.push_str("``");
            } else {
                sql.push(c);
            }
        }
    } else {
        sql.push_str(identifier);
    }
    sql.push('`');
}

/// Common bind collector implementation.
///
/// This struct provides the shared implementation for all backend-specific
/// bind collectors, avoiding code duplication.
#[derive(Debug, Default)]
pub struct CommonBindCollector {
    bindable_values: SmallVec<[BindableValue; 8]>,
}

impl CommonBindCollector {
    /// Push a bindable value for native parameter binding.
    #[inline]
    pub fn push_bindable_value(&mut self, value: BindableValue) -> Result<(), crate::result::Error> {
        self.bindable_values.push(value);
        Ok(())
    }

    /// Get the collected bindable values for native binding.
    #[inline]
    pub fn bindable_values(&self) -> &[BindableValue] {
        &self.bindable_values
    }
}

// =============================================================================
// HTTP Backend
// =============================================================================

/// HTTP protocol backend.
///
/// Uses the ClickHouse HTTP interface, which is:
/// - Easier to configure and debug
/// - Works through HTTP proxies
/// - Has better TLS support
/// - Slightly higher overhead per request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HttpBackend;

impl Backend for HttpBackend {
    type RawValue<'a> = HttpRawValue<'a>;
    type BindCollector<'a> = HttpBindCollector;
    type QueryBuilder = HttpQueryBuilder;

    fn name() -> &'static str {
        "ClickHouse HTTP"
    }
}

/// Raw value type for HTTP backend.
#[derive(Debug)]
pub struct HttpRawValue<'a> {
    /// The raw bytes from the response.
    pub bytes: &'a [u8],
    /// The column type.
    pub type_name: &'a str,
}

/// Bind collector for HTTP backend.
///
/// Wraps CommonBindCollector to provide HTTP-specific BindCollector implementation.
#[derive(Debug, Default)]
pub struct HttpBindCollector(CommonBindCollector);

impl<'a> BindCollector<'a, HttpBackend> for HttpBindCollector {
    #[inline]
    fn push_bindable_value(&mut self, value: BindableValue) -> Result<(), crate::result::Error> {
        self.0.push_bindable_value(value)
    }

    #[inline]
    fn bindable_values(&self) -> &[BindableValue] {
        self.0.bindable_values()
    }
}

/// Query builder for HTTP backend.
#[derive(Debug)]
pub struct HttpQueryBuilder {
    sql: String,
    param_count: usize,
}

impl Default for HttpQueryBuilder {
    fn default() -> Self {
        Self {
            sql: String::with_capacity(DEFAULT_SQL_CAPACITY),
            param_count: 0,
        }
    }
}

impl QueryBuilder for HttpQueryBuilder {
    #[inline]
    fn push_sql(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    #[inline]
    fn push_identifier(&mut self, identifier: &str) {
        push_escaped_identifier(&mut self.sql, identifier);
    }

    fn push_bind_param(&mut self) {
        // ClickHouse HTTP uses {name:Type} format for parameters
        self.sql.push_str("{p");
        let mut buf = itoa::Buffer::new();
        self.sql.push_str(buf.format(self.param_count));
        self.sql.push_str(":String}");
        self.param_count += 1;
    }

    fn finish(self) -> String {
        self.sql
    }

    #[inline]
    fn sql(&self) -> &str {
        &self.sql
    }
}

// =============================================================================
// Native Backend
// =============================================================================

/// Native protocol backend.
///
/// Uses the ClickHouse native binary protocol, which:
/// - Has lower overhead for large data transfers
/// - Supports streaming inserts
/// - Has better compression support
/// - Requires direct TCP connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NativeBackend;

impl Backend for NativeBackend {
    type RawValue<'a> = NativeRawValue<'a>;
    type BindCollector<'a> = NativeBindCollector;
    type QueryBuilder = NativeQueryBuilder;

    fn name() -> &'static str {
        "ClickHouse Native"
    }
}

/// Raw value type for Native backend.
#[derive(Debug)]
pub struct NativeRawValue<'a> {
    /// The raw bytes from the response.
    pub bytes: &'a [u8],
    /// The column index.
    pub column_index: usize,
}

/// Bind collector for Native backend.
///
/// Wraps CommonBindCollector to provide Native-specific BindCollector implementation.
#[derive(Debug, Default)]
pub struct NativeBindCollector(CommonBindCollector);

impl<'a> BindCollector<'a, NativeBackend> for NativeBindCollector {
    #[inline]
    fn push_bindable_value(&mut self, value: BindableValue) -> Result<(), crate::result::Error> {
        self.0.push_bindable_value(value)
    }

    #[inline]
    fn bindable_values(&self) -> &[BindableValue] {
        self.0.bindable_values()
    }
}

/// Query builder for Native backend.
#[derive(Debug)]
pub struct NativeQueryBuilder {
    sql: String,
    param_count: usize,
}

impl Default for NativeQueryBuilder {
    fn default() -> Self {
        Self {
            sql: String::with_capacity(DEFAULT_SQL_CAPACITY),
            param_count: 0,
        }
    }
}

impl QueryBuilder for NativeQueryBuilder {
    #[inline]
    fn push_sql(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    #[inline]
    fn push_identifier(&mut self, identifier: &str) {
        push_escaped_identifier(&mut self.sql, identifier);
    }

    fn push_bind_param(&mut self) {
        // Native protocol uses $1, $2, etc.
        self.param_count += 1;
        self.sql.push('$');
        let mut buf = itoa::Buffer::new();
        self.sql.push_str(buf.format(self.param_count));
    }

    fn finish(self) -> String {
        self.sql
    }

    #[inline]
    fn sql(&self) -> &str {
        &self.sql
    }
}

// =============================================================================
// Generic ClickHouse backend (for backend-agnostic code)
// =============================================================================

/// Generic ClickHouse backend marker.
///
/// This can be used in generic code that doesn't care about the
/// specific protocol being used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClickHouse;

impl Backend for ClickHouse {
    type RawValue<'a> = GenericRawValue<'a>;
    type BindCollector<'a> = GenericBindCollector;
    type QueryBuilder = GenericQueryBuilder;

    fn name() -> &'static str {
        "ClickHouse"
    }
}

/// Generic raw value (for backend-agnostic code).
#[derive(Debug)]
pub struct GenericRawValue<'a> {
    pub bytes: &'a [u8],
}

/// Generic bind collector.
///
/// Wraps CommonBindCollector to provide generic BindCollector implementation.
#[derive(Debug, Default)]
pub struct GenericBindCollector(CommonBindCollector);

impl<'a> BindCollector<'a, ClickHouse> for GenericBindCollector {
    #[inline]
    fn push_bindable_value(&mut self, value: BindableValue) -> Result<(), crate::result::Error> {
        self.0.push_bindable_value(value)
    }

    #[inline]
    fn bindable_values(&self) -> &[BindableValue] {
        self.0.bindable_values()
    }
}

/// Generic query builder.
#[derive(Debug)]
pub struct GenericQueryBuilder {
    sql: String,
}

impl Default for GenericQueryBuilder {
    fn default() -> Self {
        Self {
            sql: String::with_capacity(DEFAULT_SQL_CAPACITY),
        }
    }
}

impl QueryBuilder for GenericQueryBuilder {
    #[inline]
    fn push_sql(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    #[inline]
    fn push_identifier(&mut self, identifier: &str) {
        push_escaped_identifier(&mut self.sql, identifier);
    }

    #[inline]
    fn push_bind_param(&mut self) {
        self.sql.push('?');
    }

    fn finish(self) -> String {
        self.sql
    }

    #[inline]
    fn sql(&self) -> &str {
        &self.sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_query_builder() {
        let mut builder = HttpQueryBuilder::default();
        builder.push_sql("SELECT ");
        builder.push_identifier("user_id");
        builder.push_sql(" FROM ");
        builder.push_identifier("events");
        builder.push_sql(" WHERE ");
        builder.push_identifier("timestamp");
        builder.push_sql(" > ");
        builder.push_bind_param();

        let sql = builder.finish();
        assert_eq!(sql, "SELECT `user_id` FROM `events` WHERE `timestamp` > {p0:String}");
    }

    #[test]
    fn test_native_query_builder() {
        let mut builder = NativeQueryBuilder::default();
        builder.push_sql("SELECT * FROM ");
        builder.push_identifier("users");
        builder.push_sql(" WHERE id = ");
        builder.push_bind_param();

        let sql = builder.finish();
        assert_eq!(sql, "SELECT * FROM `users` WHERE id = $1");
    }

    #[test]
    fn test_identifier_escaping() {
        let mut builder = HttpQueryBuilder::default();
        builder.push_identifier("weird`name");
        let sql = builder.finish();
        assert_eq!(sql, "`weird``name`");
    }
}
