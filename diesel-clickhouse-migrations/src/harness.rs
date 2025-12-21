//! Migration harness for running migrations.

use async_trait::async_trait;

use crate::error::{MigrationError, Result};
use crate::migration::{Migration, MigrationVersion};
use crate::source::MigrationSource;
use crate::table::MigrationsTable;

// =============================================================================
// SQL Escaping Utilities
// =============================================================================

/// Escape a string value for use in SQL single-quoted strings.
/// Escapes single quotes by doubling them.
#[inline]
fn escape_sql_string(s: &str) -> String {
    if s.contains('\'') {
        s.replace('\'', "''")
    } else {
        s.to_string()
    }
}

/// A connection that can run migrations.
#[async_trait]
pub trait MigrationConnection: Send {
    /// Execute a SQL statement.
    async fn execute(&mut self, sql: &str) -> Result<()>;

    /// Execute a query and return whether any rows exist.
    async fn query_exists(&mut self, sql: &str) -> Result<bool>;

    /// Execute a query and return the first column of the first row as a string.
    async fn query_scalar_string(&mut self, sql: &str) -> Result<Option<String>>;

    /// Execute a query and return all versions.
    async fn query_versions(&mut self, sql: &str) -> Result<Vec<String>>;

    /// Get the database name for this connection.
    fn database_name(&self) -> &str;
}

/// The migration harness that manages running migrations.
#[async_trait]
pub trait MigrationHarness: MigrationConnection {
    /// Ensure the migrations table exists.
    async fn setup_migrations_table(&mut self) -> Result<()> {
        let table = MigrationsTable::new();
        self.execute(&table.create_table_sql()).await
    }

    /// Check if the migrations table exists.
    async fn migrations_table_exists(&mut self) -> Result<bool> {
        let table = MigrationsTable::new();
        let sql = table.exists_sql(self.database_name());
        self.query_exists(&sql).await
    }

    /// Get all applied migration versions.
    async fn applied_migrations(&mut self) -> Result<Vec<MigrationVersion>> {
        let table = MigrationsTable::new();
        let versions = self.query_versions(&table.select_all_sql()).await?;
        Ok(versions.into_iter().map(MigrationVersion::new).collect())
    }

    /// Check if a specific migration has been applied.
    async fn has_migration(&mut self, version: &MigrationVersion) -> Result<bool> {
        let table = MigrationsTable::new();
        let sql = table.select_one_sql(version.as_str());
        self.query_exists(&sql).await
    }

    /// Get the latest applied migration version.
    async fn latest_migration(&mut self) -> Result<Option<MigrationVersion>> {
        let table = MigrationsTable::new();
        let sql = table.select_latest_sql();
        match self.query_scalar_string(&sql).await? {
            Some(version) => Ok(Some(MigrationVersion::new(version))),
            None => Ok(None),
        }
    }

    /// Get pending migrations that haven't been applied yet.
    async fn pending_migrations<S: MigrationSource + Send + Sync>(
        &mut self,
        source: &S,
    ) -> Result<Vec<Migration>> {
        let applied = self.applied_migrations().await?;
        let all = source.migrations()?;

        Ok(all
            .into_iter()
            .filter(|m| !applied.contains(&m.version))
            .collect())
    }

    /// Run a single migration.
    async fn run_migration(&mut self, migration: &Migration) -> Result<()> {
        // Execute the up SQL
        // Split by semicolons and execute each statement
        for statement in split_sql_statements(&migration.up_sql) {
            let trimmed = statement.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("--") {
                self.execute(trimmed).await.map_err(|e| {
                    MigrationError::sql_error(&migration.version.to_string(), e.to_string())
                })?;
            }
        }

        // Record the migration
        let table = MigrationsTable::new();
        let checksum = migration.checksum();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        // Escape all values to prevent SQL injection
        let insert_sql = format!(
            "{} ('{}', '{}', '{}')",
            table.insert_sql(),
            escape_sql_string(&migration.version.to_string()),
            escape_sql_string(&now),
            escape_sql_string(&checksum)
        );
        self.execute(&insert_sql).await?;

        Ok(())
    }

