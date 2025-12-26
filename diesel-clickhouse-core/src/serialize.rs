//! Serialization traits for inserting data.

use compact_str::CompactString;

use crate::backend::Backend;
use crate::query_builder::AstPass;
use crate::result::QueryResult;
use diesel_clickhouse_types::{SqlType, ToClickHouse};

/// Trait for types that can be serialized as a row for insertion.
pub trait ToRow {
    /// Get the column names for this row type.
    fn column_names() -> &'static [&'static str];

    /// Serialize the values to bytes.
    fn to_row_bytes(&self, out: &mut Vec<u8>) -> QueryResult<()>;
}

/// A single value that can be bound as a parameter.
pub trait ToSql<ST: SqlType> {
    /// Serialize to SQL format.
    fn to_sql(&self, out: &mut Vec<u8>) -> QueryResult<()>;
}

// Blanket implementation for types that implement ToClickHouse
impl<T, ST> ToSql<ST> for T
where
    ST: SqlType,
    T: ToClickHouse<ST>,
{
    fn to_sql(&self, out: &mut Vec<u8>) -> QueryResult<()> {
        self.to_clickhouse(out).map_err(Into::into)
    }
}

// =============================================================================
// SQL Value Writing (for INSERT statements)
// =============================================================================

/// Trait for writing SQL values using native parameter binding.
///
/// This uses `push_bindable` for proper parameter binding, enabling:
/// - Query plan caching on the ClickHouse server
/// - Type-safe value serialization
/// - Protection against SQL injection
pub trait WriteSqlValue {
    /// Write the value to the AST pass using native binding.
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()>;
}

/// Write a value using native binding to an AstPass.
pub fn write_sql_value<T: WriteSqlValue, DB: Backend>(value: &T, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
    value.write_sql(pass)
}

/// Write a value as bytes (for binary protocol).
pub fn write_sql_bytes<T>(_value: &T, _out: &mut Vec<u8>) {
    // Binary serialization - not used for SQL generation
}

// Implement WriteSqlValue for common types using native binding
macro_rules! impl_write_sql_value_bindable {
    ($($t:ty),*) => {
        $(
            impl WriteSqlValue for $t {
                fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
                    pass.push_bindable(self)
                }
            }
        )*
    };
}

impl_write_sql_value_bindable!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, bool);

impl WriteSqlValue for String {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        pass.push_bindable(self)
    }
}

impl WriteSqlValue for str {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        pass.push_bindable(&self.to_string())
    }
}

impl WriteSqlValue for &str {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        pass.push_bindable(&self.to_string())
    }
}

/// CompactString: inline string (up to 24 bytes on stack, heap otherwise).
/// Zero allocation for small strings like column names, short values, etc.
impl WriteSqlValue for CompactString {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        pass.push_bindable(&self.to_string())
    }
}

impl<T: WriteSqlValue> WriteSqlValue for Option<T> {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        match self {
            Some(value) => value.write_sql(pass),
            None => {
                pass.push_sql("NULL");
                Ok(())
            }
        }
    }
}

impl<T: WriteSqlValue> WriteSqlValue for &T {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        (*self).write_sql(pass)
    }
}

// =============================================================================
// JSON type support (ClickHouse 24.10+)
// =============================================================================

/// WriteSqlValue implementation for serde_json::Value.
/// JSON values are serialized to string for SQL generation.
#[cfg(feature = "json")]
impl WriteSqlValue for serde_json::Value {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        let json_str = serde_json::to_string(self)
            .map_err(|e| crate::result::Error::SerializationError(
                std::borrow::Cow::Owned(format!("Failed to serialize JSON: {}", e))
            ))?;
        pass.push_bindable(&json_str)
    }
}

