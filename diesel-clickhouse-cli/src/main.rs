// Deny unwrap/expect in code to prevent panics
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

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

use diesel_clickhouse::clickhouse;
use diesel_clickhouse::Connection;
use diesel_clickhouse_migrations::{
    FileBasedMigrations, MigrationHarness, MigrationSource, MigrationVersion,
};

mod config;

/// Establish a connection from a URL string.
///
/// Parses the URL and uses the appropriate builder (HTTP or Native).
async fn establish_from_url(url_str: &str) -> Result<Connection> {
    let parsed = url::Url::parse(url_str)
        .map_err(|e| anyhow::anyhow!("Invalid URL: {}", e))?;

    let host = parsed.host_str()
        .ok_or_else(|| anyhow::anyhow!("Missing host in URL"))?;
    let port = parsed.port();
    let database = parsed.path().trim_start_matches('/');
    let database = if database.is_empty() { "default" } else { database };
    let user = parsed.username();
    let password = parsed.password().unwrap_or("");

    if url_str.starts_with("http://") || url_str.starts_with("https://") {
        let mut builder = Connection::http()
            .host(host)
            .database(database);

        if let Some(p) = port {
            builder = builder.port(p);
        } else if url_str.starts_with("https://") {
            builder = builder.port(8443);
        } else {
            builder = builder.port(8123);
        }

        if !user.is_empty() {
            builder = builder.user(user);
        }
        if !password.is_empty() {
            builder = builder.password(password);
        }

        Ok(builder.build().await?)
    } else if url_str.starts_with("tcp://") {
        let mut builder = Connection::native()
            .host(host)
            .database(database)
            .user(if user.is_empty() { "default" } else { user })
            .password(password);

        if let Some(p) = port {
            builder = builder.port(p);
        } else {
            builder = builder.port(9000);
        }

        // Parse query params for native options
        for (key, value) in parsed.query_pairs() {
            match key.as_ref() {
                "secure" if value == "true" => builder = builder.secure(true),
                "compression" if value == "lz4" => {
                    builder = builder.compression(diesel_clickhouse::native::NativeCompression::Lz4)
                }
                _ => {}
            }
        }

        Ok(builder.build().await?)
    } else {
        Err(anyhow::anyhow!(
            "Unknown URL scheme. Use 'http://', 'https://', or 'tcp://'. Got: {}",
            url_str
        ))
    }
}