    /// Revert a single migration.
    async fn revert_migration(&mut self, migration: &Migration) -> Result<()> {
        // Check if migration exists
        if !self.has_migration(&migration.version).await? {
            return Err(MigrationError::MigrationNotFound(
                migration.version.to_string(),
            ));
        }

        // Execute the down SQL
        for statement in split_sql_statements(&migration.down_sql) {
            let trimmed = statement.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("--") {
                self.execute(trimmed).await.map_err(|e| {
                    MigrationError::sql_error(&migration.version.to_string(), e.to_string())
                })?;
            }
        }

        // Remove the migration record
        let table = MigrationsTable::new();
        self.execute(&table.delete_sql(migration.version.as_str())).await?;

        Ok(())
    }

    /// Run all pending migrations.
    async fn run_pending_migrations<S: MigrationSource + Send + Sync>(
        &mut self,
        source: &S,
    ) -> Result<Vec<MigrationVersion>> {
        self.setup_migrations_table().await?;

        let pending = self.pending_migrations(source).await?;
        if pending.is_empty() {
            return Ok(Vec::new());
        }

        let mut applied = Vec::new();
        for migration in &pending {
            self.run_migration(migration).await?;
            applied.push(migration.version.clone());
        }

        Ok(applied)
    }

    /// Revert the last N migrations.
    async fn revert_migrations<S: MigrationSource + Send + Sync>(
        &mut self,
        source: &S,
        count: usize,
    ) -> Result<Vec<MigrationVersion>> {
        let applied = self.applied_migrations().await?;
        if applied.is_empty() {
            return Err(MigrationError::NoMigrationsToRevert);
        }

        let all_migrations = source.migrations()?;

        // Get the last N applied migrations in reverse order
        let to_revert: Vec<_> = applied
            .into_iter()
            .rev()
            .take(count)
            .collect();

        let mut reverted = Vec::new();
        for version in to_revert {
            // Find the migration
            let migration = all_migrations
                .iter()
                .find(|m| m.version == version)
                .ok_or_else(|| MigrationError::MigrationNotFound(version.to_string()))?;

            self.revert_migration(migration).await?;
            reverted.push(version);
        }

        Ok(reverted)
    }

    /// Revert and re-apply the last N migrations.
    async fn redo_migrations<S: MigrationSource + Send + Sync>(
        &mut self,
        source: &S,
        count: usize,
    ) -> Result<Vec<MigrationVersion>> {
        let all_migrations = source.migrations()?;
        let applied = self.applied_migrations().await?;

        if applied.is_empty() {
            return Err(MigrationError::NoMigrationsToRevert);
        }

        // Get the last N applied migrations
        let to_redo: Vec<_> = applied.into_iter().rev().take(count).collect();

        // Revert them
        for version in &to_redo {
            let migration = all_migrations
                .iter()
                .find(|m| &m.version == version)
                .ok_or_else(|| MigrationError::MigrationNotFound(version.to_string()))?;

            self.revert_migration(migration).await?;
        }

        // Re-apply them in order
        let mut reapplied = Vec::new();
        for version in to_redo.into_iter().rev() {
            let migration = all_migrations
                .iter()
                .find(|m| m.version == version)
                .ok_or_else(|| MigrationError::MigrationNotFound(version.to_string()))?;

            self.run_migration(migration).await?;
            reapplied.push(version);
        }

        Ok(reapplied)
    }

    /// Run migrations up to a specific version.
    async fn run_to_version<S: MigrationSource + Send + Sync>(
        &mut self,
        source: &S,
        target: &MigrationVersion,
    ) -> Result<Vec<MigrationVersion>> {
        self.setup_migrations_table().await?;

        let pending = self.pending_migrations(source).await?;
        let mut applied = Vec::new();

        for migration in pending {
            if &migration.version > target {
                break;
            }
            self.run_migration(&migration).await?;
            applied.push(migration.version.clone());
        }

        Ok(applied)
    }

