//! Unit tests for diesel-clickhouse-migrations.

use diesel_clickhouse_migrations::*;
use diesel_clickhouse_migrations::migration::*;
use diesel_clickhouse_migrations::source::*;
use diesel_clickhouse_migrations::error::MigrationError;
use std::path::PathBuf;
use tempfile::TempDir;

// =============================================================================
// MigrationVersion Tests
// =============================================================================

mod version_tests {
    use super::*;

    #[test]
    fn test_version_new() {
        let version = MigrationVersion::new("20240115120000");
        assert_eq!(version.as_str(), "20240115120000");
    }

    #[test]
    fn test_version_display() {
        let version = MigrationVersion::new("12345");
        assert_eq!(format!("{}", version), "12345");
    }

    #[test]
    fn test_version_from_directory_name_timestamp() {
        let version = MigrationVersion::from_directory_name("20240115120000_create_users");
        assert!(version.is_some());
        assert_eq!(version.unwrap().as_str(), "20240115120000");
    }

    #[test]
    fn test_version_from_directory_name_sequential() {
        let version = MigrationVersion::from_directory_name("00001_create_users");
        assert!(version.is_some());
        assert_eq!(version.unwrap().as_str(), "00001");
    }

    #[test]
    fn test_version_from_directory_name_invalid() {
        let version = MigrationVersion::from_directory_name("invalid_name");
        assert!(version.is_none());
    }

    #[test]
    fn test_version_ordering_numeric() {
        let v1 = MigrationVersion::new("1");
        let v2 = MigrationVersion::new("2");
        let v10 = MigrationVersion::new("10");

        assert!(v1 < v2);
        assert!(v2 < v10);
        assert!(v1 < v10);
    }

    #[test]
    fn test_version_ordering_timestamp() {
        let v1 = MigrationVersion::new("20240115100000");
        let v2 = MigrationVersion::new("20240115120000");
        let v3 = MigrationVersion::new("20240116090000");

        assert!(v1 < v2);
        assert!(v2 < v3);
    }

    #[test]
    fn test_version_equality() {
        let v1 = MigrationVersion::new("12345");
        let v2 = MigrationVersion::new("12345");
        let v3 = MigrationVersion::new("12346");

        assert_eq!(v1, v2);
        assert_ne!(v1, v3);
    }

    #[test]
    fn test_version_generate() {
        let v1 = MigrationVersion::generate();
        let v2 = MigrationVersion::generate();

        // Generated versions should be numeric strings of 14 digits
        assert_eq!(v1.as_str().len(), 14);
        assert!(v1.as_str().chars().all(|c| c.is_ascii_digit()));

        // Should be monotonically increasing or equal (within same second)
        assert!(v1 <= v2);
    }
}

// =============================================================================
// MigrationName Tests
// =============================================================================

mod name_tests {
    use super::*;

    #[test]
    fn test_name_new() {
        let name = MigrationName::new("create_users");
        assert_eq!(name.as_str(), "create_users");
    }

    #[test]
    fn test_name_display() {
        let name = MigrationName::new("add_index");
        assert_eq!(format!("{}", name), "add_index");
    }

    #[test]
    fn test_name_from_directory_name() {
        let name = MigrationName::from_directory_name("20240115120000_create_users");
        assert!(name.is_some());
        assert_eq!(name.unwrap().as_str(), "create_users");
    }

    #[test]
    fn test_name_from_directory_name_with_underscores() {
        let name = MigrationName::from_directory_name("00001_add_user_email_index");
        assert!(name.is_some());
        assert_eq!(name.unwrap().as_str(), "add_user_email_index");
    }

    #[test]
    fn test_name_from_directory_name_no_underscore() {
        let name = MigrationName::from_directory_name("20240115120000");
        assert!(name.is_none());
    }
}

// =============================================================================
// Migration Tests
// =============================================================================

mod migration_tests {
    use super::*;

    #[test]
    fn test_migration_new() {
        let migration = Migration::new(
            "20240115120000",
            "create_users",
            "CREATE TABLE users (id UInt64)",
            "DROP TABLE users",
        );

        assert_eq!(migration.version.as_str(), "20240115120000");
        assert_eq!(migration.name.as_str(), "create_users");
        assert_eq!(migration.up_sql, "CREATE TABLE users (id UInt64)");
        assert_eq!(migration.down_sql, "DROP TABLE users");
    }

