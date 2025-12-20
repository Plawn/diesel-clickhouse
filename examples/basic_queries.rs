//! Basic query building examples for diesel-clickhouse.
//!
//! This example demonstrates how to build SQL queries using the query builder API.
//! It shows the generated SQL for each query type.
//!
//! Run with: cargo run --example basic_queries

use diesel_clickhouse::prelude::*;
use diesel_clickhouse::backend::{ClickHouse, GenericQueryBuilder, GenericBindCollector, QueryBuilder};
use diesel_clickhouse::expression::sql;
use diesel_clickhouse::query_builder::{SelectStatement, AstPass, QueryFragment};

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
    println!("=== Basic Query Examples ===\n");

    // -------------------------------------------------------------------------
    // 1. Simple SELECT *
    // -------------------------------------------------------------------------
    println!("1. SELECT * (all columns):");
    let query = SelectStatement::new(Table("events"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 2. SELECT specific columns
    // -------------------------------------------------------------------------
    println!("2. SELECT specific columns:");
    let query = SelectStatement::new(Table("events"))
        .select(sql::<(UInt64, CHString, DateTime)>("id, event_type, timestamp"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 3. SELECT with WHERE clause
    // -------------------------------------------------------------------------
    println!("3. SELECT with WHERE (single condition):");
    let query = SelectStatement::new(Table("events"))
        .filter(sql::<Bool>("user_id = 42"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 4. SELECT with multiple WHERE conditions
    // -------------------------------------------------------------------------
    println!("4. SELECT with multiple conditions (AND):");
    let query = SelectStatement::new(Table("events"))
        .filter(sql::<Bool>("user_id = 42 AND event_type = 'click' AND value > 0"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 5. SELECT with OR conditions
    // -------------------------------------------------------------------------
    println!("5. SELECT with OR conditions:");
    let query = SelectStatement::new(Table("events"))
        .filter(sql::<Bool>("event_type = 'click' OR event_type = 'view'"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 6. ORDER BY (ascending)
    // -------------------------------------------------------------------------
    println!("6. ORDER BY ascending:");
    let query = SelectStatement::new(Table("events"))
        .order_by(sql::<UInt64>("id ASC"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 7. ORDER BY (descending)
    // -------------------------------------------------------------------------
    println!("7. ORDER BY descending:");
    let query = SelectStatement::new(Table("events"))
        .order_by(sql::<DateTime>("timestamp DESC"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 8. ORDER BY multiple columns
    // -------------------------------------------------------------------------
    println!("8. ORDER BY multiple columns:");
    let query = SelectStatement::new(Table("events"))
        .order_by(sql::<(UInt32, DateTime)>("user_id ASC, timestamp DESC"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 9. LIMIT
    // -------------------------------------------------------------------------
    println!("9. LIMIT:");
    let query = SelectStatement::new(Table("events"))
        .limit(100);
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 10. LIMIT with OFFSET (pagination)
    // -------------------------------------------------------------------------
    println!("10. LIMIT with OFFSET (pagination):");
    let query = SelectStatement::new(Table("events"))
        .order_by(sql::<UInt64>("id"))
        .limit(10)
        .offset(20);
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 11. COUNT aggregation
    // -------------------------------------------------------------------------
    println!("11. COUNT aggregation:");
    let query = SelectStatement::new(Table("events"))
        .select(sql::<UInt64>("count(*)"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 12. Multiple aggregations
    // -------------------------------------------------------------------------
    println!("12. Multiple aggregations:");
    let query = SelectStatement::new(Table("events"))
        .select(sql::<(UInt64, Float64, Float64)>("count(*), sum(value), avg(value)"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 13. GROUP BY
    // -------------------------------------------------------------------------
    println!("13. GROUP BY:");
    let query = SelectStatement::new(Table("events"))
        .select(sql::<(CHString, UInt64)>("event_type, count(*)"))
        .group_by(sql::<CHString>("event_type"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 14. GROUP BY with HAVING
    // -------------------------------------------------------------------------
    println!("14. GROUP BY with HAVING:");
    let query = SelectStatement::new(Table("events"))
        .select(sql::<(UInt32, UInt64)>("user_id, count(*) as cnt"))
        .group_by(sql::<UInt32>("user_id"))
        .having(sql::<Bool>("cnt > 10"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 15. Complete query with all clauses
    // -------------------------------------------------------------------------
    println!("15. Complete query (SELECT, WHERE, GROUP BY, HAVING, ORDER BY, LIMIT):");
    let query = SelectStatement::new(Table("events"))
        .select(sql::<(UInt32, UInt64, Float64)>("user_id, count(*) as cnt, sum(value) as total"))
        .filter(sql::<Bool>("timestamp >= '2024-01-01' AND timestamp < '2024-02-01'"))
        .group_by(sql::<UInt32>("user_id"))
        .having(sql::<Bool>("cnt >= 5"))
        .order_by(sql::<Float64>("total DESC"))
        .limit(50);
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 16. Subquery in WHERE (using IN)
    // -------------------------------------------------------------------------
    println!("16. Using IN clause:");
    let query = SelectStatement::new(Table("events"))
        .filter(sql::<Bool>("user_id IN (1, 2, 3, 4, 5)"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 17. LIKE pattern matching
    // -------------------------------------------------------------------------
    println!("17. LIKE pattern matching:");
    let query = SelectStatement::new(Table("events"))
        .filter(sql::<Bool>("event_type LIKE 'click%'"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 18. NULL handling
    // -------------------------------------------------------------------------
    println!("18. NULL handling:");
    let query = SelectStatement::new(Table("events"))
        .filter(sql::<Bool>("parent_id IS NOT NULL"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 19. Date/time functions
    // -------------------------------------------------------------------------
    println!("19. Date/time functions:");
    let query = SelectStatement::new(Table("events"))
        .select(sql::<(UInt32, UInt64)>("toYYYYMM(timestamp) as month, count(*)"))
        .group_by(sql::<UInt32>("month"))
        .order_by(sql::<UInt32>("month"));
    println!("   SQL: {}\n", to_sql(&query));

    // -------------------------------------------------------------------------
    // 20. DISTINCT
    // -------------------------------------------------------------------------
    println!("20. DISTINCT:");
    let query = SelectStatement::new(Table("events"))
        .select(sql::<CHString>("DISTINCT event_type"));
    println!("   SQL: {}\n", to_sql(&query));

    println!("=== End of Basic Query Examples ===");
}