    /// Revert migrations down to a specific version (exclusive).
    async fn revert_to_version<S: MigrationSource + Send + Sync>(
        &mut self,
        source: &S,
        target: &MigrationVersion,
    ) -> Result<Vec<MigrationVersion>> {
        let all_migrations = source.migrations()?;
        let applied = self.applied_migrations().await?;

        let to_revert: Vec<_> = applied
            .into_iter()
            .filter(|v| v > target)
            .collect();

        let mut reverted = Vec::new();
        for version in to_revert.into_iter().rev() {
            let migration = all_migrations
                .iter()
                .find(|m| m.version == version)
                .ok_or_else(|| MigrationError::MigrationNotFound(version.to_string()))?;

            self.revert_migration(migration).await?;
            reverted.push(version);
        }

        Ok(reverted)
    }
}

// Blanket implementation for all MigrationConnection types
impl<T: MigrationConnection + Send> MigrationHarness for T {}

/// Split SQL into individual statements.
///
/// Handles:
/// - Semicolon-separated statements
/// - Comments (-- and /* */)
/// - String literals
fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut string_char = '"';
    let mut in_comment = false;
    let mut in_block_comment = false;
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        // Handle block comments
        if !in_string && !in_comment && c == '/' && chars.peek() == Some(&'*') {
            in_block_comment = true;
            current.push(c);
            // SAFETY: We just verified peek() == Some(&'*'), so next() will return Some('*')
            current.push(chars.next().expect("peek() returned Some, so next() must succeed"));
            continue;
        }
        if in_block_comment && c == '*' && chars.peek() == Some(&'/') {
            in_block_comment = false;
            current.push(c);
            // SAFETY: We just verified peek() == Some(&'/'), so next() will return Some('/')
            current.push(chars.next().expect("peek() returned Some, so next() must succeed"));
            continue;
        }
        if in_block_comment {
            current.push(c);
            continue;
        }

        // Handle line comments
        if !in_string && c == '-' && chars.peek() == Some(&'-') {
            in_comment = true;
            current.push(c);
            continue;
        }
        if in_comment {
            current.push(c);
            if c == '\n' {
                in_comment = false;
            }
            continue;
        }

        // Handle strings
        if !in_string && (c == '\'' || c == '"') {
            in_string = true;
            string_char = c;
            current.push(c);
            continue;
        }
        if in_string && c == string_char {
            in_string = false;
            current.push(c);
            continue;
        }

        // Handle statement separator
        if !in_string && c == ';' {
            let stmt = current.trim().to_string();
            if !stmt.is_empty() {
                statements.push(stmt);
            }
            current.clear();
            continue;
        }

        current.push(c);
    }

    // Don't forget the last statement
    let stmt = current.trim().to_string();
    if !stmt.is_empty() {
        statements.push(stmt);
    }

    statements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_sql_statements() {
        let sql = r#"
            CREATE TABLE users (id UInt64);
            CREATE TABLE events (id UInt64);
        "#;

        let statements = split_sql_statements(sql);
        assert_eq!(statements.len(), 2);
        assert!(statements[0].contains("users"));
        assert!(statements[1].contains("events"));
    }

    #[test]
    fn test_split_sql_with_comments() {
        let sql = r#"
            -- This is a comment
            CREATE TABLE users (id UInt64);
            /* Block comment */
            CREATE TABLE events (id UInt64);
        "#;

        let statements = split_sql_statements(sql);
        assert_eq!(statements.len(), 2);
    }

    #[test]
    fn test_split_sql_with_strings() {
        let sql = r#"
            INSERT INTO users VALUES (1, 'hello; world');
            INSERT INTO users VALUES (2, 'test');
        "#;

        let statements = split_sql_statements(sql);
        assert_eq!(statements.len(), 2);
        assert!(statements[0].contains("hello; world"));
    }
}
