//! AST traversal pass for SQL generation.

use crate::backend::{Backend, BindableValue, BindCollector, QueryBuilder, ToBindableValue};

/// A pass through the query AST for generating SQL.
pub struct AstPass<'a, 'b, DB: Backend> {
    builder: &'a mut DB::QueryBuilder,
    collector: &'a mut DB::BindCollector<'b>,
}

impl<'a, 'b, DB: Backend> AstPass<'a, 'b, DB> {
    /// Create a new AST pass.
    pub fn new(
        builder: &'a mut DB::QueryBuilder,
        collector: &'a mut DB::BindCollector<'b>,
    ) -> Self {
        Self { builder, collector }
    }

    /// Reborrow the pass for a sub-expression.
    pub fn reborrow(&mut self) -> AstPass<'_, 'b, DB> {
        AstPass {
            builder: self.builder,
            collector: self.collector,
        }
    }

    /// Push raw SQL text.
    pub fn push_sql(&mut self, sql: &str) {
        self.builder.push_sql(sql);
    }

    /// Push an identifier (escaped appropriately).
    pub fn push_identifier(&mut self, identifier: &str) {
        self.builder.push_identifier(identifier);
    }

    /// Push a bind parameter marker.
    pub fn push_bind_param(&mut self) {
        self.builder.push_bind_param();
    }

    /// Push a bind parameter and collect a bindable value for native binding.
    ///
    /// This adds a `?` placeholder to the SQL and collects the typed value
    /// for native `.bind()` at execution time, enabling query plan caching
    /// on the ClickHouse server.
    pub fn push_bindable<T: ToBindableValue>(&mut self, value: &T) -> crate::result::QueryResult<()> {
        self.builder.push_bind_param();
        self.collector.push_bindable_value(value.to_bindable_value())
    }

    /// Push a bind parameter with a pre-constructed BindableValue.
    pub fn push_bindable_value(&mut self, value: BindableValue) -> crate::result::QueryResult<()> {
        self.builder.push_bind_param();
        self.collector.push_bindable_value(value)
    }

    /// Get the current SQL (for debugging).
    pub fn sql(&self) -> &str {
        self.builder.sql()
    }

    /// Get the bind collector (for accessing collected bindings).
    pub fn collector(&self) -> &DB::BindCollector<'b> {
        self.collector
    }
}
