//! Schema migrations tracking table.

use crate::DEFAULT_MIGRATIONS_TABLE;

/// SQL for the migrations tracking table.
pub struct MigrationsTable {
    /// The table name.
    pub name: String,
}

impl MigrationsTable {
    /// Create with default table name.
    pub fn new() -> Self {
        Self {
            name: DEFAULT_MIGRATIONS_TABLE.to_string(),
        }
    }

    /// Create with custom table name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// SQL to create the migrations table.
    ///
    /// Uses ReplacingMergeTree to ensure deduplication on version.
    pub fn create_table_sql(&self) -> String {
        format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                version String,
                run_on DateTime64(3) DEFAULT now64(3),
                checksum Nullable(String)
            )
            ENGINE = ReplacingMergeTree(run_on)
            ORDER BY version
            "#,
            self.name
        )
    }

    /// SQL to check if the table exists.
    pub fn exists_sql(&self, database: &str) -> String {
        format!(
            "SELECT 1 FROM system.tables WHERE database = '{}' AND name = '{}'",
            database, self.name
        )
    }

    /// SQL to insert a new migration record.
    pub fn insert_sql(&self) -> String {
        format!(
            "INSERT INTO {} (version, run_on, checksum) VALUES",
            self.name
        )
    }

    /// SQL to delete a migration record.
    pub fn delete_sql(&self, version: &str) -> String {
        format!(
            "ALTER TABLE {} DELETE WHERE version = '{}'",
            self.name, version
        )
    }

    /// SQL to select all run migrations.
    pub fn select_all_sql(&self) -> String {
        format!(
            "SELECT version, run_on, checksum FROM {} FINAL ORDER BY version",
            self.name
        )
    }

    /// SQL to select a specific migration.
    pub fn select_one_sql(&self, version: &str) -> String {
        format!(
            "SELECT version, run_on, checksum FROM {} FINAL WHERE version = '{}'",
            self.name, version
        )
    }

    /// SQL to get the latest migration version.
    pub fn select_latest_sql(&self) -> String {
        format!(
            "SELECT version FROM {} FINAL ORDER BY version DESC LIMIT 1",
            self.name
        )
    }

    /// SQL to count migrations.
    pub fn count_sql(&self) -> String {
        format!("SELECT count() FROM {} FINAL", self.name)
    }
}

impl Default for MigrationsTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Static instance for the default migrations table.
pub static MIGRATIONS_TABLE: std::sync::LazyLock<MigrationsTable> =
    std::sync::LazyLock::new(MigrationsTable::new);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_table_sql() {
        let table = MigrationsTable::new();
        let sql = table.create_table_sql();
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS"));
        assert!(sql.contains("version String"));
        assert!(sql.contains("ReplacingMergeTree"));
    }

    #[test]
    fn test_custom_table_name() {
        let table = MigrationsTable::with_name("my_migrations");
        assert_eq!(table.name, "my_migrations");
        assert!(table.create_table_sql().contains("my_migrations"));
    }
}
