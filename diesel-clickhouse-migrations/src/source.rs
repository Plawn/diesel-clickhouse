//! Migration sources (file-based and embedded).

use std::path::{Path, PathBuf};
use include_dir::{Dir, DirEntry};

use crate::error::{MigrationError, Result};
use crate::migration::Migration;

/// A source of migrations.
pub trait MigrationSource {
    /// Get all migrations from this source, sorted by version.
    fn migrations(&self) -> Result<Vec<Migration>>;
}

// =============================================================================
// File-based migrations
// =============================================================================

/// Migrations loaded from the filesystem.
#[derive(Debug, Clone)]
pub struct FileBasedMigrations {
    path: PathBuf,
}

impl FileBasedMigrations {
    /// Create a new file-based migration source.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Try to find migrations in the current directory or parent directories.
    pub fn find_migrations_directory() -> Result<Self> {
        let mut current = std::env::current_dir()?;

        loop {
            let migrations_path = current.join("migrations");
            if migrations_path.is_dir() {
                return Ok(Self::new(migrations_path));
            }

            if !current.pop() {
                break;
            }
        }

        Err(MigrationError::DirectoryNotFound(PathBuf::from("migrations")))
    }

    /// Get the path to this migration source.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Create a new migration directory with up.sql and down.sql.
    pub fn create_migration(&self, name: &str) -> Result<PathBuf> {
        let version = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
        let dir_name = format!("{}_{}", version, name.replace(' ', "_").to_lowercase());
        let migration_path = self.path.join(&dir_name);

        std::fs::create_dir_all(&migration_path)?;

        let up_path = migration_path.join("up.sql");
        let down_path = migration_path.join("down.sql");

        std::fs::write(&up_path, "-- Your SQL goes here\n")?;
        std::fs::write(&down_path, "-- This file should undo anything in `up.sql`\n")?;

        Ok(migration_path)
    }
}

impl MigrationSource for FileBasedMigrations {
    fn migrations(&self) -> Result<Vec<Migration>> {
        if !self.path.exists() {
            return Err(MigrationError::DirectoryNotFound(self.path.clone()));
        }

        let mut migrations = Vec::new();

        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };

            // Skip hidden directories
            if dir_name.starts_with('.') {
                continue;
            }

            let up_path = path.join("up.sql");
            let down_path = path.join("down.sql");

            if !up_path.exists() {
                return Err(MigrationError::FileNotFound {
                    path: up_path,
                    expected: "up.sql",
                });
            }

            let up_sql = std::fs::read_to_string(&up_path)?;
            let down_sql = if down_path.exists() {
                std::fs::read_to_string(&down_path)?
            } else {
                String::new()
            };

            if let Some(migration) = Migration::from_parts(dir_name, up_sql, down_sql) {
                migrations.push(migration);
            } else {
                return Err(MigrationError::InvalidMigrationName {
                    name: dir_name.to_string(),
                });
            }
        }

        migrations.sort();
        Ok(migrations)
    }
}

// =============================================================================
// Embedded migrations
// =============================================================================

/// Migrations embedded at compile time using include_dir.
pub struct EmbeddedMigrations {
    dir: Dir<'static>,
}

impl EmbeddedMigrations {
    /// Create a new embedded migrations source.
    pub const fn new(dir: Dir<'static>) -> Self {
        Self { dir }
    }
}

impl MigrationSource for EmbeddedMigrations {
    fn migrations(&self) -> Result<Vec<Migration>> {
        let mut migrations = Vec::new();

        for entry in self.dir.entries() {
            if let DirEntry::Dir(dir) = entry {
                let dir_name = dir.path().file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                // Skip hidden directories
                if dir_name.starts_with('.') {
                    continue;
                }

                // Find up.sql and down.sql in this directory's files
                let mut up_sql = "";
                let mut down_sql = "";

                for file in dir.files() {
                    if let Some(name) = file.path().file_name().and_then(|n| n.to_str()) {
                        match name {
                            "up.sql" => up_sql = file.contents_utf8().unwrap_or(""),
                            "down.sql" => down_sql = file.contents_utf8().unwrap_or(""),
                            _ => {}
                        }
                    }
                }

                if let Some(migration) = Migration::from_parts(dir_name, up_sql, down_sql) {
                    migrations.push(migration);
                }
            }
        }

        migrations.sort();
        Ok(migrations)
    }
}

// =============================================================================
// In-memory migrations (for testing)
// =============================================================================

/// In-memory migrations for testing purposes.
#[derive(Debug, Clone, Default)]
pub struct InMemoryMigrations {
    migrations: Vec<Migration>,
}

impl InMemoryMigrations {
    /// Create a new empty in-memory migration source.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a migration.
    pub fn add(&mut self, migration: Migration) {
        self.migrations.push(migration);
    }

    /// Add a migration using builder-style.
    pub fn with_migration(mut self, migration: Migration) -> Self {
        self.migrations.push(migration);
        self
    }
}

impl MigrationSource for InMemoryMigrations {
    fn migrations(&self) -> Result<Vec<Migration>> {
        let mut migrations = self.migrations.clone();
        migrations.sort();
        Ok(migrations)
    }
}

// =============================================================================
// Combined migrations source
// =============================================================================

/// Combine multiple migration sources.
pub struct CombinedMigrations {
    sources: Vec<Box<dyn MigrationSource + Send + Sync>>,
}

impl CombinedMigrations {
    /// Create a new combined migrations source.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Add a migration source.
    pub fn add<S: MigrationSource + Send + Sync + 'static>(mut self, source: S) -> Self {
        self.sources.push(Box::new(source));
        self
    }
}

impl Default for CombinedMigrations {
    fn default() -> Self {
        Self::new()
    }
}

impl MigrationSource for CombinedMigrations {
    fn migrations(&self) -> Result<Vec<Migration>> {
        let mut all_migrations = Vec::new();

        for source in &self.sources {
            all_migrations.extend(source.migrations()?);
        }

        all_migrations.sort();

        // Check for duplicate versions
        for window in all_migrations.windows(2) {
            if window[0].version == window[1].version {
                return Err(MigrationError::MigrationExists(
                    window[0].version.to_string(),
                ));
            }
        }

        Ok(all_migrations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::MigrationVersion;

    #[test]
    fn test_in_memory_migrations() {
        let source = InMemoryMigrations::new()
            .with_migration(Migration::new("20240115120000", "create_users", "CREATE TABLE users", "DROP TABLE users"))
            .with_migration(Migration::new("20240116090000", "add_events", "CREATE TABLE events", "DROP TABLE events"));

        let migrations = source.migrations().unwrap();
        assert_eq!(migrations.len(), 2);
        assert_eq!(migrations[0].version, MigrationVersion::new("20240115120000"));
        assert_eq!(migrations[1].version, MigrationVersion::new("20240116090000"));
    }
}
