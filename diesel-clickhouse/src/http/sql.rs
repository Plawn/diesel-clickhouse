//! SQL building utilities for HTTP backend.

use crate::core::backend::ClickHouse;
use crate::core::query_builder::QueryFragment;
use crate::core::result::QueryResult;

// Re-export from core for backward compatibility
pub use crate::core::sql_builder::{build_sql, BindableValue};

/// A compiled query with SQL placeholders and typed bindable values.
///
/// This is the SOTA format for native parameter binding, enabling:
/// - Query plan caching on the ClickHouse server
/// - Proper type safety through the clickhouse crate's `.bind()` method
/// - No manual string escaping
#[derive(Debug, Clone)]
pub struct NativeCompiledQuery {
    /// The SQL string with `?` placeholders.
    pub sql: String,
    /// The collected bindable values for native binding.
    pub bindings: Vec<BindableValue>,
}

impl NativeCompiledQuery {
    /// Get the number of bind parameters.
    pub fn param_count(&self) -> usize {
        self.bindings.len()
    }

    /// Check if there are any bind parameters.
    pub fn has_bindings(&self) -> bool {
        !self.bindings.is_empty()
    }

    /// Apply all bindings to a clickhouse Query object.
    pub fn bind_to(&self, mut query: clickhouse::query::Query) -> clickhouse::query::Query {
        for binding in &self.bindings {
            query = query.bind(binding);
        }
        query
    }
}

/// Build SQL with native bindable values from a QueryFragment.
///
/// This is the SOTA way to build queries for execution:
/// - Returns SQL with `?` placeholders
/// - Returns typed BindableValue instances for native `.bind()` calls
/// - Enables query plan caching on the ClickHouse server
pub fn build_sql_native<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<NativeCompiledQuery> {
    let (sql, bindings) = crate::core::sql_builder::build_sql_with_bindings(fragment)?;
    Ok(NativeCompiledQuery { sql, bindings })
}
