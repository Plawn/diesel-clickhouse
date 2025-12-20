//! Advanced ClickHouse-specific query examples.
//!
//! This example demonstrates ClickHouse-specific features:
//! - FINAL modifier for ReplacingMergeTree deduplication
//! - PREWHERE for optimized pre-filtering
//! - SAMPLE for approximate queries
//! - WITH TOTALS for aggregation totals
//! - Query SETTINGS
//! - FORMAT clause
//!
//! Run with: cargo run --example advanced_queries

use diesel_clickhouse::prelude::*;
use diesel_clickhouse::backend::{ClickHouse, GenericQueryBuilder, GenericBindCollector, QueryBuilder};
use diesel_clickhouse::expression::sql;
use diesel_clickhouse::query_builder::{SelectStatement, AstPass, QueryFragment, ClickHouseQueryExt};

/// Helper to convert a QueryFragment to SQL string.
fn to_sql<T: QueryFragment<ClickHouse>>(fragment: &T) -> String {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass).unwrap();
    builder.finish()
}

/// Simple table name wrapper.
#[derive(Debug, Clone, Copy)]
struct Table(&'static str);

impl<DB: diesel_clickhouse::backend::Backend> QueryFragment<DB> for Table {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> diesel_clickhouse::result::QueryResult<()> {
        pass.push_sql(self.0);
        Ok(())
    }
}

fn main() {
    println!("=== ClickHouse-Specific Query Features ===\n");

    // =========================================================================
    // FINAL Modifier
    // =========================================================================
    println!("--- FINAL Modifier ---");
    println!("Use FINAL with ReplacingMergeTree/CollapsingMergeTree to get deduplicated results.\n");

    let query = SelectStatement::new(Table("users")).final_();
    println!("1. Basic FINAL:");
    println!("   SQL: {}\n", to_sql(&query));

    let query = SelectStatement::new(Table("users"))
        .filter(sql::<Bool>("active = 1"))
        .final_();
    println!("2. FINAL with WHERE:");
    println!("   SQL: {}\n", to_sql(&query));

    // =========================================================================
    // PREWHERE Clause
    // =========================================================================
    println!("--- PREWHERE Clause ---");
    println!("PREWHERE filters data BEFORE reading all columns - much faster for");
    println!("partition keys and primary key columns.");
    println!("Note: Apply PREWHERE after building the base query.\n");

    let base_query = SelectStatement::new(Table("events"));
    let query = base_query.prewhere(sql::<Bool>("toYYYYMM(timestamp) = 202406"));
    println!("3. PREWHERE on partition key:");
    println!("   SQL: {}\n", to_sql(&query));

    // For PREWHERE + WHERE, build the WHERE first, then apply PREWHERE
    let base_query = SelectStatement::new(Table("events"))
        .filter(sql::<Bool>("event_type = 'purchase'"));
    let query = base_query.prewhere(sql::<Bool>("user_id > 1000"));
    println!("4. WHERE then PREWHERE (PREWHERE applied last):");
    println!("   SQL: {}\n", to_sql(&query));

    // =========================================================================
    // SAMPLE Clause
    // =========================================================================
    println!("--- SAMPLE Clause ---");
    println!("Sample a fraction of data for approximate queries on large tables.\n");

    let query = SelectStatement::new(Table("events")).sample(0.1);
    println!("5. SAMPLE 10% of data:");
    println!("   SQL: {}\n", to_sql(&query));

    let query = SelectStatement::new(Table("events"))
        .select(sql::<UInt64>("count(*)"))
        .sample(0.01);
    println!("6. COUNT with 1% sample:");
    println!("   SQL: {}\n", to_sql(&query));

    let query = SelectStatement::new(Table("events"))
        .sample_with_offset(0.1, 0.5);
    println!("7. SAMPLE with OFFSET (for reproducibility):");
    println!("   SQL: {}\n", to_sql(&query));

    // =========================================================================
    // WITH TOTALS
    // =========================================================================
    println!("--- WITH TOTALS ---");
    println!("Get an extra row with totals for aggregations.\n");

    let query = SelectStatement::new(Table("events"))
        .select(sql::<(CHString, UInt64)>("event_type, count(*)"))
        .group_by(sql::<CHString>("event_type"))
        .with_totals();
    println!("8. GROUP BY with totals row:");
    println!("   SQL: {}\n", to_sql(&query));

    let query = SelectStatement::new(Table("events"))
        .select(sql::<(UInt32, Float64, Float64)>("user_id, sum(value), avg(value)"))
        .group_by(sql::<UInt32>("user_id"))
        .having(sql::<Bool>("sum(value) > 100"))
        .with_totals();
    println!("9. Aggregation with HAVING and totals:");
    println!("   SQL: {}\n", to_sql(&query));

    // =========================================================================
    // Query SETTINGS
    // =========================================================================
    println!("--- Query SETTINGS ---");
    println!("Apply ClickHouse settings to specific queries.\n");

    let query = SelectStatement::new(Table("events"))
        .settings()
        .set("max_threads", "4");
    println!("10. Limit threads:");
    println!("   SQL: {}\n", to_sql(&query));

    let query = SelectStatement::new(Table("events"))
        .settings()
        .set("max_memory_usage", "10000000000")
        .set("max_execution_time", "60");
    println!("11. Memory and time limits:");
    println!("   SQL: {}\n", to_sql(&query));

    let query = SelectStatement::new(Table("events"))
        .settings()
        .set("optimize_read_in_order", "1")
        .set("read_overflow_mode", "break");
    println!("12. Optimization settings:");
    println!("   SQL: {}\n", to_sql(&query));

    // =========================================================================
    // FORMAT Clause
    // =========================================================================
    println!("--- FORMAT Clause ---");
    println!("Specify output format for raw queries.\n");

    let query = SelectStatement::new(Table("events"))
        .limit(10)
        .format("JSONEachRow");
    println!("13. JSON output:");
    println!("   SQL: {}\n", to_sql(&query));

    let query = SelectStatement::new(Table("events"))
        .select(sql::<(UInt64, CHString)>("id, event_type"))
        .format("CSV");
    println!("14. CSV output:");
    println!("   SQL: {}\n", to_sql(&query));

    let query = SelectStatement::new(Table("events"))
        .format("TabSeparatedWithNames");
    println!("15. TSV with headers:");
    println!("   SQL: {}\n", to_sql(&query));

    // =========================================================================
    // Combining Features
    // =========================================================================
    println!("--- Combining Features ---");
    println!("Use multiple ClickHouse features together.");
    println!("Important: Build the standard query first, then apply modifiers.\n");

    // Build base query, then apply ClickHouse modifiers
    let base = SelectStatement::new(Table("events"))
        .filter(sql::<Bool>("event_type IN ('click', 'view')"))
        .order_by(sql::<DateTime>("timestamp DESC"))
        .limit(100);
    let query = base
        .prewhere(sql::<Bool>("toYYYYMM(timestamp) = 202406"))
        .final_();
    println!("16. Complex query with PREWHERE + FINAL:");
    println!("   SQL: {}\n", to_sql(&query));

    // Full analytics query
    let base = SelectStatement::new(Table("analytics"))
        .select(sql::<(UInt32, UInt64, Float64)>("user_id, count(*), sum(revenue)"))
        .filter(sql::<Bool>("country = 'US' AND date >= '2024-01-01'"))
        .group_by(sql::<UInt32>("user_id"))
        .order_by(sql::<Float64>("sum(revenue) DESC"))
        .limit(100);
    let query = base.with_totals().sample(0.5);
    println!("17. Full analytics query with SAMPLE and WITH TOTALS:");
    println!("   SQL: {}\n", to_sql(&query));

    let base = SelectStatement::new(Table("events"))
        .select(sql::<(CHString, UInt64)>("event_type, count(*)"))
        .group_by(sql::<CHString>("event_type"));
    let query = base
        .with_totals()
        .settings()
        .set("totals_mode", "after_having_auto")
        .set("max_threads", "8");
    println!("18. Aggregation with custom totals mode:");
    println!("   SQL: {}\n", to_sql(&query));

    // =========================================================================
    // Real-world Patterns
    // =========================================================================
    println!("--- Real-world Query Patterns ---\n");

    // Time-series analysis
    let query = SelectStatement::new(Table("metrics"))
        .select(sql::<(DateTime, Float64, Float64, Float64)>(
            "toStartOfHour(timestamp) as hour, avg(value), min(value), max(value)"
        ))
        .filter(sql::<Bool>("timestamp >= now() - INTERVAL 24 HOUR"))
        .group_by(sql::<DateTime>("hour"))
        .order_by(sql::<DateTime>("hour"));
    println!("19. Time-series aggregation (hourly):");
    println!("   SQL: {}\n", to_sql(&query));

    // Funnel analysis
    let query = SelectStatement::new(Table("events"))
        .select(sql::<(UInt64, UInt64, UInt64)>(
            "countIf(event_type = 'view') as views, \
             countIf(event_type = 'click') as clicks, \
             countIf(event_type = 'purchase') as purchases"
        ))
        .filter(sql::<Bool>("date = today()"));
    println!("20. Funnel analysis:");
    println!("   SQL: {}\n", to_sql(&query));

    // Top N with percentage
    let query = SelectStatement::new(Table("events"))
        .select(sql::<(CHString, UInt64, Float64)>(
            "event_type, count(*) as cnt, \
             round(cnt * 100 / sum(cnt) OVER (), 2) as percentage"
        ))
        .group_by(sql::<CHString>("event_type"))
        .order_by(sql::<UInt64>("cnt DESC"))
        .limit(10);
    println!("21. Top N with percentage:");
    println!("   SQL: {}\n", to_sql(&query));

    // Deduplication query
    let query = SelectStatement::new(Table("user_events"))
        .filter(sql::<Bool>("user_id = 12345"))
        .order_by(sql::<DateTime>("timestamp DESC"))
        .limit(1)
        .final_();
    println!("22. Get latest event for user (with deduplication):");
    println!("   SQL: {}\n", to_sql(&query));

    println!("=== End of Advanced Examples ===");
}