/// WriteSqlValue implementation for JsonTyped<T>.
/// JSON values are serialized to string for SQL generation.
#[cfg(feature = "json")]
impl<T: serde::Serialize> WriteSqlValue for diesel_clickhouse_types::JsonTyped<T> {
    fn write_sql<DB: Backend>(&self, pass: &mut AstPass<'_, '_, DB>) -> QueryResult<()> {
        let json_str = serde_json::to_string(&self.0)
            .map_err(|e| crate::result::Error::SerializationError(
                std::borrow::Cow::Owned(format!("Failed to serialize JSON: {}", e))
            ))?;
        pass.push_bindable(&json_str)
    }
}

/// A collection of values for a single row.
#[derive(Debug, Default)]
pub struct RowValues {
    columns: Vec<&'static str>,
    values: Vec<Vec<u8>>,
}

impl RowValues {
    /// Create a new row values collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new row values collection with pre-allocated capacity.
    ///
    /// Use this when you know the number of columns in advance to avoid
    /// reallocations.
    pub fn with_capacity(num_columns: usize) -> Self {
        Self {
            columns: Vec::with_capacity(num_columns),
            values: Vec::with_capacity(num_columns),
        }
    }

    /// Add a value.
    pub fn add<T, ST>(&mut self, column: &'static str, value: &T) -> QueryResult<()>
    where
        ST: SqlType,
        T: ToSql<ST>,
    {
        // Pre-allocate with reasonable initial capacity for value bytes
        let mut bytes = Vec::with_capacity(32);
        value.to_sql(&mut bytes)?;
        self.columns.push(column);
        self.values.push(bytes);
        Ok(())
    }

    /// Get the column names.
    pub fn columns(&self) -> &[&'static str] {
        &self.columns
    }

    /// Get the values.
    pub fn values(&self) -> &[Vec<u8>] {
        &self.values
    }
}

// =============================================================================
// Primitive ToRow implementations
// =============================================================================

macro_rules! impl_to_row_primitive {
    ($rust_ty:ty, $sql_ty:ty) => {
        impl ToRow for $rust_ty {
            fn column_names() -> &'static [&'static str] {
                &["value"]
            }

            fn to_row_bytes(&self, out: &mut Vec<u8>) -> QueryResult<()> {
                <Self as ToSql<$sql_ty>>::to_sql(self, out)
            }
        }
    };
}

use diesel_clickhouse_types::*;

impl_to_row_primitive!(u8, UInt8);
impl_to_row_primitive!(u16, UInt16);
impl_to_row_primitive!(u32, UInt32);
impl_to_row_primitive!(u64, UInt64);
impl_to_row_primitive!(i8, Int8);
impl_to_row_primitive!(i16, Int16);
impl_to_row_primitive!(i32, Int32);
impl_to_row_primitive!(i64, Int64);
impl_to_row_primitive!(f32, Float32);
impl_to_row_primitive!(f64, Float64);
impl_to_row_primitive!(bool, Bool);
impl_to_row_primitive!(String, CHString);

// =============================================================================
// Tuple ToRow implementations
// =============================================================================

macro_rules! impl_to_row_tuple {
    ($(($idx:tt, $T:ident, $col:literal)),+) => {
        impl<$($T: ToRow),+> ToRow for ($($T,)+) {
            fn column_names() -> &'static [&'static str] {
                &[$($col),+]
            }

            fn to_row_bytes(&self, out: &mut Vec<u8>) -> QueryResult<()> {
                $(
                    self.$idx.to_row_bytes(out)?;
                )+
                Ok(())
            }
        }
    };
}

impl_to_row_tuple!((0, A, "0"));
impl_to_row_tuple!((0, A, "0"), (1, B, "1"));
impl_to_row_tuple!((0, A, "0"), (1, B, "1"), (2, C, "2"));
impl_to_row_tuple!((0, A, "0"), (1, B, "1"), (2, C, "2"), (3, D, "3"));

// =============================================================================
// Option ToRow
// =============================================================================

