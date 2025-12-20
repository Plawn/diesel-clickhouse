//! Migrations example for diesel-clickhouse.
//!
//! Run with: cargo run --example migrations_example
//! Prerequisites: docker-compose up -d

use diesel_clickhouse::http::ClickHouseConnection;
use diesel_clickhouse_migrations::{
    Migration, MigrationHarness, MigrationConnection, MigrationSource,
    InMemoryMigrations, Result as MigrationResult,
};
use async_trait::async_trait;

// =============================================================================
// Option 1: Embed migrations from files at compile time
// =============================================================================

// This embeds all migrations from the examples/migrations/ directory at compile time.
// The migrations are baked into the binary, no filesystem access needed at runtime.
use diesel_clickhouse_migrations::EmbeddedMigrations;
use include_dir::include_dir;

static MIGRATIONS: EmbeddedMigrations = EmbeddedMigrations::new(
    include_dir!("$CARGO_MANIFEST_DIR/examples/migrations")
);

// =============================================================================
// Option 2: Define migrations in code (for this example)
// =============================================================================

fn create_in_memory_migrations() -> InMemoryMigrations {
    InMemoryMigrations::new()
        .with_migration(Migration::new(
            "20240101000000",           // version
            "create_users",             // name
            // up.sql
            r#"
                CREATE TABLE IF NOT EXISTS users (
                    id UInt64,
                    name String,
                    email String,
                    created_at DateTime DEFAULT now()
                ) ENGINE = MergeTree()
                ORDER BY (id, created_at)
            "#,
            // down.sql
            "DROP TABLE IF EXISTS users",
        ))
        .with_migration(Migration::new(
            "20240102000000",
            "create_posts",
            // up.sql
            r#"
                CREATE TABLE IF NOT EXISTS posts (
                    id UInt64,
                    user_id UInt64,
                    title String,
                    content String,
                    published Bool DEFAULT false,
                    created_at DateTime DEFAULT now()
                ) ENGINE = MergeTree()
                ORDER BY (id, created_at)
            "#,
            // down.sql
            "DROP TABLE IF EXISTS posts",
        ))
        .with_migration(Migration::new(
            "20240103000000",
            "add_user_status",
            // up.sql
            r#"
                ALTER TABLE users ADD COLUMN IF NOT EXISTS status String DEFAULT 'active'
            "#,
            // down.sql
            r#"
                ALTER TABLE users DROP COLUMN IF EXISTS status
            "#,
        ))
}

// =============================================================================
// Implement MigrationConnection for ClickHouseConnection
// =============================================================================

struct MigrationConn<'a> {
    conn: &'a ClickHouseConnection,
    database: String,
}

impl<'a> MigrationConn<'a> {
    fn new(conn: &'a ClickHouseConnection, database: &str) -> Self {
        Self {
            conn,
            database: database.to_string(),
        }
    }
}