use config::Config;

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
    /// Print the database schema as table! macros
    PrintSchema {
        /// Only print schema for these tables (comma-separated)
        #[arg(short, long)]
        tables: Option<String>,
        /// Exclude these tables (comma-separated)
        #[arg(short, long)]
        exclude: Option<String>,
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
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
        Commands::PrintSchema { tables, exclude, output } => {
            run_print_schema(&config, tables, exclude, output).await
        }
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

            let mut conn = establish_from_url(&config.database_url).await?;
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

            let mut conn = establish_from_url(&config.database_url).await?;
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

            let mut conn = establish_from_url(&config.database_url).await?;
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

            let mut conn = establish_from_url(&config.database_url).await?;
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

            let mut conn = establish_from_url(&config.database_url).await?;
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

            let mut conn = establish_from_url(&config.database_url).await?;
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

// =============================================================================
// Print Schema
// =============================================================================

async fn run_print_schema(
    config: &Config,
    tables: Option<String>,
    exclude: Option<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    eprintln!("{}", "Generating schema...".cyan());

    let conn = establish_from_url(&config.database_url).await?;

    // Get database name from connection
    let database = conn.database();

    // Parse table filters
    let include_tables: Option<Vec<&str>> = tables.as_ref().map(|t| t.split(',').map(|s| s.trim()).collect());
    let exclude_tables: Vec<&str> = exclude.as_ref().map(|t| t.split(',').map(|s| s.trim()).collect()).unwrap_or_default();

    // Get all tables
    let table_names = get_table_names(&conn, database).await?;

    // Filter tables
    let table_names: Vec<String> = table_names
        .into_iter()
        .filter(|name| {
            // Skip system tables
            if name.starts_with('.') || name.starts_with("__") {
                return false;
            }
            // Apply include filter
            if let Some(ref include) = include_tables {
                if !include.contains(&name.as_str()) {
                    return false;
                }
            }
            // Apply exclude filter
            !exclude_tables.contains(&name.as_str())
        })
        .collect();

    if table_names.is_empty() {
        eprintln!("  {}", "No tables found".yellow());
        return Ok(());
    }

    eprintln!("  Found {} table(s)", table_names.len());

    // Generate schema
    let mut schema = String::new();
    schema.push_str("// @generated automatically by diesel-clickhouse-cli\n");
    schema.push_str("// To regenerate: diesel-clickhouse print-schema\n\n");

    for table_name in &table_names {
        let table_schema = generate_table_schema(&conn, database, table_name).await?;
        schema.push_str(&table_schema);
        schema.push('\n');
    }

    // Output
    if let Some(path) = output {
        std::fs::write(&path, &schema)?;
        eprintln!("  {} Schema written to {}", "✓".green(), path.display());
    } else {
        println!("{}", schema);
    }

    Ok(())
}

async fn get_table_names(conn: &Connection, database: &str) -> Result<Vec<String>> {
    let sql = format!(
        "SELECT name FROM system.tables WHERE database = '{}' ORDER BY name",
        database
    );

    // Use HTTP connection (CLI only supports HTTP)
    let http_conn = conn.as_http().ok_or_else(|| {
        anyhow::anyhow!("print-schema only supports HTTP connections")
    })?;

    let names: Vec<String> = http_conn.client().query(&sql).fetch_all().await?;
    Ok(names)
}

#[derive(Debug)]
struct ColumnInfo {
    name: String,
    type_name: String,
    is_in_primary_key: bool,
    is_in_sorting_key: bool,
}

async fn get_columns(conn: &Connection, database: &str, table: &str) -> Result<Vec<ColumnInfo>> {
    let sql = format!(
        "SELECT name, type, is_in_primary_key, is_in_sorting_key \
         FROM system.columns \
         WHERE database = '{}' AND table = '{}' \
         ORDER BY position",
        database, table
    );

    // Use HTTP connection (CLI only supports HTTP)
    let http_conn = conn.as_http().ok_or_else(|| {
        anyhow::anyhow!("print-schema only supports HTTP connections")
    })?;

    #[derive(clickhouse::Row, serde::Deserialize)]
    struct ColumnRow {
        name: String,
        #[serde(rename = "type")]
        type_name: String,
        is_in_primary_key: u8,
        is_in_sorting_key: u8,
    }

    let rows: Vec<ColumnRow> = http_conn.client().query(&sql).fetch_all().await?;
    let columns = rows
        .into_iter()
        .map(|r| ColumnInfo {
            name: r.name,
            type_name: r.type_name,
            is_in_primary_key: r.is_in_primary_key != 0,
            is_in_sorting_key: r.is_in_sorting_key != 0,
        })
        .collect();

    Ok(columns)
}

async fn generate_table_schema(conn: &Connection, database: &str, table: &str) -> Result<String> {
    let columns = get_columns(conn, database, table).await?;

    if columns.is_empty() {
        return Ok(format!("// Table '{}' has no columns\n", table));
    }

    // Find primary/sorting key columns
    let key_columns: Vec<&str> = columns
        .iter()
        .filter(|c| c.is_in_primary_key || c.is_in_sorting_key)
        .map(|c| c.name.as_str())
        .collect();

    let mut output = String::new();

    // Generate table! macro
    output.push_str("diesel_clickhouse::table! {\n");

    // Table name with primary key columns
    if key_columns.is_empty() {
        output.push_str(&format!("    {} {{\n", table));
    } else {
        output.push_str(&format!("    {} ({}) {{\n", table, key_columns.join(", ")));
    }

    // Columns
    for col in &columns {
        let rust_type = clickhouse_type_to_diesel(&col.type_name);
        output.push_str(&format!("        {} -> {},\n", col.name, rust_type));
    }

    output.push_str("    }\n");
    output.push_str("}\n");

    Ok(output)
}

/// Convert ClickHouse type string to diesel-clickhouse type using the type parser.
fn clickhouse_type_to_diesel(ch_type: &str) -> String {
    use diesel_clickhouse::core::type_parser::parse_type;

    match parse_type(ch_type) {
        Ok(ty) => ty.diesel_type(),
        Err(_) => {
            // Fallback: return the original type as-is
            eprintln!("  {} Unknown type: {}", "!".yellow(), ch_type);
            ch_type.to_string()
        }
    }
}
