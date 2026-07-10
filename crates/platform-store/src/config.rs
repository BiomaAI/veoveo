use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::error::StoreConfigError;

const DEFAULT_QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_TRANSACTION_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONNECTION_CAPACITY: usize = 4_096;
const DEFAULT_WS_BUFFER: usize = 128 * 1024;
const DEFAULT_WS_MAX_WRITE_BUFFER: usize = 2 * 1024 * 1024;
const DEFAULT_WS_MAX_MESSAGE: usize = 64 * 1024 * 1024;

/// Validated configuration for the remote WebSocket store.
#[derive(Clone)]
pub struct StoreConfig {
    endpoint: Url,
    namespace: String,
    database: String,
    credentials: StoreCredentials,
    query_timeout: Duration,
    transaction_timeout: Duration,
    connection_capacity: usize,
    websocket_read_buffer: usize,
    websocket_write_buffer: usize,
    websocket_max_write_buffer: usize,
    websocket_max_message: usize,
    migrate_on_connect: bool,
}

impl StoreConfig {
    pub fn builder(
        endpoint: impl AsRef<str>,
        namespace: impl Into<String>,
        database: impl Into<String>,
        credentials: StoreCredentials,
    ) -> StoreConfigBuilder {
        StoreConfigBuilder {
            endpoint: endpoint.as_ref().to_owned(),
            namespace: namespace.into(),
            database: database.into(),
            credentials,
            query_timeout: DEFAULT_QUERY_TIMEOUT,
            transaction_timeout: DEFAULT_TRANSACTION_TIMEOUT,
            connection_capacity: DEFAULT_CONNECTION_CAPACITY,
            websocket_read_buffer: DEFAULT_WS_BUFFER,
            websocket_write_buffer: DEFAULT_WS_BUFFER,
            websocket_max_write_buffer: DEFAULT_WS_MAX_WRITE_BUFFER,
            websocket_max_message: DEFAULT_WS_MAX_MESSAGE,
            migrate_on_connect: false,
        }
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn database(&self) -> &str {
        &self.database
    }

    pub fn username(&self) -> &str {
        self.credentials.username()
    }

    pub fn auth_level(&self) -> StoreAuthLevel {
        self.credentials.auth_level()
    }

    pub(crate) fn credentials(&self) -> &StoreCredentials {
        &self.credentials
    }

    pub fn query_timeout(&self) -> Duration {
        self.query_timeout
    }

    pub fn transaction_timeout(&self) -> Duration {
        self.transaction_timeout
    }

    pub fn connection_capacity(&self) -> usize {
        self.connection_capacity
    }

    pub fn websocket_read_buffer(&self) -> usize {
        self.websocket_read_buffer
    }

    pub fn websocket_write_buffer(&self) -> usize {
        self.websocket_write_buffer
    }

    pub fn websocket_max_write_buffer(&self) -> usize {
        self.websocket_max_write_buffer
    }

    pub fn websocket_max_message(&self) -> usize {
        self.websocket_max_message
    }

    pub fn migrate_on_connect(&self) -> bool {
        self.migrate_on_connect
    }
}

impl fmt::Debug for StoreConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoreConfig")
            .field("endpoint", &self.endpoint)
            .field("namespace", &self.namespace)
            .field("database", &self.database)
            .field("credentials", &self.credentials)
            .field("query_timeout", &self.query_timeout)
            .field("transaction_timeout", &self.transaction_timeout)
            .field("connection_capacity", &self.connection_capacity)
            .field("websocket_read_buffer", &self.websocket_read_buffer)
            .field("websocket_write_buffer", &self.websocket_write_buffer)
            .field(
                "websocket_max_write_buffer",
                &self.websocket_max_write_buffer,
            )
            .field("websocket_max_message", &self.websocket_max_message)
            .field("migrate_on_connect", &self.migrate_on_connect)
            .finish()
    }
}

pub struct StoreConfigBuilder {
    endpoint: String,
    namespace: String,
    database: String,
    credentials: StoreCredentials,
    query_timeout: Duration,
    transaction_timeout: Duration,
    connection_capacity: usize,
    websocket_read_buffer: usize,
    websocket_write_buffer: usize,
    websocket_max_write_buffer: usize,
    websocket_max_message: usize,
    migrate_on_connect: bool,
}

impl StoreConfigBuilder {
    pub fn query_timeout(mut self, value: Duration) -> Self {
        self.query_timeout = value;
        self
    }

