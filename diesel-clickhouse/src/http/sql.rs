//! SQL building utilities for HTTP backend.

use crate::core::backend::{BindCollector, ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::core::query_builder::{AstPass, QueryFragment};
use crate::core::result::QueryResult;

// Re-export for convenience
pub use crate::core::backend::BindableValue;

/// Build SQL from a QueryFragment.
///
/// Returns the SQL string with `?` placeholders for parameters.
/// This is a lightweight function for display/debugging purposes.
///
/// For actual query execution, use the connection methods which apply
/// native parameter binding via `build_sql_native()`.
pub fn build_sql<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<String> {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)?;
    Ok(builder.finish())
}

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
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)?;

    Ok(NativeCompiledQuery {
        sql: builder.finish(),
        bindings: collector.bindable_values().to_vec(),
    })
}
