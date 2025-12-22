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
        match self {
            BindableValue::U8(v) => v.to_string(),
            BindableValue::U16(v) => v.to_string(),
            BindableValue::U32(v) => v.to_string(),
            BindableValue::U64(v) => v.to_string(),
            BindableValue::I8(v) => v.to_string(),
            BindableValue::I16(v) => v.to_string(),
            BindableValue::I32(v) => v.to_string(),
            BindableValue::I64(v) => v.to_string(),
            BindableValue::F32(v) => v.to_string(),
            BindableValue::F64(v) => v.to_string(),
            BindableValue::Bool(v) => if *v { "true" } else { "false" }.to_string(),
            BindableValue::String(v) => {
                let escaped = v.replace('\'', "''");
                format!("'{}'", escaped)
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

/// Trait for values that can be bound as query parameters.
pub trait BindValue {
    /// The ClickHouse type name for this value.
    fn type_name(&self) -> &'static str;

    /// Serialize the value to bytes.
    fn to_bytes(&self) -> Vec<u8>;

    /// Serialize the value to SQL literal string.
    fn to_sql_literal(&self) -> String;
}

// Implement BindValue for integer types using itoa for fast formatting
macro_rules! impl_bind_value_int {
    ($($t:ty => $name:literal),*) => {
        $(
            impl BindValue for $t {
                fn type_name(&self) -> &'static str { $name }
                fn to_bytes(&self) -> Vec<u8> { self.to_le_bytes().to_vec() }
                fn to_sql_literal(&self) -> String {
                    let mut buf = itoa::Buffer::new();
                    buf.format(*self).to_owned()
                }
            }
        )*
    };
}

// Implement BindValue for float types using ryu for fast formatting
macro_rules! impl_bind_value_float {
    ($($t:ty => $name:literal),*) => {
        $(
            impl BindValue for $t {
                fn type_name(&self) -> &'static str { $name }
                fn to_bytes(&self) -> Vec<u8> { self.to_le_bytes().to_vec() }
                fn to_sql_literal(&self) -> String {
                    let mut buf = ryu::Buffer::new();
                    buf.format_finite(*self).to_owned()
                }
            }
        )*
    };
}

impl_bind_value_int!(
    u8 => "UInt8", u16 => "UInt16", u32 => "UInt32", u64 => "UInt64",
    i8 => "Int8", i16 => "Int16", i32 => "Int32", i64 => "Int64"
);

impl_bind_value_float!(
    f32 => "Float32", f64 => "Float64"
);

impl BindValue for bool {
    fn type_name(&self) -> &'static str { "Bool" }
    fn to_bytes(&self) -> Vec<u8> { vec![if *self { 1 } else { 0 }] }
    fn to_sql_literal(&self) -> String { (if *self { "true" } else { "false" }).to_owned() }
}

impl BindValue for str {
    fn type_name(&self) -> &'static str { "String" }
    fn to_bytes(&self) -> Vec<u8> { self.as_bytes().to_vec() }
    fn to_sql_literal(&self) -> String {
        // Pre-allocate: original length + 2 quotes + potential escapes
        let mut result = String::with_capacity(self.len() + 2);
        result.push('\'');
        if self.contains('\'') {
            result.push_str(&self.replace('\'', "''"));
        } else {
            result.push_str(self);
        }
        result.push('\'');
        result
    }
}

impl BindValue for String {
    fn type_name(&self) -> &'static str { "String" }
    fn to_bytes(&self) -> Vec<u8> { self.as_bytes().to_vec() }
    fn to_sql_literal(&self) -> String {
        self.as_str().to_sql_literal()
    }
}

impl<T: BindValue> BindValue for &T {
    fn type_name(&self) -> &'static str { (*self).type_name() }
    fn to_bytes(&self) -> Vec<u8> { (*self).to_bytes() }
    fn to_sql_literal(&self) -> String { (*self).to_sql_literal() }
}

/// Trait for collecting bound parameters.
pub trait BindCollector<'a, DB: Backend>: Default {
    /// Push a bound value (legacy - for backward compatibility).
    fn push_bound_value<T: BindValue>(&mut self, value: &'a T) -> Result<(), crate::result::Error>;

    /// Push an unsized bound value (like str).
    fn push_bound_value_unsized<T: BindValue + ?Sized>(&mut self, value: &'a T) -> Result<(), crate::result::Error>;

    /// Push a bindable value for native parameter binding.
    fn push_bindable_value(&mut self, value: BindableValue) -> Result<(), crate::result::Error>;

    /// Get the collected bindings (legacy format).
    fn bindings(&self) -> &[BoundValue<'a>];

    /// Get the collected bindable values for native binding.
    fn bindable_values(&self) -> &[BindableValue];
}

/// A bound parameter value.
#[derive(Debug, Clone)]
pub struct BoundValue<'a> {
    /// The serialized bytes of the value.
    pub bytes: std::borrow::Cow<'a, [u8]>,
    /// The ClickHouse type name.
    pub type_name: &'static str,
    /// SQL literal representation.
    pub sql_literal: String,
    /// Phantom lifetime.
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> BoundValue<'a> {
    /// Create a new bound value.
    pub fn new<T: BindValue + ?Sized>(value: &T) -> Self {
        Self {
            bytes: std::borrow::Cow::Owned(value.to_bytes()),
            type_name: value.type_name(),
            sql_literal: value.to_sql_literal(),
            _phantom: std::marker::PhantomData,
        }
    }
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
    type BindCollector<'a> = HttpBindCollector<'a>;
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
/// Uses SmallVec to store up to 8 bindings on the stack without allocation.
#[derive(Debug)]
pub struct HttpBindCollector<'a> {
    bindings: SmallVec<[BoundValue<'a>; 8]>,
    bindable_values: SmallVec<[BindableValue; 8]>,
}

impl<'a> Default for HttpBindCollector<'a> {
    fn default() -> Self {
        Self {
            bindings: SmallVec::new(),
            bindable_values: SmallVec::new(),
        }
    }
}

impl<'a> BindCollector<'a, HttpBackend> for HttpBindCollector<'a> {
    fn push_bound_value<T: BindValue>(&mut self, value: &'a T) -> Result<(), crate::result::Error> {
        self.bindings.push(BoundValue::new(value));
        Ok(())
    }

    fn push_bound_value_unsized<T: BindValue + ?Sized>(&mut self, value: &'a T) -> Result<(), crate::result::Error> {
        self.bindings.push(BoundValue::new(value));
        Ok(())
    }

    fn push_bindable_value(&mut self, value: BindableValue) -> Result<(), crate::result::Error> {
        self.bindable_values.push(value);
        Ok(())
    }

    fn bindings(&self) -> &[BoundValue<'a>] {
        &self.bindings
    }

    fn bindable_values(&self) -> &[BindableValue] {
        &self.bindable_values
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
            // Pre-allocate for typical query size
            sql: String::with_capacity(256),
            param_count: 0,
        }
    }
}

impl QueryBuilder for HttpQueryBuilder {
    fn push_sql(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    fn push_identifier(&mut self, identifier: &str) {
        // ClickHouse uses backticks for identifiers
        self.sql.push('`');
        // Fast path: no escaping needed if no backticks
        if identifier.contains('`') {
            for c in identifier.chars() {
                if c == '`' {
                    self.sql.push_str("``");
                } else {
                    self.sql.push(c);
                }
            }
        } else {
            self.sql.push_str(identifier);
        }
        self.sql.push('`');
    }

    fn push_bind_param(&mut self) {
        // ClickHouse HTTP uses {name:Type} format for parameters
        // Build without format! macro for efficiency
        self.sql.push_str("{p");
        let mut buf = itoa::Buffer::new();
        self.sql.push_str(buf.format(self.param_count));
        self.sql.push_str(":String}");
        self.param_count += 1;
    }

    fn finish(self) -> String {
        self.sql
    }

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
    type BindCollector<'a> = NativeBindCollector<'a>;
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
/// Uses SmallVec to store up to 8 bindings on the stack without allocation.
#[derive(Debug)]
pub struct NativeBindCollector<'a> {
    bindings: SmallVec<[BoundValue<'a>; 8]>,
    bindable_values: SmallVec<[BindableValue; 8]>,
}

impl<'a> Default for NativeBindCollector<'a> {
    fn default() -> Self {
        Self {
            bindings: SmallVec::new(),
            bindable_values: SmallVec::new(),
        }
    }
}

impl<'a> BindCollector<'a, NativeBackend> for NativeBindCollector<'a> {
    fn push_bound_value<T: BindValue>(&mut self, value: &'a T) -> Result<(), crate::result::Error> {
        self.bindings.push(BoundValue::new(value));
        Ok(())
    }

    fn push_bound_value_unsized<T: BindValue + ?Sized>(&mut self, value: &'a T) -> Result<(), crate::result::Error> {
        self.bindings.push(BoundValue::new(value));
        Ok(())
    }

    fn push_bindable_value(&mut self, value: BindableValue) -> Result<(), crate::result::Error> {
        self.bindable_values.push(value);
        Ok(())
    }

    fn bindings(&self) -> &[BoundValue<'a>] {
        &self.bindings
    }

    fn bindable_values(&self) -> &[BindableValue] {
        &self.bindable_values
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
            sql: String::with_capacity(256),
            param_count: 0,
        }
    }
}

impl QueryBuilder for NativeQueryBuilder {
    fn push_sql(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    fn push_identifier(&mut self, identifier: &str) {
        self.sql.push('`');
        if identifier.contains('`') {
            for c in identifier.chars() {
                if c == '`' {
                    self.sql.push_str("``");
                } else {
                    self.sql.push(c);
                }
            }
        } else {
            self.sql.push_str(identifier);
        }
        self.sql.push('`');
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
    type BindCollector<'a> = GenericBindCollector<'a>;
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
/// Uses SmallVec to store up to 8 bindings on the stack without allocation.
#[derive(Debug)]
pub struct GenericBindCollector<'a> {
    bindings: SmallVec<[BoundValue<'a>; 8]>,
    bindable_values: SmallVec<[BindableValue; 8]>,
}

impl<'a> Default for GenericBindCollector<'a> {
    fn default() -> Self {
        Self {
            bindings: SmallVec::new(),
            bindable_values: SmallVec::new(),
        }
    }
}

impl<'a> BindCollector<'a, ClickHouse> for GenericBindCollector<'a> {
    fn push_bound_value<T: BindValue>(&mut self, value: &'a T) -> Result<(), crate::result::Error> {
        self.bindings.push(BoundValue::new(value));
        Ok(())
    }

    fn push_bound_value_unsized<T: BindValue + ?Sized>(&mut self, value: &'a T) -> Result<(), crate::result::Error> {
        self.bindings.push(BoundValue::new(value));
        Ok(())
    }

    fn push_bindable_value(&mut self, value: BindableValue) -> Result<(), crate::result::Error> {
        self.bindable_values.push(value);
        Ok(())
    }

    fn bindings(&self) -> &[BoundValue<'a>] {
        &self.bindings
    }

    fn bindable_values(&self) -> &[BindableValue] {
        &self.bindable_values
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
            sql: String::with_capacity(256),
        }
    }
}

impl QueryBuilder for GenericQueryBuilder {
    fn push_sql(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    fn push_identifier(&mut self, identifier: &str) {
        self.sql.push('`');
        if identifier.contains('`') {
            for c in identifier.chars() {
                if c == '`' {
                    self.sql.push_str("``");
                } else {
                    self.sql.push(c);
                }
            }
        } else {
            self.sql.push_str(identifier);
        }
        self.sql.push('`');
    }

    fn push_bind_param(&mut self) {
        self.sql.push('?');
    }

    fn finish(self) -> String {
        self.sql
    }

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