    pub fn transaction_timeout(mut self, value: Duration) -> Self {
        self.transaction_timeout = value;
        self
    }

    pub fn connection_capacity(mut self, value: usize) -> Self {
        self.connection_capacity = value;
        self
    }

    pub fn websocket_buffers(mut self, read: usize, write: usize, max_write: usize) -> Self {
        self.websocket_read_buffer = read;
        self.websocket_write_buffer = write;
        self.websocket_max_write_buffer = max_write;
        self
    }

    pub fn websocket_max_message(mut self, value: usize) -> Self {
        self.websocket_max_message = value;
        self
    }

    pub fn migrate_on_connect(mut self, value: bool) -> Self {
        self.migrate_on_connect = value;
        self
    }

    pub fn build(self) -> Result<StoreConfig, StoreConfigError> {
        let endpoint = Url::parse(&self.endpoint)
            .map_err(|error| StoreConfigError::InvalidEndpoint(error.to_string()))?;
        if !matches!(endpoint.scheme(), "ws" | "wss") {
            return Err(StoreConfigError::UnsupportedEndpointScheme(
                endpoint.scheme().to_owned(),
            ));
        }
        if endpoint.host_str().is_none() {
            return Err(StoreConfigError::MissingEndpointHost);
        }
        if !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(StoreConfigError::UnsafeEndpoint);
        }
        validate_name("namespace", &self.namespace)?;
        validate_name("database", &self.database)?;
        if self.credentials.username().trim().is_empty() {
            return Err(StoreConfigError::EmptyUsername);
        }
        if self.credentials.password().is_empty() {
            return Err(StoreConfigError::EmptyPassword);
        }
        validate_name("username", self.credentials.username())?;
        for (field, value) in [
            ("query_timeout", self.query_timeout.as_nanos()),
            ("transaction_timeout", self.transaction_timeout.as_nanos()),
            ("connection_capacity", self.connection_capacity as u128),
            ("websocket_read_buffer", self.websocket_read_buffer as u128),
            (
                "websocket_write_buffer",
                self.websocket_write_buffer as u128,
            ),
            (
                "websocket_max_write_buffer",
                self.websocket_max_write_buffer as u128,
            ),
            ("websocket_max_message", self.websocket_max_message as u128),
        ] {
            if value == 0 {
                return Err(StoreConfigError::ZeroValue { field });
            }
        }
        if self.websocket_max_write_buffer <= self.websocket_write_buffer {
            return Err(StoreConfigError::InvalidWriteBuffer);
        }
        if self.migrate_on_connect && self.credentials.auth_level() != StoreAuthLevel::Root {
            return Err(StoreConfigError::MigrationRequiresRootCredentials);
        }

        Ok(StoreConfig {
            endpoint,
            namespace: self.namespace,
            database: self.database,
            credentials: self.credentials,
            query_timeout: self.query_timeout,
            transaction_timeout: self.transaction_timeout,
            connection_capacity: self.connection_capacity,
            websocket_read_buffer: self.websocket_read_buffer,
            websocket_write_buffer: self.websocket_write_buffer,
            websocket_max_write_buffer: self.websocket_max_write_buffer,
            websocket_max_message: self.websocket_max_message,
            migrate_on_connect: self.migrate_on_connect,
        })
    }
}

/// Authentication scope used for the SurrealDB connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreAuthLevel {
    Root,
    Namespace,
    Database,
}

impl StoreAuthLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Namespace => "namespace",
            Self::Database => "database",
        }
    }
}

impl FromStr for StoreAuthLevel {
    type Err = StoreConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "root" => Ok(Self::Root),
            "namespace" => Ok(Self::Namespace),
            "database" => Ok(Self::Database),
            other => Err(StoreConfigError::InvalidAuthLevel(other.to_owned())),
        }
    }
}

/// Secret-bearing credentials with an explicit SurrealDB authentication scope.
#[derive(Clone)]
pub enum StoreCredentials {
    Root {
        username: String,
        password: SecretString,
    },
    Namespace {
        username: String,
        password: SecretString,
    },
    Database {
        username: String,
        password: SecretString,
    },
}

