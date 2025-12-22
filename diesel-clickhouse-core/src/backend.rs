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
use smallvec::SmallVec;

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
    /// Push a bound value.
    fn push_bound_value<T: BindValue>(&mut self, value: &'a T) -> Result<(), crate::result::Error>;

    /// Push an unsized bound value (like str).
    fn push_bound_value_unsized<T: BindValue + ?Sized>(&mut self, value: &'a T) -> Result<(), crate::result::Error>;

    /// Get the collected bindings.
    fn bindings(&self) -> &[BoundValue<'a>];
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
}

impl<'a> Default for HttpBindCollector<'a> {
    fn default() -> Self {
        Self {
            bindings: SmallVec::new(),
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

    fn bindings(&self) -> &[BoundValue<'a>] {
        &self.bindings
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
}

impl<'a> Default for NativeBindCollector<'a> {
    fn default() -> Self {
        Self {
            bindings: SmallVec::new(),
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

    fn bindings(&self) -> &[BoundValue<'a>] {
        &self.bindings
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
}

impl<'a> Default for GenericBindCollector<'a> {
    fn default() -> Self {
        Self {
            bindings: SmallVec::new(),
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

    fn bindings(&self) -> &[BoundValue<'a>] {
        &self.bindings
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
