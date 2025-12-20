//! Configuration management for the CLI.

use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use serde::Deserialize;

/// CLI configuration.
#[derive(Debug)]
pub struct Config {
    /// Database connection URL.
    pub database_url: String,
    /// Migrations directory.
    pub migrations_dir: PathBuf,
}

/// Configuration file format.
#[derive(Debug, Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    database: DatabaseConfig,
    #[serde(default)]
    migrations: MigrationsConfig,
}

#[derive(Debug, Deserialize, Default)]
struct DatabaseConfig {
    url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct MigrationsConfig {
    dir: Option<String>,
}

impl Config {
    /// Load configuration from file and CLI arguments.
    pub fn load(
        config_path: &Path,
        cli_database_url: Option<String>,
        cli_migrations_dir: Option<PathBuf>,
    ) -> Result<Self> {
        // Try to load config file
        let file_config = if config_path.exists() {
            let content = std::fs::read_to_string(config_path)
                .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
            toml::from_str::<ConfigFile>(&content)
                .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?
        } else {
            ConfigFile::default()
        };

        // Resolve database URL (CLI > env > config file)
        let database_url = cli_database_url
            .or_else(|| std::env::var("DATABASE_URL").ok())
            .or(file_config.database.url)
            .ok_or_else(|| anyhow::anyhow!(
                "Database URL not set. Set DATABASE_URL environment variable or add it to diesel.toml"
            ))?;

        // Resolve migrations directory (CLI > config file > default)
        let migrations_dir = cli_migrations_dir
            .or_else(|| file_config.migrations.dir.map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("migrations"));

        Ok(Self {
            database_url,
            migrations_dir,
        })
    }
}