impl StoreCredentials {
    pub fn new(
        auth_level: StoreAuthLevel,
        username: impl Into<String>,
        password: impl Into<SecretString>,
    ) -> Self {
        let username = username.into();
        let password = password.into();
        match auth_level {
            StoreAuthLevel::Root => Self::Root { username, password },
            StoreAuthLevel::Namespace => Self::Namespace { username, password },
            StoreAuthLevel::Database => Self::Database { username, password },
        }
    }

    pub fn root(username: impl Into<String>, password: impl Into<SecretString>) -> Self {
        Self::new(StoreAuthLevel::Root, username, password)
    }

    pub fn namespace(username: impl Into<String>, password: impl Into<SecretString>) -> Self {
        Self::new(StoreAuthLevel::Namespace, username, password)
    }

    pub fn database(username: impl Into<String>, password: impl Into<SecretString>) -> Self {
        Self::new(StoreAuthLevel::Database, username, password)
    }

    pub const fn auth_level(&self) -> StoreAuthLevel {
        match self {
            Self::Root { .. } => StoreAuthLevel::Root,
            Self::Namespace { .. } => StoreAuthLevel::Namespace,
            Self::Database { .. } => StoreAuthLevel::Database,
        }
    }

    pub fn username(&self) -> &str {
        match self {
            Self::Root { username, .. }
            | Self::Namespace { username, .. }
            | Self::Database { username, .. } => username,
        }
    }

    pub(crate) fn password(&self) -> &str {
        match self {
            Self::Root { password, .. }
            | Self::Namespace { password, .. }
            | Self::Database { password, .. } => password.expose_secret(),
        }
    }
}

impl fmt::Debug for StoreCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoreCredentials")
            .field("auth_level", &self.auth_level().as_str())
            .field("username", &self.username())
            .field("password", &"[REDACTED]")
            .finish()
    }
}

pub(crate) fn validate_name(field: &'static str, value: &str) -> Result<(), StoreConfigError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(StoreConfigError::InvalidName { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_redacts_configuration() {
        let config = StoreConfig::builder(
            "wss://db.internal.example/rpc",
            "veoveo",
            "platform",
            StoreCredentials::root("root", "not-for-logs"),
        )
        .build()
        .unwrap();

        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("not-for-logs"));
    }

    #[test]
    fn rejects_ambient_or_malformed_endpoints() {
        for endpoint in [
            "https://db.internal.example",
            "ws://user:pass@db.internal.example",
            "ws://db.internal.example?token=secret",
        ] {
            assert!(
                StoreConfig::builder(
                    endpoint,
                    "veoveo",
                    "platform",
                    StoreCredentials::database("runtime", "secret"),
                )
                .build()
                .is_err(),
                "accepted {endpoint}"
            );
        }
    }

    #[test]
    fn rejects_invalid_resource_names_and_limits() {
        let error = StoreConfig::builder(
            "ws://127.0.0.1:8000",
            "not a namespace",
            "platform",
            StoreCredentials::root("root", "secret"),
        )
        .build()
        .unwrap_err();
        assert_eq!(error, StoreConfigError::InvalidName { field: "namespace" });

        let error = StoreConfig::builder(
            "ws://127.0.0.1:8000",
            "veoveo",
            "platform",
            StoreCredentials::root("root", "secret"),
        )
        .websocket_buffers(1, 1024, 1024)
        .build()
        .unwrap_err();
        assert_eq!(error, StoreConfigError::InvalidWriteBuffer);
    }

    #[test]
    fn migration_is_explicit_and_root_only() {
        let runtime = StoreConfig::builder(
            "ws://127.0.0.1:8000",
            "veoveo",
            "platform",
            StoreCredentials::database("runtime", "secret"),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap_err();
        assert_eq!(runtime, StoreConfigError::MigrationRequiresRootCredentials);

        let root = StoreConfig::builder(
            "ws://127.0.0.1:8000",
            "veoveo",
            "platform",
            StoreCredentials::root("root", "secret"),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap();
        assert_eq!(root.auth_level(), StoreAuthLevel::Root);
    }

    #[test]
    fn authentication_level_is_canonical() {
        assert_eq!("root".parse(), Ok(StoreAuthLevel::Root));
        assert_eq!("namespace".parse(), Ok(StoreAuthLevel::Namespace));
        assert_eq!("database".parse(), Ok(StoreAuthLevel::Database));
        assert_eq!(
            "db".parse::<StoreAuthLevel>(),
            Err(StoreConfigError::InvalidAuthLevel("db".to_owned()))
        );
    }
}