#[async_trait]
impl MigrationConnection for MigrationConn<'_> {
    async fn execute(&mut self, sql: &str) -> MigrationResult<()> {
        self.conn.execute_raw(sql).await
            .map_err(|e| diesel_clickhouse_migrations::MigrationError::SqlError {
                migration: "".to_string(),
                message: e.to_string(),
            })
    }

    async fn query_exists(&mut self, sql: &str) -> MigrationResult<bool> {
        // For ClickHouse, we check if the query returns any rows
        let result: Option<u8> = self.conn.client()
            .query(sql)
            .fetch_optional()
            .await
            .map_err(|e| diesel_clickhouse_migrations::MigrationError::SqlError {
                migration: "".to_string(),
                message: e.to_string(),
            })?;
        Ok(result.is_some())
    }

    async fn query_scalar_string(&mut self, sql: &str) -> MigrationResult<Option<String>> {
        let result: Option<String> = self.conn.client()
            .query(sql)
            .fetch_optional()
            .await
            .map_err(|e| diesel_clickhouse_migrations::MigrationError::SqlError {
                migration: "".to_string(),
                message: e.to_string(),
            })?;
        Ok(result)
    }

    async fn query_versions(&mut self, sql: &str) -> MigrationResult<Vec<String>> {
        let versions: Vec<String> = self.conn.client()
            .query(sql)
            .fetch_all()
            .await
            .map_err(|e| diesel_clickhouse_migrations::MigrationError::SqlError {
                migration: "".to_string(),
                message: e.to_string(),
            })?;
        Ok(versions)
    }

    fn database_name(&self) -> &str {
        &self.database
    }
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Diesel-ClickHouse Migrations Example ===\n");

    let url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123/test_db".to_string());

    let conn = match ClickHouseConnection::new(&url).await {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Connection failed: {} (run: docker-compose up -d)\n", e);
            return Ok(demo_mode());
        }
    };

    // Option 1: Use embedded migrations from files (recommended for production)
    // let migrations = &MIGRATIONS;

    // Option 2: Use in-memory migrations (shown here for demo)
    let migrations = create_in_memory_migrations();

    let mut harness = MigrationConn::new(&conn, "test_db");

    // Setup migrations table
    println!("1. Setting up migrations table...");
    harness.setup_migrations_table().await?;
    println!("   Done!\n");

    // Check pending migrations
    println!("2. Checking pending migrations...");
    let pending = harness.pending_migrations(&migrations).await?;
    println!("   {} migrations pending:", pending.len());
    for m in &pending {
        println!("   - {}", m.version);
    }
    println!();

    // Run all pending migrations
    println!("3. Running pending migrations...");
    let applied = harness.run_pending_migrations(&migrations).await?;
    println!("   Applied {} migrations:", applied.len());
    for v in &applied {
        println!("   - {}", v);
    }
    println!();

    // Check applied migrations
    println!("4. Checking applied migrations...");
    let all_applied = harness.applied_migrations().await?;
    println!("   {} migrations applied:", all_applied.len());
    for v in &all_applied {
        println!("   - {}", v);
    }
    println!();

    // Get latest migration
    println!("5. Latest migration:");
    if let Some(latest) = harness.latest_migration().await? {
        println!("   {}", latest);
    }
    println!();

    // Demo: Revert last migration
    println!("6. Reverting last migration...");
    let reverted = harness.revert_migrations(&migrations, 1).await?;
    println!("   Reverted {} migrations:", reverted.len());
    for v in &reverted {
        println!("   - {}", v);
    }
    println!();

    // Demo: Redo last migration
    println!("7. Re-applying last migration...");
    let reapplied = harness.run_pending_migrations(&migrations).await?;
    println!("   Re-applied {} migrations:", reapplied.len());
    for v in &reapplied {
        println!("   - {}", v);
    }
    println!();

    // Cleanup (optional - comment out to keep tables)
    println!("8. Cleaning up (reverting all migrations)...");
    let reverted = harness.revert_migrations(&migrations, 10).await?;
    println!("   Reverted {} migrations", reverted.len());

    // Drop migrations table
    conn.execute_raw("DROP TABLE IF EXISTS __diesel_schema_migrations").await?;
    println!("   Dropped migrations table");

    println!("\nDone!");

    Ok(())
}

fn demo_mode() {
    println!("=== Demo Mode (No DB Connection) ===\n");

    // Show embedded migrations (from files)
    println!("=== Embedded Migrations (from examples/migrations/) ===");
    for m in MIGRATIONS.migrations().unwrap() {
        println!("\n--- {} ---", m.version);
        println!("UP SQL:");
        for line in m.up_sql.lines().filter(|l| !l.trim().is_empty()) {
            println!("  {}", line.trim());
        }
        println!("DOWN SQL:");
        for line in m.down_sql.lines().filter(|l| !l.trim().is_empty()) {
            println!("  {}", line.trim());
        }
    }

    // Show in-memory migrations
    println!("\n\n=== In-Memory Migrations (defined in code) ===");
    let migrations = create_in_memory_migrations();
    for m in migrations.migrations().unwrap() {
        println!("\n--- {} ---", m.version);
        println!("UP SQL:");
        for line in m.up_sql.lines().filter(|l| !l.trim().is_empty()) {
            println!("  {}", line.trim());
        }
        println!("DOWN SQL:");
        for line in m.down_sql.lines().filter(|l| !l.trim().is_empty()) {
            println!("  {}", line.trim());
        }
    }

    println!("\n\nTo run migrations with a real database:");
    println!("  1. Start ClickHouse: docker-compose up -d");
    println!("  2. Run: cargo run --example migrations_example");
}
