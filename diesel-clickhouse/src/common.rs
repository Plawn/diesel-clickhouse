//! Common utilities shared between HTTP and Native backends.
//!
//! This module contains shared types and functionality used by both
//! connection backends to reduce code duplication.

use std::borrow::Cow;

use crate::core::result::{Error, QueryResult};

// =============================================================================
// Connection Parameters
// =============================================================================

/// Common connection parameters shared between HTTP and Native backends.
///
/// This struct contains the required connection parameters that are identical
/// for both protocols. Backend-specific options (like TLS, timeouts, compression)
/// are handled separately by each builder.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::common::ConnectionParams;
///
/// let params = ConnectionParams::new()
///     .host("localhost")
///     .port(8123)
///     .database("default")
///     .user("default")
///     .password("");
///
/// let validated = params.validate()?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct ConnectionParams {
    /// ClickHouse server hostname or IP address.
    pub host: Option<String>,
    /// ClickHouse server port.
    pub port: Option<u16>,
    /// Database name to connect to.
    pub database: Option<String>,
    /// Username for authentication.
    pub user: Option<String>,
    /// Password for authentication.
    pub password: Option<String>,
}

/// Validated connection parameters.
///
/// This struct is returned by `ConnectionParams::validate()` and guarantees
/// that all required fields are present.
#[derive(Debug, Clone)]
pub struct ValidatedConnectionParams {
    /// ClickHouse server hostname or IP address.
    pub host: String,
    /// ClickHouse server port.
    pub port: u16,
    /// Database name to connect to.
    pub database: String,
    /// Username for authentication.
    pub user: String,
    /// Password for authentication.
    pub password: String,
}

impl ConnectionParams {
    /// Create a new empty connection parameters builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the host (required).
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Set the port (required).
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the database (required).
    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    /// Set the user (required).
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Set the password (required).
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Validate that all required fields are present.
    ///
    /// # Errors
    ///
    /// Returns an error if any required field is missing.
    pub fn validate(self) -> QueryResult<ValidatedConnectionParams> {
        let host = self.host.ok_or_else(|| {
            Error::ConnectionError(Cow::Borrowed("host is required"))
        })?;
        let port = self.port.ok_or_else(|| {
            Error::ConnectionError(Cow::Borrowed("port is required"))
        })?;
        let database = self.database.ok_or_else(|| {
            Error::ConnectionError(Cow::Borrowed("database is required"))
        })?;
        let user = self.user.ok_or_else(|| {
            Error::ConnectionError(Cow::Borrowed("user is required"))
        })?;
        let password = self.password.ok_or_else(|| {
            Error::ConnectionError(Cow::Borrowed("password is required"))
        })?;

        Ok(ValidatedConnectionParams {
            host,
            port,
            database,
            user,
            password,
        })
    }
}

impl ValidatedConnectionParams {
    /// Get the server address as "host:port".
    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// =============================================================================
// Connection Builder Trait
// =============================================================================

/// Trait for connection builders with common parameters.
///
/// This trait provides a unified interface for setting connection parameters
/// on both HTTP and Native builders.
pub trait ConnectionBuilder: Sized {
    /// Get a mutable reference to the connection parameters.
    fn params_mut(&mut self) -> &mut ConnectionParams;

    /// Set the host (required).
    fn host(mut self, host: impl Into<String>) -> Self {
        self.params_mut().host = Some(host.into());
        self
    }

    /// Set the port (required).
    fn port(mut self, port: u16) -> Self {
        self.params_mut().port = Some(port);
        self
    }

    /// Set the database (required).
    fn database(mut self, database: impl Into<String>) -> Self {
        self.params_mut().database = Some(database.into());
        self
    }

    /// Set the user (required).
    fn user(mut self, user: impl Into<String>) -> Self {
        self.params_mut().user = Some(user.into());
        self
    }

    /// Set the password (required).
    fn password(mut self, password: impl Into<String>) -> Self {
        self.params_mut().password = Some(password.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_params_validation() {
        let params = ConnectionParams::new()
            .host("localhost")
            .port(8123)
            .database("default")
            .user("default")
            .password("secret");

        let validated = params.validate().unwrap();
        assert_eq!(validated.host, "localhost");
        assert_eq!(validated.port, 8123);
        assert_eq!(validated.database, "default");
        assert_eq!(validated.user, "default");
        assert_eq!(validated.password, "secret");
        assert_eq!(validated.server_addr(), "localhost:8123");
    }

    #[test]
    fn test_connection_params_missing_host() {
        let params = ConnectionParams::new()
            .port(8123)
            .database("default")
            .user("default")
            .password("");

        let result = params.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_connection_params_missing_port() {
        let params = ConnectionParams::new()
            .host("localhost")
            .database("default")
            .user("default")
            .password("");

        let result = params.validate();
        assert!(result.is_err());
    }
}
