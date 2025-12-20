//! CLI tool for diesel-clickhouse migrations.
//!
//! Usage:
//! ```bash
//! # Setup and run migrations
//! diesel-clickhouse migration run
//!
//! # Revert last migration
//! diesel-clickhouse migration revert
//!
//! # Generate a new migration
//! diesel-clickhouse migration generate create_users
//!
//! # List pending migrations
//! diesel-clickhouse migration pending
//!
//! # List all migrations
//! diesel-clickhouse migration list
//! ```

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;

use diesel_clickhouse_migrations::{
    FileBasedMigrations, MigrationHarness, MigrationSource, MigrationVersion,
};

mod config;
mod connection;

use config::Config;
use connection::CliConnection;

/// diesel-clickhouse CLI - Migration tool for ClickHouse
#[derive(Parser)]
#[command(name = "diesel-clickhouse")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long, default_value = "diesel.toml")]
    config: PathBuf,

    /// Database URL (overrides config file)
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,

    /// Migrations directory (overrides config file)
    #[arg(short, long)]
    migrations_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Database management commands
    Database {
        #[command(subcommand)]
        command: DatabaseCommands,
    },
    /// Migration management commands
    Migration {
        #[command(subcommand)]
        command: MigrationCommands,
    },
    /// Setup the project (create diesel.toml and migrations directory)
    Setup,
}

#[derive(Subcommand)]
enum DatabaseCommands {
    /// Create the database
    Create,
    /// Drop the database
    Drop,
    /// Reset the database (drop, create, run migrations)
    Reset,
}

#[derive(Subcommand)]
enum MigrationCommands {
    /// Run pending migrations
    Run {
        /// Run up to a specific version
        #[arg(long)]
        version: Option<String>,
    },
    /// Revert the last N migrations
    Revert {
        /// Number of migrations to revert (default: 1)
        #[arg(short, long, default_value = "1")]
        count: usize,
        /// Revert all migrations
        #[arg(long)]
        all: bool,
    },
    /// Redo (revert and re-run) the last N migrations
    Redo {
        /// Number of migrations to redo (default: 1)
        #[arg(short, long, default_value = "1")]
        count: usize,
    },
    /// Generate a new migration
    Generate {
        /// Name of the migration
        name: String,
    },
    /// List all migrations and their status
    List,
    /// Show pending migrations
    Pending,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if it exists
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    // Load configuration
    let config = Config::load(&cli.config, cli.database_url, cli.migrations_dir)?;

    match cli.command {
        Commands::Setup => run_setup(&config).await,
        Commands::Database { command } => run_database_command(command, &config).await,
        Commands::Migration { command } => run_migration_command(command, &config).await,
    }
}

async fn run_setup(config: &Config) -> Result<()> {
    println!("{}", "Setting up diesel-clickhouse...".cyan());

    // Create migrations directory
    let migrations_dir = &config.migrations_dir;
    if !migrations_dir.exists() {
        std::fs::create_dir_all(migrations_dir)?;
        println!("  {} Created migrations directory: {}", "✓".green(), migrations_dir.display());
    } else {
        println!("  {} Migrations directory already exists", "✓".green());
    }

    // Create diesel.toml if it doesn't exist
    let config_path = PathBuf::from("diesel.toml");
    if !config_path.exists() {
        let default_config = r#"# diesel-clickhouse configuration

[migrations]
# Directory containing migration files
dir = "migrations"

[database]
# Connection URL (can also be set via DATABASE_URL env var)
# url = "http://localhost:8123/default"
"#;
        std::fs::write(&config_path, default_config)?;
        println!("  {} Created diesel.toml", "✓".green());
    } else {
        println!("  {} diesel.toml already exists", "✓".green());
    }

    println!("\n{}", "Setup complete!".green().bold());
    println!("\nNext steps:");
    println!("  1. Set DATABASE_URL environment variable or add it to diesel.toml");
    println!("  2. Run: diesel-clickhouse migration generate create_initial_schema");
    println!("  3. Edit the generated migration files");
    println!("  4. Run: diesel-clickhouse migration run");

    Ok(())
}

async fn run_database_command(command: DatabaseCommands, config: &Config) -> Result<()> {
    match command {
        DatabaseCommands::Create => {
            println!("{}", "Creating database...".cyan());
            // Note: ClickHouse usually requires the database in the connection URL
            // This would need a special connection without the database
            println!("  {} Database creation is typically done via ClickHouse directly", "!".yellow());
            println!("  Use: CREATE DATABASE IF NOT EXISTS your_database");
        }
        DatabaseCommands::Drop => {
            println!("{}", "Dropping database...".cyan());
            println!("  {} Database dropping is typically done via ClickHouse directly", "!".yellow());
            println!("  Use: DROP DATABASE IF EXISTS your_database");
        }
        DatabaseCommands::Reset => {
            println!("{}", "Resetting database...".cyan());

            let mut conn = CliConnection::connect(&config.database_url).await?;
            let source = FileBasedMigrations::new(&config.migrations_dir);

            // Revert all migrations
            let applied = conn.applied_migrations().await?;
            if !applied.is_empty() {
                println!("  Reverting {} migrations...", applied.len());
                let reverted = conn.revert_migrations(&source, applied.len()).await?;
                for v in reverted {
                    println!("    {} Reverted: {}", "↓".yellow(), v);
                }
            }

            // Run all migrations
            let applied = conn.run_pending_migrations(&source).await?;
            for v in &applied {
                println!("    {} Applied: {}", "↑".green(), v);
            }

            println!("\n{}", "Database reset complete!".green().bold());
        }
    }
    Ok(())
}

