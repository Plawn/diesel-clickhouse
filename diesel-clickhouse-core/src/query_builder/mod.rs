//! Query building types and traits.

mod ast_pass;
mod select;
mod insert;
mod update;
mod delete;
mod clickhouse;
pub mod set_operations;
pub mod with;
pub mod modifiers;

pub use ast_pass::*;
pub use select::*;
pub use insert::*;
pub use update::*;
pub use delete::*;
pub use clickhouse::*;
pub use set_operations::{
    Union, UnionAll, Intersect, Except,
    SetOperationsDsl, SetOperationModifiers,
    OrderedSetOperation, LimitedSetOperation,
};
pub use with::{
    Cte, WithClause, CteList, CteRef, cte_ref,
    WithQueryBuilder, with_query,
    WithQueriesBuilder, with_queries,
    DynamicWithBuilder, dynamic_with,
    WithDsl,
};
pub use modifiers::{
    LimitBy, LimitOffsetBy, LimitByDsl,
    Alias, AliasDsl,
};

use crate::backend::Backend;
use crate::result::QueryResult;

/// Trait for types that can generate SQL.
pub trait QueryFragment<DB: Backend> {
    /// Walk the AST and generate SQL.
    fn walk_ast<'b>(&'b self, pass: AstPass<'_, 'b, DB>) -> QueryResult<()>;

    /// Generate the SQL string.
    fn to_sql(&self) -> QueryResult<String>
    where
        DB::QueryBuilder: Default,
    {
        let mut builder = DB::QueryBuilder::default();
        let mut collector = DB::BindCollector::default();
        let pass = AstPass::new(&mut builder, &mut collector);
        self.walk_ast(pass)?;
        Ok(crate::backend::QueryBuilder::finish(builder))
    }
}

/// A complete query that can be executed.
pub trait Query: QueryFragment<crate::backend::ClickHouse> {
    /// The SQL type returned by this query.
    type SqlType: diesel_clickhouse_types::SqlType;
}

/// Trait for queries that have a known output SQL type.
///
/// This trait exposes the SQL type of a query's result columns at compile time,
/// enabling type-safe verification that a result struct matches the query output.
///
/// # Example
///
/// ```rust,ignore
/// // SELECT * FROM users returns (UInt64, CHString, Bool, ...)
/// // The SqlType is determined by the table's AllColumnsSqlType
/// users::table // SqlType = users::AllColumnsSqlType
///
/// // SELECT id, name FROM users returns (UInt64, CHString)
/// users::table.select((users::id, users::name)) // SqlType = (UInt64, CHString)
/// ```
pub trait QueryOutputType {
    /// The SQL type of the query's result columns.
    type SqlType: diesel_clickhouse_types::SqlType;
}

// Implement QueryFragment for common types
impl<DB: Backend> QueryFragment<DB> for () {
    fn walk_ast<'b>(&'b self, _pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        Ok(())
    }
}

impl<DB: Backend> QueryFragment<DB> for str {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(self);
        Ok(())
    }
}

impl<DB: Backend> QueryFragment<DB> for String {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql(self);
        Ok(())
    }
}

// Tuple implementations
macro_rules! impl_query_fragment_tuple {
    ($($T:ident),+) => {
        impl<DB: Backend, $($T: QueryFragment<DB>),+> QueryFragment<DB> for ($($T,)+) {
            #[allow(non_snake_case, unused_assignments)]
            fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
                let ($($T,)+) = self;
                let mut first = true;
                $(
                    if !first {
                        pass.push_sql(", ");
                    }
                    first = false;
                    $T.walk_ast(pass.reborrow())?;
                )+
                Ok(())
            }
        }
    };
}

impl_query_fragment_tuple!(A);
impl_query_fragment_tuple!(A, B);
impl_query_fragment_tuple!(A, B, C);
impl_query_fragment_tuple!(A, B, C, D);
impl_query_fragment_tuple!(A, B, C, D, E);
impl_query_fragment_tuple!(A, B, C, D, E, F);
impl_query_fragment_tuple!(A, B, C, D, E, F, G);
impl_query_fragment_tuple!(A, B, C, D, E, F, G, H);
