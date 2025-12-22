//! Database connection for the CLI.

use anyhow::Context;
use async_trait::async_trait;

use diesel_clickhouse_migrations::{MigrationConnection, MigrationError, Result as MigrationResult};

/// CLI database connection.
///
/// This is a simple HTTP-based connection for running migrations.
pub struct CliConnection {
    client: reqwest::Client,
    url: String,
    database: String,
}

impl CliConnection {
    /// Connect to the database.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        // Parse URL to extract database name
        let parsed = url::Url::parse(url)
            .with_context(|| format!("Invalid database URL: {}", url))?;

        let database = parsed.path().trim_start_matches('/').to_string();
        let database = if database.is_empty() {
            "default".to_string()
        } else {
            database
        };

        // Reconstruct base URL without path
        let base_url = format!(
            "{}://{}{}",
            parsed.scheme(),
            parsed.host_str().unwrap_or("localhost"),
            parsed.port().map(|p| format!(":{}", p)).unwrap_or_default()
        );

        let client = reqwest::Client::new();

        // Test connection
        let test_query = "SELECT 1";
        let response = client
            .post(&base_url)
            .query(&[("database", &database)])
            .body(test_query)
            .send()
            .await
            .with_context(|| format!("Failed to connect to ClickHouse at {}", url))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to connect to ClickHouse: {}", error);
        }

        Ok(Self {
            client,
            url: base_url,
            database,
        })
    }

    async fn query(&mut self, sql: &str) -> std::result::Result<String, MigrationError> {
        let response = self.client
            .post(&self.url)
            .query(&[("database", &self.database)])
            .body(sql.to_string())
            .send()
            .await
            .map_err(|e| MigrationError::database_error(e.to_string()))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(MigrationError::database_error(error));
        }

        response.text().await
            .map_err(|e| MigrationError::database_error(e.to_string()))
    }
}

#[async_trait]
impl MigrationConnection for CliConnection {
    async fn execute(&mut self, sql: &str) -> MigrationResult<()> {
        self.query(sql).await?;
        Ok(())
    }

    async fn query_exists(&mut self, sql: &str) -> MigrationResult<bool> {
        let result = self.query(sql).await?;
        Ok(!result.trim().is_empty())
    }

    async fn query_scalar_string(&mut self, sql: &str) -> MigrationResult<Option<String>> {
        let result = self.query(sql).await?;
        let trimmed = result.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            // Take first line, first column
            Ok(trimmed.lines().next().map(|s| s.to_string()))
        }
    }

    async fn query_versions(&mut self, sql: &str) -> MigrationResult<Vec<String>> {
        let result = self.query(sql).await?;
        let versions: Vec<String> = result
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                // Take first column (tab-separated)
                line.split('\t').next().unwrap_or(line).to_string()
            })
            .collect();
        Ok(versions)
    }

    fn database_name(&self) -> &str {
        &self.database
    }
}