    #[test]
    fn test_migration_directory_name() {
        let migration = Migration::new(
            "20240115120000",
            "create users table",
            "",
            "",
        );

        assert_eq!(migration.directory_name(), "20240115120000_create_users_table");
    }

    #[test]
    fn test_migration_checksum() {
        let migration = Migration::new(
            "1",
            "test",
            "SELECT 1",
            "",
        );

        let checksum = migration.checksum();

        // Checksum should be consistent
        assert_eq!(migration.checksum(), checksum);

        // Checksum should be a hex string of 16 chars
        assert_eq!(checksum.len(), 16);
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_migration_checksum_differs_by_content() {
        let m1 = Migration::new("1", "test", "SELECT 1", "");
        let m2 = Migration::new("1", "test", "SELECT 2", "");

        assert_ne!(m1.checksum(), m2.checksum());
    }

    #[test]
    fn test_migration_from_parts() {
        let migration = Migration::from_parts(
            "20240115120000_create_users",
            "CREATE TABLE users",
            "DROP TABLE users",
        );

        assert!(migration.is_some());
        let m = migration.unwrap();
        assert_eq!(m.version.as_str(), "20240115120000");
        assert_eq!(m.name.as_str(), "create_users");
    }

    #[test]
    fn test_migration_from_parts_invalid() {
        let migration = Migration::from_parts("invalid", "", "");
        assert!(migration.is_none());
    }

    #[test]
    fn test_migration_ordering() {
        let m1 = Migration::new("1", "first", "", "");
        let m2 = Migration::new("2", "second", "", "");
        let m3 = Migration::new("10", "tenth", "", "");

        assert!(m1 < m2);
        assert!(m2 < m3);
    }

    #[test]
    fn test_migration_equality() {
        let m1 = Migration::new("1", "test", "sql1", "");
        let m2 = Migration::new("1", "other", "sql2", "");
        let m3 = Migration::new("2", "test", "sql1", "");

        // Migrations are equal if versions are equal
        assert_eq!(m1, m2);
        assert_ne!(m1, m3);
    }
}

// =============================================================================
// MigrationBuilder Tests
// =============================================================================

mod builder_tests {
    use super::*;

    #[test]
    fn test_builder_complete() {
        let migration = MigrationBuilder::new()
            .version("20240115120000")
            .name("create_users")
            .up("CREATE TABLE users")
            .down("DROP TABLE users")
            .build();

        assert!(migration.is_some());
        let m = migration.unwrap();
        assert_eq!(m.version.as_str(), "20240115120000");
        assert_eq!(m.name.as_str(), "create_users");
    }

    #[test]
    fn test_builder_incomplete_version() {
        let migration = MigrationBuilder::new()
            .name("test")
            .up("SELECT 1")
            .down("")
            .build();

        assert!(migration.is_none());
    }

    #[test]
    fn test_builder_incomplete_name() {
        let migration = MigrationBuilder::new()
            .version("1")
            .up("SELECT 1")
            .down("")
            .build();

        assert!(migration.is_none());
    }

    #[test]
    fn test_builder_incomplete_up() {
        let migration = MigrationBuilder::new()
            .version("1")
            .name("test")
            .down("")
            .build();

        assert!(migration.is_none());
    }

    #[test]
    fn test_builder_default() {
        let builder = MigrationBuilder::default();
        let migration = builder.build();
        assert!(migration.is_none());
    }
}

// =============================================================================
// MigrationMetadata Tests
// =============================================================================

mod metadata_tests {
    use super::*;

    #[test]
    fn test_metadata_new() {
        let version = MigrationVersion::new("123");
        let meta = MigrationMetadata::new(version);

        assert_eq!(meta.version.as_str(), "123");
        assert!(meta.checksum.is_none());
    }

    #[test]
    fn test_metadata_with_checksum() {
        let version = MigrationVersion::new("456");
        let meta = MigrationMetadata::with_checksum(version, "abc123".to_string());

        assert_eq!(meta.version.as_str(), "456");
        assert_eq!(meta.checksum, Some("abc123".to_string()));
    }
}

// =============================================================================
// InMemoryMigrations Tests
// =============================================================================

mod in_memory_source_tests {
    use super::*;

    #[test]
    fn test_empty_source() {
        let source = InMemoryMigrations::new();
        let migrations = source.migrations().unwrap();
        assert!(migrations.is_empty());
    }

    #[test]
    fn test_add_migration() {
        let mut source = InMemoryMigrations::new();
        source.add(Migration::new("1", "first", "", ""));

        let migrations = source.migrations().unwrap();
        assert_eq!(migrations.len(), 1);
    }

