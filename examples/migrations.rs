//! Migration system examples for diesel-clickhouse.
//!
//! This example demonstrates the migration API without requiring a database connection.
//! It shows how migrations are structured and how to work with them programmatically.
//!
//! Run with: cargo run --example migrations

use diesel_clickhouse_migrations::{
    Migration, MigrationVersion, MigrationName, MigrationBuilder,
};

fn main() {
    println!("=== Migration System Examples ===\n");

    // =========================================================================
    // Creating Migrations
    // =========================================================================
    println!("--- Creating Migrations ---\n");

    // Create a simple migration
    let migration = Migration::new(
        "20240101000000",
        "create_events",
        r#"
CREATE TABLE events (
    id UInt64,
    user_id UInt32,
    event_type LowCardinality(String),
    timestamp DateTime64(3) DEFAULT now64(3),
    value Float64 DEFAULT 0.0,
    properties Map(String, String)
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (user_id, timestamp, id)
        "#.trim(),
        "DROP TABLE IF EXISTS events",
    );

    println!("1. Created migration:");
    println!("   Version: {}", migration.version);
    println!("   Name:    {}", migration.name);
    println!("   Directory: {}", migration.directory_name());
    println!("   Checksum: {}", migration.checksum());
    println!();

    // Create migration for ReplacingMergeTree (with deduplication)
    let users_migration = Migration::new(
        "20240102000000",
        "create_users",
        r#"
CREATE TABLE users (
    id UInt64,
    email String,
    name String,
    country LowCardinality(String) DEFAULT '',
    created_at DateTime DEFAULT now(),
    updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at)
ORDER BY id
        "#.trim(),
        "DROP TABLE IF EXISTS users",
    );

    println!("2. Created users migration (ReplacingMergeTree):");
    println!("   Version: {}", users_migration.version);
    println!("   Name:    {}", users_migration.name);
    println!();

    // =========================================================================
    // Migration Versioning
    // =========================================================================
    println!("--- Migration Versioning ---\n");

    let v1 = MigrationVersion::new("20240101000000");
    let v2 = MigrationVersion::new("20240102000000");
    let v3 = MigrationVersion::new("20240615143000");

    println!("3. Version parsing:");
    println!("   v1: {} (timestamp format: YYYYMMDDHHmmss)", v1.as_str());
    println!("   v2: {}", v2.as_str());
    println!("   v3: {}", v3.as_str());
    println!();

    println!("4. Version ordering:");
    println!("   v1 < v2: {}", v1 < v2);
    println!("   v2 < v3: {}", v2 < v3);
    println!("   v1 < v3: {}", v1 < v3);
    println!();

    // =========================================================================
    // Migration Names
    // =========================================================================
    println!("--- Migration Names ---\n");

    let name1 = MigrationName::new("create_events");
    let name2 = MigrationName::new("add_index_to_users");
    let name3 = MigrationName::new("create_materialized_view");

    println!("5. Migration names:");
    println!("   - {}", name1.as_str());
    println!("   - {}", name2.as_str());
    println!("   - {}", name3.as_str());
    println!();

    // =========================================================================
    // Migration Builder
    // =========================================================================
    println!("--- Migration Builder ---\n");

    let built_migration = MigrationBuilder::new()
        .version("20240301000000")
        .name("add_orders_table")
        .up(r#"
CREATE TABLE orders (
    id UInt64,
    user_id UInt32,
    total Float64,
    created_at DateTime DEFAULT now()
) ENGINE = MergeTree()
ORDER BY (created_at, id)
        "#.trim())
        .down("DROP TABLE IF EXISTS orders")
        .build();

    if let Some(m) = built_migration {
        println!("6. Built migration using builder:");
        println!("   Version: {}", m.version);
        println!("   Name:    {}", m.name);
    }
    println!();

    // =========================================================================
    // Multi-Statement Migrations
    // =========================================================================
    println!("--- Multi-Statement Migrations ---\n");

    let multi_migration = Migration::new(
        "20240201000000",
        "create_analytics_schema",
        r#"
-- Create base table
CREATE TABLE page_views (
    id UInt64,
    page_url String,
    user_id UInt32,
    timestamp DateTime64(3)
) ENGINE = MergeTree()
ORDER BY (timestamp, user_id);

-- Create daily aggregation table
CREATE TABLE daily_stats (
    date Date,
    page_url String,
    views UInt64,
    unique_users UInt64
) ENGINE = SummingMergeTree()
ORDER BY (date, page_url);

-- Create materialized view for automatic aggregation
CREATE MATERIALIZED VIEW page_views_mv TO daily_stats AS
SELECT
    toDate(timestamp) as date,
    page_url,
    count() as views,
    uniq(user_id) as unique_users
FROM page_views
GROUP BY date, page_url;
        "#.trim(),
        r#"
DROP VIEW IF EXISTS page_views_mv;
DROP TABLE IF EXISTS daily_stats;
DROP TABLE IF EXISTS page_views;
        "#.trim(),
    );

    println!("7. Multi-statement migration:");
    println!("   Version: {}", multi_migration.version);
    println!("   Name:    {}", multi_migration.name);
    println!("   Creates: page_views, daily_stats, page_views_mv");
    println!();

    // =========================================================================
    // Parsing from Directory
    // =========================================================================
    println!("--- Parsing from Directory ---\n");

    // Parse version and name from directory name
    let dir_name = "20240315120000_add_user_sessions";
    if let Some(version) = MigrationVersion::from_directory_name(dir_name) {
        println!("8. Parsed from directory '{}':", dir_name);
        println!("   Version: {}", version.as_str());
    }
    if let Some(name) = MigrationName::from_directory_name(dir_name) {
        println!("   Name: {}", name.as_str());
    }
    println!();

    // =========================================================================
    // Directory Structure
    // =========================================================================
    println!("--- Migration Directory Structure ---\n");

    println!("9. Expected file structure:");
    println!("   migrations/");
    println!("   +-- 20240101000000_create_events/");
    println!("   |   +-- up.sql");
    println!("   |   +-- down.sql");
    println!("   +-- 20240102000000_create_users/");
    println!("   |   +-- up.sql");
    println!("   |   +-- down.sql");
    println!("   +-- 20240201000000_create_analytics_schema/");
    println!("       +-- up.sql");
    println!("       +-- down.sql");
    println!();

    // =========================================================================
    // CLI Commands
    // =========================================================================
    println!("--- CLI Commands Reference ---\n");

    println!("10. Generate a new migration:");
    println!("    $ diesel-clickhouse migration generate create_orders");
    println!();

    println!("11. Run pending migrations:");
    println!("    $ diesel-clickhouse migration run");
    println!("    $ DATABASE_URL=http://localhost:8123 diesel-clickhouse migration run");
    println!();

    println!("12. Revert last migration:");
    println!("    $ diesel-clickhouse migration revert");
    println!();

    println!("13. List migration status:");
    println!("    $ diesel-clickhouse migration list");
    println!();

    // =========================================================================
    // Best Practices
    // =========================================================================
    println!("--- Best Practices ---\n");

    println!("14. Migration best practices:");
    println!("    - Always include a down.sql for reversibility");
    println!("    - Use descriptive names (create_X, add_X_to_Y, remove_X)");
    println!("    - One logical change per migration");
    println!("    - Test migrations on a copy of production data");
    println!("    - For ClickHouse: prefer ALTER TABLE over recreating tables");
    println!();

    println!("15. ClickHouse-specific tips:");
    println!("    - Use appropriate MergeTree engine variants:");
    println!("      * MergeTree: general purpose");
    println!("      * ReplacingMergeTree: deduplication by ORDER BY key");
    println!("      * SummingMergeTree: pre-aggregation");
    println!("      * AggregatingMergeTree: complex aggregations");
    println!("    - Define PARTITION BY for time-series data");
    println!("    - Choose ORDER BY carefully (affects query performance)");
    println!("    - Use LowCardinality for low-cardinality string columns");
    println!("    - Consider TTL for data retention policies");
    println!();

    // =========================================================================
    // Sample Migration SQL
    // =========================================================================
    println!("--- Sample Migration Templates ---\n");

    println!("16. Adding an index:");
    println!("    ALTER TABLE users ADD INDEX idx_email email TYPE bloom_filter GRANULARITY 1;");
    println!();

    println!("17. Adding a column:");
    println!("    ALTER TABLE events ADD COLUMN session_id UUID AFTER user_id;");
    println!();

    println!("18. Adding TTL (auto-delete old data):");
    println!("    ALTER TABLE events MODIFY TTL timestamp + INTERVAL 90 DAY;");
    println!();

    println!("19. Creating a dictionary:");
    println!("    CREATE DICTIONARY users_dict (");
    println!("        id UInt64,");
    println!("        name String");
    println!("    ) PRIMARY KEY id");
    println!("    SOURCE(CLICKHOUSE(TABLE 'users'))");
    println!("    LAYOUT(FLAT())");
    println!("    LIFETIME(MIN 300 MAX 360);");
    println!();

    println!("=== End of Migration Examples ===");
}
