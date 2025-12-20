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
    /// Push a bound value.
    fn push_bound_value<T>(&mut self, value: &'a T) -> Result<(), crate::result::Error>
    where
        T: ?Sized;

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
#[derive(Debug, Default)]
pub struct HttpBindCollector<'a> {
    bindings: Vec<BoundValue<'a>>,
}

impl<'a> BindCollector<'a, HttpBackend> for HttpBindCollector<'a> {
    fn push_bound_value<T>(&mut self, _value: &'a T) -> Result<(), crate::result::Error>
    where
        T: ?Sized,
    {
        // HTTP backend typically uses inline values or query parameters
        // Full implementation would serialize the value here
        todo!("Implement value binding for HTTP backend")
    }

    fn bindings(&self) -> &[BoundValue<'a>] {
        &self.bindings
    }
}

/// Query builder for HTTP backend.
#[derive(Debug, Default)]
pub struct HttpQueryBuilder {
    sql: String,
    param_count: usize,
}

impl QueryBuilder for HttpQueryBuilder {
    fn push_sql(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    fn push_identifier(&mut self, identifier: &str) {
        // ClickHouse uses backticks for identifiers
        self.sql.push('`');
        // Escape backticks in the identifier
        for c in identifier.chars() {
            if c == '`' {
                self.sql.push_str("``");
            } else {
                self.sql.push(c);
            }
        }
        self.sql.push('`');
    }

    fn push_bind_param(&mut self) {
        // ClickHouse HTTP uses {name:Type} format for parameters
        // For simplicity, we use positional {p0:String} style
        self.sql.push_str(&format!("{{p{}:String}}", self.param_count));
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
#[derive(Debug, Default)]
pub struct NativeBindCollector<'a> {
    bindings: Vec<BoundValue<'a>>,
}

impl<'a> BindCollector<'a, NativeBackend> for NativeBindCollector<'a> {
    fn push_bound_value<T>(&mut self, _value: &'a T) -> Result<(), crate::result::Error>
    where
        T: ?Sized,
    {
        // Native backend uses binary protocol for parameters
        todo!("Implement value binding for Native backend")
    }

    fn bindings(&self) -> &[BoundValue<'a>] {
        &self.bindings
    }
}

/// Query builder for Native backend.
#[derive(Debug, Default)]
pub struct NativeQueryBuilder {
    sql: String,
    param_count: usize,
}

impl QueryBuilder for NativeQueryBuilder {
    fn push_sql(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    fn push_identifier(&mut self, identifier: &str) {
        // Same escaping as HTTP
        self.sql.push('`');
        for c in identifier.chars() {
            if c == '`' {
                self.sql.push_str("``");
            } else {
                self.sql.push(c);
            }
        }
        self.sql.push('`');
    }

    fn push_bind_param(&mut self) {
        // Native protocol uses $1, $2, etc.
        self.param_count += 1;
        self.sql.push_str(&format!("${}", self.param_count));
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
#[derive(Debug, Default)]
pub struct GenericBindCollector<'a> {
    bindings: Vec<BoundValue<'a>>,
}

impl<'a> BindCollector<'a, ClickHouse> for GenericBindCollector<'a> {
    fn push_bound_value<T>(&mut self, _value: &'a T) -> Result<(), crate::result::Error>
    where
        T: ?Sized,
    {
        todo!("Implement generic value binding")
    }

    fn bindings(&self) -> &[BoundValue<'a>] {
        &self.bindings
    }
}

/// Generic query builder.
#[derive(Debug, Default)]
pub struct GenericQueryBuilder {
    sql: String,
}

impl QueryBuilder for GenericQueryBuilder {
    fn push_sql(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    fn push_identifier(&mut self, identifier: &str) {
        self.sql.push('`');
        for c in identifier.chars() {
            if c == '`' {
                self.sql.push_str("``");
            } else {
                self.sql.push(c);
            }
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
