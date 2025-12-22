//! Migration types and traits.

use std::cmp::Ordering;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A unique version identifier for a migration.
///
/// Versions are typically timestamps in the format YYYYMMDDHHMMSS,
/// but can also be sequential numbers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MigrationVersion(String);

impl MigrationVersion {
    /// Create a new migration version.
    pub fn new(version: impl Into<String>) -> Self {
        Self(version.into())
    }

    /// Get the version string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Generate a new version based on current timestamp.
    pub fn generate() -> Self {
        let timestamp = Utc::now().format("%Y%m%d%H%M%S").to_string();
        Self(timestamp)
    }

    /// Parse version from a migration directory name.
    ///
    /// Supports formats:
    /// - `YYYYMMDDHHMMSS_name` (timestamp-based)
    /// - `NNNNN_name` (sequential)
    pub fn from_directory_name(name: &str) -> Option<Self> {
        let parts: Vec<&str> = name.splitn(2, '_').collect();
        if !parts.is_empty() {
            let version = parts[0];
            // Validate it's numeric
            if version.chars().all(|c| c.is_ascii_digit()) {
                return Some(Self(version.to_string()));
            }
        }
        None
    }
}

impl PartialOrd for MigrationVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MigrationVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare numerically if both are numbers, otherwise lexicographically
        match (self.0.parse::<u64>(), other.0.parse::<u64>()) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            _ => self.0.cmp(&other.0),
        }
    }
}

impl std::fmt::Display for MigrationVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for MigrationVersion {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// The name/description of a migration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MigrationName(String);

impl MigrationName {
    /// Create a new migration name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the name string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse name from a migration directory name.
    pub fn from_directory_name(name: &str) -> Option<Self> {
        let parts: Vec<&str> = name.splitn(2, '_').collect();
        if parts.len() == 2 {
            Some(Self(parts[1].to_string()))
        } else {
            None
        }
    }
}

impl std::fmt::Display for MigrationName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for MigrationName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Metadata about a migration stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationMetadata {
    /// The version of this migration.
    pub version: MigrationVersion,
    /// When this migration was run.
    pub run_on: DateTime<Utc>,
    /// Checksum of the migration SQL (for verification).
    pub checksum: Option<String>,
}

impl MigrationMetadata {
    /// Create new migration metadata.
    pub fn new(version: MigrationVersion) -> Self {
        Self {
            version,
            run_on: Utc::now(),
            checksum: None,
        }
    }

    /// Create metadata with a checksum.
    pub fn with_checksum(version: MigrationVersion, checksum: String) -> Self {
        Self {
            version,
            run_on: Utc::now(),
            checksum: Some(checksum),
        }
    }
}

/// A single migration with up and down SQL.
#[derive(Debug, Clone)]
pub struct Migration {
    /// The version identifier.
    pub version: MigrationVersion,
    /// The descriptive name.
    pub name: MigrationName,
    /// SQL to apply the migration.
    pub up_sql: String,
    /// SQL to revert the migration.
    pub down_sql: String,
}

impl Migration {
    /// Create a new migration.
    pub fn new(
        version: impl Into<String>,
        name: impl Into<String>,
        up_sql: impl Into<String>,
        down_sql: impl Into<String>,
    ) -> Self {
        Self {
            version: MigrationVersion::new(version),
            name: MigrationName::new(name),
            up_sql: up_sql.into(),
            down_sql: down_sql.into(),
        }
    }

    /// Get the full directory name for this migration.
    pub fn directory_name(&self) -> String {
        format!("{}_{}", self.version, self.name.0.replace(' ', "_"))
    }

    /// Calculate a checksum of the up SQL.
    pub fn checksum(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.up_sql.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Parse a migration from directory name and SQL contents.
    pub fn from_parts(
        dir_name: &str,
        up_sql: impl Into<String>,
        down_sql: impl Into<String>,
    ) -> Option<Self> {
        let version = MigrationVersion::from_directory_name(dir_name)?;
        let name = MigrationName::from_directory_name(dir_name)?;

        Some(Self {
            version,
            name,
            up_sql: up_sql.into(),
            down_sql: down_sql.into(),
        })
    }
}

impl PartialEq for Migration {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
    }
}

impl Eq for Migration {}

impl PartialOrd for Migration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Migration {
    fn cmp(&self, other: &Self) -> Ordering {
        self.version.cmp(&other.version)
    }
}

/// Builder for creating migrations programmatically.
pub struct MigrationBuilder {
    version: Option<MigrationVersion>,
    name: Option<MigrationName>,
    up_sql: Option<String>,
    down_sql: Option<String>,
}

impl MigrationBuilder {
    /// Create a new migration builder.
    pub fn new() -> Self {
        Self {
            version: None,
            name: None,
            up_sql: None,
            down_sql: None,
        }
    }

    /// Set the version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(MigrationVersion::new(version));
        self
    }

    /// Set the name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(MigrationName::new(name));
        self
    }

    /// Set the up SQL.
    pub fn up(mut self, sql: impl Into<String>) -> Self {
        self.up_sql = Some(sql.into());
        self
    }

    /// Set the down SQL.
    pub fn down(mut self, sql: impl Into<String>) -> Self {
        self.down_sql = Some(sql.into());
        self
    }

    /// Build the migration.
    pub fn build(self) -> Option<Migration> {
        Some(Migration {
            version: self.version?,
            name: self.name?,
            up_sql: self.up_sql?,
            down_sql: self.down_sql?,
        })
    }
}

impl Default for MigrationBuilder {
    fn default() -> Self {
        Self::new()
    }
}
