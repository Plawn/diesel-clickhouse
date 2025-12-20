//! Example demonstrating the query builder functionality.
//!
//! This example shows how to build SQL queries using the type-safe query builder.
//!
//! Run with: cargo run --example query_builder

use diesel_clickhouse::prelude::*;
use diesel_clickhouse::backend::{
    ClickHouse, GenericQueryBuilder, GenericBindCollector, QueryBuilder,
};
use diesel_clickhouse::expression::sql;
use diesel_clickhouse::query_builder::{
    SelectStatement, AstPass, QueryFragment, ClickHouseQueryExt,
};

/// A simple table name wrapper that implements QueryFragment.
#[derive(Debug, Clone, Copy)]
struct TableName(&'static str);

impl<DB: diesel_clickhouse::backend::Backend> QueryFragment<DB> for TableName {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> diesel_clickhouse::result::QueryResult<()> {
        pass.push_sql(self.0);
        Ok(())
    }
}

/// Helper to build SQL string from a query fragment.
fn to_sql<T: QueryFragment<ClickHouse>>(fragment: &T) -> String {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass).unwrap();
    builder.finish()
}

fn main() {
    println!("=== Query Builder Examples ===");
    println!();

    // 1. Simple SELECT *
    println!("1. Simple SELECT *:");
    let query = SelectStatement::new(TableName("events"));
    println!("   {}", to_sql(&query));
    println!();

    // 2. SELECT with specific columns
    println!("2. SELECT specific columns:");
    let query = SelectStatement::new(TableName("events"))
        .select(sql::<(UInt64, CHString)>("id, event_type"));
    println!("   {}", to_sql(&query));
    println!();

    // 3. SELECT with WHERE clause
    println!("3. SELECT with WHERE:");
    let query = SelectStatement::new(TableName("events"))
        .filter(sql::<Bool>("user_id = 42"));
    println!("   {}", to_sql(&query));
    println!();

    // 4. SELECT with multiple conditions (combined in filter)
    println!("4. SELECT with AND conditions:");
    let query = SelectStatement::new(TableName("events"))
        .filter(sql::<Bool>("user_id = 42 AND event_type = 'click'"));
    println!("   {}", to_sql(&query));
    println!();

    // 5. SELECT with ORDER BY
    println!("5. SELECT with ORDER BY:");
    let query = SelectStatement::new(TableName("events"))
        .order_by(sql::<DateTime>("timestamp DESC"));
    println!("   {}", to_sql(&query));
    println!();

    // 6. SELECT with LIMIT
    println!("6. SELECT with LIMIT:");
    let query = SelectStatement::new(TableName("events"))
        .limit(100);
    println!("   {}", to_sql(&query));
    println!();

    // 7. SELECT with LIMIT and OFFSET (pagination)
    println!("7. SELECT with LIMIT and OFFSET:");
    let query = SelectStatement::new(TableName("events"))
        .limit(10)
        .offset(20);
    println!("   {}", to_sql(&query));
    println!();

    // 8. SELECT with GROUP BY
    println!("8. SELECT with GROUP BY:");
    let query = SelectStatement::new(TableName("events"))
        .select(sql::<(CHString, UInt64)>("event_type, count(*) as cnt"))
        .group_by(sql::<CHString>("event_type"));
    println!("   {}", to_sql(&query));
    println!();

    // 9. Combined query with multiple clauses
    println!("9. Complex query:");
    let query = SelectStatement::new(TableName("events"))
        .select(sql::<(UInt32, UInt64)>("user_id, count(*) as event_count"))
        .filter(sql::<Bool>("timestamp > now() - INTERVAL 7 DAY"))
        .group_by(sql::<UInt32>("user_id"))
        .having(sql::<Bool>("event_count > 10"))
        .order_by(sql::<UInt64>("event_count DESC"))
        .limit(50);
    println!("   {}", to_sql(&query));
    println!();

    // 10. ClickHouse FINAL modifier (for ReplacingMergeTree)
    println!("10. SELECT with FINAL:");
    let query = SelectStatement::new(TableName("users"))
        .final_();
    println!("   {}", to_sql(&query));
    println!();

    // 11. ClickHouse PREWHERE (optimized pre-filtering)
    // Note: PREWHERE is applied as a wrapper around the query
    println!("11. SELECT with PREWHERE:");
    let base_query = SelectStatement::new(TableName("events"))
        .filter(sql::<Bool>("event_type = 'purchase'"));
    let query = base_query.prewhere(sql::<Bool>("user_id > 1000"));
    println!("   {}", to_sql(&query));
    println!();

    // 12. ClickHouse SAMPLE (query fraction of data)
    println!("12. SELECT with SAMPLE (10% of data):");
    let query = SelectStatement::new(TableName("events"))
        .sample(0.1);
    println!("   {}", to_sql(&query));
    println!();

    // 13. WITH TOTALS for aggregations
    println!("13. SELECT with WITH TOTALS:");
    let query = SelectStatement::new(TableName("events"))
        .select(sql::<(CHString, UInt64)>("event_type, count(*)"))
        .group_by(sql::<CHString>("event_type"))
        .with_totals();
    println!("   {}", to_sql(&query));
    println!();

    // 14. Query with SETTINGS
    println!("14. SELECT with SETTINGS:");
    let query = SelectStatement::new(TableName("events"))
        .settings()
        .set("max_threads", "4")
        .set("max_memory_usage", "10000000000");
    println!("   {}", to_sql(&query));
    println!();

    // 15. FORMAT clause for output format
    println!("15. SELECT with FORMAT:");
    let query = SelectStatement::new(TableName("events"))
        .limit(10)
        .format("JSONEachRow");
    println!("   {}", to_sql(&query));
    println!();

    // 16. All ClickHouse features combined
    // Build the base query first, then apply ClickHouse-specific modifiers
    println!("16. All features combined:");
    let base_query = SelectStatement::new(TableName("events"))
        .select(sql::<(UInt32, UInt64, Float64)>("user_id, count(*), sum(value)"))
        .filter(sql::<Bool>("event_type IN ('click', 'view')"))
        .group_by(sql::<UInt32>("user_id"))
        .having(sql::<Bool>("count(*) > 5"))
        .order_by(sql::<UInt64>("count(*) DESC"))
        .limit(100);

    // Apply ClickHouse modifiers
    let query = base_query
        .prewhere(sql::<Bool>("toYYYYMM(timestamp) = 202406"))
        .final_()
        .sample(0.5)
        .with_totals();
    println!("   {}", to_sql(&query));
    println!();

    println!("=== Done ===");
}