    #[test]
    fn test_with_migration_builder_pattern() {
        let source = InMemoryMigrations::new()
            .with_migration(Migration::new("1", "first", "", ""))
            .with_migration(Migration::new("2", "second", "", ""));

        let migrations = source.migrations().unwrap();
        assert_eq!(migrations.len(), 2);
    }

    #[test]
    fn test_migrations_are_sorted() {
        let source = InMemoryMigrations::new()
            .with_migration(Migration::new("3", "third", "", ""))
            .with_migration(Migration::new("1", "first", "", ""))
            .with_migration(Migration::new("2", "second", "", ""));

        let migrations = source.migrations().unwrap();
        assert_eq!(migrations[0].version.as_str(), "1");
        assert_eq!(migrations[1].version.as_str(), "2");
        assert_eq!(migrations[2].version.as_str(), "3");
    }
}

// =============================================================================
// FileBasedMigrations Tests
// =============================================================================

mod file_based_source_tests {
    use super::*;

    #[test]
    fn test_path_accessor() {
        let source = FileBasedMigrations::new("/some/path");
        assert_eq!(source.path(), PathBuf::from("/some/path"));
    }

    #[test]
    fn test_nonexistent_directory() {
        let source = FileBasedMigrations::new("/nonexistent/directory");
        let result = source.migrations();
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let source = FileBasedMigrations::new(temp_dir.path());

        let migrations = source.migrations().unwrap();
        assert!(migrations.is_empty());
    }

    #[test]
    fn test_load_single_migration() {
        let temp_dir = TempDir::new().unwrap();
        let migration_dir = temp_dir.path().join("20240115120000_create_users");
        std::fs::create_dir_all(&migration_dir).unwrap();

        std::fs::write(migration_dir.join("up.sql"), "CREATE TABLE users").unwrap();
        std::fs::write(migration_dir.join("down.sql"), "DROP TABLE users").unwrap();

        let source = FileBasedMigrations::new(temp_dir.path());
        let migrations = source.migrations().unwrap();

        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].version.as_str(), "20240115120000");
        assert_eq!(migrations[0].name.as_str(), "create_users");
        assert_eq!(migrations[0].up_sql, "CREATE TABLE users");
        assert_eq!(migrations[0].down_sql, "DROP TABLE users");
    }

    #[test]
    fn test_load_multiple_migrations_sorted() {
        let temp_dir = TempDir::new().unwrap();

        // Create migrations out of order
        for (version, name) in [
            ("20240117000000", "third"),
            ("20240115000000", "first"),
            ("20240116000000", "second"),
        ] {
            let dir = temp_dir.path().join(format!("{}_{}", version, name));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("up.sql"), "SELECT 1").unwrap();
            std::fs::write(dir.join("down.sql"), "").unwrap();
        }

        let source = FileBasedMigrations::new(temp_dir.path());
        let migrations = source.migrations().unwrap();

        assert_eq!(migrations.len(), 3);
        assert_eq!(migrations[0].name.as_str(), "first");
        assert_eq!(migrations[1].name.as_str(), "second");
        assert_eq!(migrations[2].name.as_str(), "third");
    }

    #[test]
    fn test_skip_hidden_directories() {
        let temp_dir = TempDir::new().unwrap();

        // Create a valid migration
        let valid_dir = temp_dir.path().join("00001_valid");
        std::fs::create_dir_all(&valid_dir).unwrap();
        std::fs::write(valid_dir.join("up.sql"), "SELECT 1").unwrap();

        // Create a hidden directory
        let hidden_dir = temp_dir.path().join(".hidden");
        std::fs::create_dir_all(&hidden_dir).unwrap();
        std::fs::write(hidden_dir.join("up.sql"), "SELECT 2").unwrap();

        let source = FileBasedMigrations::new(temp_dir.path());
        let migrations = source.migrations().unwrap();

        assert_eq!(migrations.len(), 1);
    }

    #[test]
    fn test_missing_up_sql() {
        let temp_dir = TempDir::new().unwrap();
        let migration_dir = temp_dir.path().join("00001_test");
        std::fs::create_dir_all(&migration_dir).unwrap();
        // Only create down.sql, not up.sql

        let source = FileBasedMigrations::new(temp_dir.path());
        let result = source.migrations();

        assert!(result.is_err());
    }

    #[test]
    fn test_missing_down_sql_is_ok() {
        let temp_dir = TempDir::new().unwrap();
        let migration_dir = temp_dir.path().join("00001_test");
        std::fs::create_dir_all(&migration_dir).unwrap();
        std::fs::write(migration_dir.join("up.sql"), "SELECT 1").unwrap();
        // No down.sql

        let source = FileBasedMigrations::new(temp_dir.path());
        let migrations = source.migrations().unwrap();

        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].down_sql, "");
    }

    #[test]
    fn test_create_migration() {
        let temp_dir = TempDir::new().unwrap();
        let source = FileBasedMigrations::new(temp_dir.path());

        let path = source.create_migration("create_users").unwrap();

        assert!(path.exists());
        assert!(path.join("up.sql").exists());
        assert!(path.join("down.sql").exists());

        // Check the directory name format
        let dir_name = path.file_name().unwrap().to_str().unwrap();
        assert!(dir_name.ends_with("_create_users"));
        assert!(dir_name.len() > 15); // version + underscore + name
    }

    #[test]
    fn test_skip_files_in_root() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file (not a directory) in the migrations folder
        std::fs::write(temp_dir.path().join("readme.txt"), "This is a readme").unwrap();

        // Create a valid migration directory
        let migration_dir = temp_dir.path().join("00001_test");
        std::fs::create_dir_all(&migration_dir).unwrap();
        std::fs::write(migration_dir.join("up.sql"), "SELECT 1").unwrap();

        let source = FileBasedMigrations::new(temp_dir.path());
        let migrations = source.migrations().unwrap();

        assert_eq!(migrations.len(), 1);
    }
}

