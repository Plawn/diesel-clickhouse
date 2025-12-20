//! Migration system for diesel-clickhouse.
//!
//! This module provides a Diesel-inspired migration system for ClickHouse,
//! supporting both file-based and embedded migrations.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use diesel_clickhouse_migrations::{Migration, MigrationHarness, embed_migrations};
//!
//! // Embed migrations at compile time
//! embed_migrations!("migrations");
//!
//! // Run migrations
//! async fn run_migrations(conn: &mut impl MigrationConnection) {
//!     conn.run_pending_migrations(MIGRATIONS).await.unwrap();
//! }
//! ```
//!
//! ## Migration Directory Structure
//!
//! ```text
//! migrations/
//! ├── 00000000000000_diesel_initial_setup/
//! │   ├── up.sql
//! │   └── down.sql
//! ├── 2024_01_15_120000_create_users/
//! │   ├── up.sql
//! │   └── down.sql
//! └── 2024_01_16_090000_add_events_table/
//!     ├── up.sql
//!     └── down.sql
//! ```

pub mod error;
pub mod migration;
pub mod harness;
pub mod source;
pub mod table;

pub use error::{MigrationError, Result};
pub use migration::{Migration, MigrationVersion, MigrationName, MigrationMetadata, MigrationBuilder};
pub use harness::{MigrationHarness, MigrationConnection};
pub use source::{MigrationSource, FileBasedMigrations, EmbeddedMigrations, InMemoryMigrations};
pub use table::MIGRATIONS_TABLE;

/// Re-export for embed_migrations macro
pub use include_dir;

/// The default migrations table name in ClickHouse.
pub const DEFAULT_MIGRATIONS_TABLE: &str = "__diesel_schema_migrations";

/// Embed migrations from a directory at compile time.
///
/// This macro embeds all migrations from the specified directory into your binary,
/// allowing you to run migrations without needing the migration files at runtime.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse_migrations::embed_migrations;
///
/// embed_migrations!("migrations");
///
/// // MIGRATIONS is now available as a static
/// async fn setup(conn: &mut impl MigrationConnection) {
///     conn.run_pending_migrations(MIGRATIONS).await.unwrap();
/// }
/// ```
#[macro_export]
macro_rules! embed_migrations {
    ($dir:literal) => {
        /// Embedded migrations from compile time.
        pub static MIGRATIONS: $crate::EmbeddedMigrations = $crate::EmbeddedMigrations::new(
            $crate::include_dir::include_dir!($dir)
        );
    };
}

/// Generate a migration name with timestamp.
pub fn generate_migration_name(name: &str) -> String {
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    format!("{}_{}", timestamp, name.replace(' ', "_").to_lowercase())
}
