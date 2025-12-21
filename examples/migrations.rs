//! Migration system examples for diesel-clickhouse.
//!
//! Demonstrates the migration API programmatically.
//! Run with: `cargo run --example migrations`

use diesel_clickhouse_migrations::{
    Migration, MigrationVersion, MigrationName, MigrationBuilder,
    source::{InMemoryMigrations, MigrationSource},
};

fn main() {
    println!("=== Migration System Examples ===\n");

    // =========================================================================
    // 1. Creating Migrations
    // =========================================================================

    let migration = Migration::new(
        "20240101000000",
        "create_events",
        r#"
CREATE TABLE events (
    id UInt64,
    user_id UInt32,
    event_type LowCardinality(String),
    timestamp DateTime64(3) DEFAULT now64(3),
    properties Map(String, String)
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (user_id, timestamp, id)
        "#.trim(),
        "DROP TABLE IF EXISTS events",
    );

    println!("1. Migration created:");
    println!("   Version: {}", migration.version);
    println!("   Name: {}", migration.name);
    println!("   Directory: {}", migration.directory_name());
    println!("   Checksum: {}\n", migration.checksum());

    // =========================================================================
    // 2. Version Ordering
    // =========================================================================

    let v1 = MigrationVersion::new("20240101000000");
    let v2 = MigrationVersion::new("20240102000000");

    println!("2. Version ordering:");
    println!("   {} < {}: {}\n", v1.as_str(), v2.as_str(), v1 < v2);

    // =========================================================================
    // 3. Builder Pattern
    // =========================================================================

    let built = MigrationBuilder::new()
        .version("20240301000000")
        .name("add_orders")
        .up("CREATE TABLE orders (id UInt64) ENGINE = MergeTree ORDER BY id")
        .down("DROP TABLE IF EXISTS orders")
        .build();

    if let Some(m) = built {
        println!("3. Built with builder: {} - {}\n", m.version, m.name);
    }

    // =========================================================================
    // 4. In-Memory Migration Source
    // =========================================================================

    let source = InMemoryMigrations::new()
        .with_migration(Migration::new("3", "third", "SELECT 3", ""))
        .with_migration(Migration::new("1", "first", "SELECT 1", ""))
        .with_migration(Migration::new("2", "second", "SELECT 2", ""));

    let migrations = source.migrations().unwrap();
    println!("4. In-memory source (sorted):");
    for m in &migrations {
        println!("   {} - {}", m.version, m.name);
    }
    println!();

    // =========================================================================
    // 5. Parsing from Directory Name
    // =========================================================================

    let dir = "20240315120000_add_user_sessions";
    println!("5. Parsing '{}':", dir);
    if let Some(v) = MigrationVersion::from_directory_name(dir) {
        println!("   Version: {}", v.as_str());
    }
    if let Some(n) = MigrationName::from_directory_name(dir) {
        println!("   Name: {}", n.as_str());
    }
    println!();

    // =========================================================================
    // 6. Expected Directory Structure
    // =========================================================================

    println!("6. Expected structure:");
    println!("   migrations/");
    println!("   +-- 20240101000000_create_events/");
    println!("   |   +-- up.sql");
    println!("   |   +-- down.sql");
    println!("   +-- 20240102000000_create_users/");
    println!("       +-- up.sql");
    println!("       +-- down.sql");
    println!();

    // =========================================================================
    // 7. CLI Commands Reference
    // =========================================================================

    println!("7. CLI commands:");
    println!("   Generate: diesel-clickhouse migration generate <name>");
    println!("   Run:      diesel-clickhouse migration run");
    println!("   Revert:   diesel-clickhouse migration revert");
    println!("   List:     diesel-clickhouse migration list");

    println!("\n=== End ===");
}