async fn run_migration_command(command: MigrationCommands, config: &Config) -> Result<()> {
    match command {
        MigrationCommands::Run { version } => {
            println!("{}", "Running migrations...".cyan());

            let mut conn = CliConnection::connect(&config.database_url).await?;
            let source = FileBasedMigrations::new(&config.migrations_dir);

            let applied = if let Some(version) = version {
                let target = MigrationVersion::new(version);
                conn.run_to_version(&source, &target).await?
            } else {
                conn.run_pending_migrations(&source).await?
            };

            if applied.is_empty() {
                println!("  {}", "No pending migrations".yellow());
            } else {
                for v in &applied {
                    println!("  {} Applied: {}", "↑".green(), v);
                }
                println!("\n{} {} migration(s) applied", "✓".green(), applied.len());
            }
        }

        MigrationCommands::Revert { count, all } => {
            println!("{}", "Reverting migrations...".cyan());

            let mut conn = CliConnection::connect(&config.database_url).await?;
            let source = FileBasedMigrations::new(&config.migrations_dir);

            let count = if all {
                conn.applied_migrations().await?.len()
            } else {
                count
            };

            let reverted = conn.revert_migrations(&source, count).await?;

            if reverted.is_empty() {
                println!("  {}", "No migrations to revert".yellow());
            } else {
                for v in &reverted {
                    println!("  {} Reverted: {}", "↓".yellow(), v);
                }
                println!("\n{} {} migration(s) reverted", "✓".green(), reverted.len());
            }
        }

        MigrationCommands::Redo { count } => {
            println!("{}", "Redoing migrations...".cyan());

            let mut conn = CliConnection::connect(&config.database_url).await?;
            let source = FileBasedMigrations::new(&config.migrations_dir);

            let redone = conn.redo_migrations(&source, count).await?;

            if redone.is_empty() {
                println!("  {}", "No migrations to redo".yellow());
            } else {
                for v in &redone {
                    println!("  {} Redone: {}", "↻".blue(), v);
                }
                println!("\n{} {} migration(s) redone", "✓".green(), redone.len());
            }
        }

        MigrationCommands::Generate { name } => {
            println!("{}", "Generating migration...".cyan());

            let source = FileBasedMigrations::new(&config.migrations_dir);

            // Ensure migrations directory exists
            if !config.migrations_dir.exists() {
                std::fs::create_dir_all(&config.migrations_dir)?;
            }

            let path = source.create_migration(&name)?;
            println!("  {} Created migration: {}", "✓".green(), path.display());
            println!("\n  Edit the following files:");
            println!("    - {}/up.sql", path.display());
            println!("    - {}/down.sql", path.display());
        }

        MigrationCommands::List => {
            println!("{}", "Migrations:".cyan());

            let mut conn = CliConnection::connect(&config.database_url).await?;
            let source = FileBasedMigrations::new(&config.migrations_dir);

            let applied = conn.applied_migrations().await?;
            let all = source.migrations()?;

            if all.is_empty() {
                println!("  {}", "No migrations found".yellow());
            } else {
                for migration in &all {
                    let status = if applied.contains(&migration.version) {
                        format!("{}", "[applied]".green())
                    } else {
                        format!("{}", "[pending]".yellow())
                    };
                    println!("  {} {} - {}", status, migration.version, migration.name);
                }
                println!();
                println!("  Total: {} migrations ({} applied, {} pending)",
                    all.len(),
                    applied.len(),
                    all.len() - applied.len()
                );
            }
        }

        MigrationCommands::Pending => {
            println!("{}", "Pending migrations:".cyan());

            let mut conn = CliConnection::connect(&config.database_url).await?;
            let source = FileBasedMigrations::new(&config.migrations_dir);

            let pending = conn.pending_migrations(&source).await?;

            if pending.is_empty() {
                println!("  {}", "No pending migrations".green());
            } else {
                for migration in &pending {
                    println!("  {} {} - {}", "[pending]".yellow(), migration.version, migration.name);
                }
                println!();
                println!("  {} pending migration(s)", pending.len());
            }
        }
    }

    Ok(())
}