impl<T: ToRow> ToRow for Option<T> {
    fn column_names() -> &'static [&'static str] {
        T::column_names()
    }

    fn to_row_bytes(&self, out: &mut Vec<u8>) -> QueryResult<()> {
        match self {
            Some(value) => {
                out.push(0); // NOT NULL flag
                value.to_row_bytes(out)
            }
            None => {
                out.push(1); // NULL flag
                Ok(())
            }
        }
    }
}

// =============================================================================
// ToSqlValues trait for async insert
// =============================================================================

/// Trait for converting a row to SQL literal values.
///
/// This is used by the async insert module to generate INSERT statements
/// with inline values. It is automatically implemented by `#[derive(Insertable)]`.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Insertable)]
/// #[diesel_clickhouse(table = events)]
/// struct NewEvent {
///     id: u64,
///     name: String,
/// }
///
/// let event = NewEvent { id: 1, name: "test".into() };
/// let columns = NewEvent::column_names(); // ["id", "name"]
/// let values = event.to_sql_values();     // ["1", "'test'"]
/// ```
pub trait ToSqlValues {
    /// Get the column names for this row type.
    fn column_names() -> Vec<&'static str>;

    /// Convert the row to SQL value strings.
    ///
    /// Each value is formatted as a SQL literal:
    /// - Integers: `42`
    /// - Floats: `3.14`
    /// - Strings: `'hello'` (with proper escaping)
    /// - NULL: `NULL`
    fn to_sql_values(&self) -> Vec<String>;
}

/// Helper function to format a value as a SQL literal.
pub fn format_sql_literal<T: ToSqlLiteral>(value: &T) -> String {
    value.to_sql_literal()
}

/// Trait for formatting a value as a SQL literal string.
pub trait ToSqlLiteral {
    /// Format as a SQL literal.
    fn to_sql_literal(&self) -> String;
}

// Implement for common types
macro_rules! impl_to_sql_literal_numeric {
    ($($t:ty),*) => {
        $(
            impl ToSqlLiteral for $t {
                fn to_sql_literal(&self) -> String {
                    self.to_string()
                }
            }
        )*
    };
}

impl_to_sql_literal_numeric!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64);

impl ToSqlLiteral for bool {
    fn to_sql_literal(&self) -> String {
        if *self { "true".to_string() } else { "false".to_string() }
    }
}

impl ToSqlLiteral for String {
    fn to_sql_literal(&self) -> String {
        format_sql_string(self)
    }
}

impl ToSqlLiteral for str {
    fn to_sql_literal(&self) -> String {
        format_sql_string(self)
    }
}

impl ToSqlLiteral for &str {
    fn to_sql_literal(&self) -> String {
        format_sql_string(self)
    }
}

impl<T: ToSqlLiteral> ToSqlLiteral for Option<T> {
    fn to_sql_literal(&self) -> String {
        match self {
            Some(v) => v.to_sql_literal(),
            None => "NULL".to_string(),
        }
    }
}

impl<T: ToSqlLiteral> ToSqlLiteral for &T {
    fn to_sql_literal(&self) -> String {
        (*self).to_sql_literal()
    }
}

/// Format a string as a SQL string literal with proper escaping.
fn format_sql_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('\'');
    for c in s.chars() {
        match c {
            '\'' => result.push_str("''"),
            '\\' => result.push_str("\\\\"),
            _ => result.push(c),
        }
    }
    result.push('\'');
    result
}

#[cfg(feature = "json")]
impl ToSqlLiteral for serde_json::Value {
    fn to_sql_literal(&self) -> String {
        match serde_json::to_string(self) {
            Ok(json_str) => format_sql_string(&json_str),
            Err(_) => "NULL".to_string(),
        }
    }
}

#[cfg(feature = "json")]
impl<T: serde::Serialize> ToSqlLiteral for diesel_clickhouse_types::JsonTyped<T> {
    fn to_sql_literal(&self) -> String {
        match serde_json::to_string(&self.0) {
            Ok(json_str) => format_sql_string(&json_str),
            Err(_) => "NULL".to_string(),
        }
    }
}