// =============================================================================
// CombinedMigrations Tests
// =============================================================================

mod combined_source_tests {
    use super::*;

    #[test]
    fn test_empty_combined() {
        let source = CombinedMigrations::new();
        let migrations = source.migrations().unwrap();
        assert!(migrations.is_empty());
    }

    #[test]
    fn test_combine_sources() {
        let source1 = InMemoryMigrations::new()
            .with_migration(Migration::new("1", "first", "", ""));

        let source2 = InMemoryMigrations::new()
            .with_migration(Migration::new("2", "second", "", ""));

        let combined = CombinedMigrations::new()
            .add(source1)
            .add(source2);

        let migrations = combined.migrations().unwrap();
        assert_eq!(migrations.len(), 2);
        assert_eq!(migrations[0].version.as_str(), "1");
        assert_eq!(migrations[1].version.as_str(), "2");
    }

    #[test]
    fn test_combined_sorted() {
        let source1 = InMemoryMigrations::new()
            .with_migration(Migration::new("3", "third", "", ""));

        let source2 = InMemoryMigrations::new()
            .with_migration(Migration::new("1", "first", "", ""));

        let combined = CombinedMigrations::new()
            .add(source1)
            .add(source2);

        let migrations = combined.migrations().unwrap();
        assert_eq!(migrations[0].version.as_str(), "1");
        assert_eq!(migrations[1].version.as_str(), "3");
    }

    #[test]
    fn test_duplicate_version_error() {
        let source1 = InMemoryMigrations::new()
            .with_migration(Migration::new("1", "first", "", ""));

        let source2 = InMemoryMigrations::new()
            .with_migration(Migration::new("1", "duplicate", "", ""));

        let combined = CombinedMigrations::new()
            .add(source1)
            .add(source2);

        let result = combined.migrations();
        assert!(result.is_err());
    }
}

// =============================================================================
// Error Tests
// =============================================================================

mod error_tests {
    use super::*;

    #[test]
    fn test_error_display_directory_not_found() {
        let err = MigrationError::DirectoryNotFound(PathBuf::from("/some/path"));
        let msg = format!("{}", err);
        assert!(msg.contains("/some/path"));
    }

    #[test]
    fn test_migration_exists_error() {
        let err = MigrationError::MigrationExists("12345".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("12345"));
    }

    #[test]
    fn test_sql_error() {
        let err = MigrationError::sql_error("00001_test", "syntax error near FROM");
        let msg = format!("{}", err);
        assert!(msg.contains("00001_test"));
        assert!(msg.contains("syntax error"));
    }

    #[test]
    fn test_database_error() {
        let err = MigrationError::database_error("connection refused");
        let msg = format!("{}", err);
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn test_version_mismatch_error() {
        let err = MigrationError::VersionMismatch {
            expected: "20240115".to_string(),
            found: "20240116".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("20240115"));
        assert!(msg.contains("20240116"));
    }
}
