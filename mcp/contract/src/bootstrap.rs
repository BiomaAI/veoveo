//! Installation-time server bootstrap: the deployment contract by which an
//! MCP server component receives its initial domain configuration.
//!
//! The core (chart, gateway, platform) only ever handles this envelope; the
//! payload schema is owned by the consuming server crate, keeping domain
//! knowledge inside the component. Delivery is a mounted file — bootstrap is
//! install-time, air-gapped-friendly, and pre-auth, mirroring how the
//! gateway control plane is seeded from file — never an MCP wire method.
//!
//! Semantics every consuming server must uphold:
//! - apply idempotently at startup (documented as create-only or reconcile);
//! - fail closed: refuse to start on an undecodable or mistargeted document;
//! - reject unknown payload fields via its typed payload schema;
//! - keep payloads secret-free (use secret references, never values);
//! - expose the canonical `bootstrap-validate <path>` CLI verb so documents
//!   can be validated without booting the server.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::gateway::ServerSlug;

/// Canonical container path where the deployment mounts the document.
pub const SERVER_BOOTSTRAP_MOUNT_PATH: &str = "/etc/veoveo/bootstrap/catalog.json";
/// Canonical serve-time flag consuming servers accept.
pub const SERVER_BOOTSTRAP_FLAG: &str = "--bootstrap-catalog";
/// Canonical CLI verb for validating a document without booting.
pub const SERVER_BOOTSTRAP_VALIDATE_COMMAND: &str = "bootstrap-validate";
/// Issuer recorded on the identity that applies bootstrap writes.
pub const SERVER_BOOTSTRAP_ISSUER: &str = "urn:veoveo:installation-bootstrap";

/// Principal key recorded on the identity that applies bootstrap writes.
pub fn server_bootstrap_principal(server: &ServerSlug) -> String {
    format!("{server}-catalog-bootstrap")
}

/// The generic bootstrap envelope. `payload` is opaque to the core; the
/// consuming server decodes it against its own `deny_unknown_fields` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServerBootstrapDocument {
    pub server: ServerSlug,
    pub tenant_key: String,
    pub payload: serde_json::Value,
}

impl ServerBootstrapDocument {
    /// Decodes an envelope and enforces that it targets `server` with a
    /// usable tenant key. Servers call this before touching the payload.
    pub fn decode_for(server: &ServerSlug, bytes: &[u8]) -> Result<Self, ServerBootstrapError> {
        let document: Self =
            serde_json::from_slice(bytes).map_err(ServerBootstrapError::Envelope)?;
        if &document.server != server {
            return Err(ServerBootstrapError::ServerMismatch {
                document: document.server,
                server: server.clone(),
            });
        }
        if document.tenant_key.trim().is_empty() {
            return Err(ServerBootstrapError::EmptyTenantKey);
        }
        Ok(document)
    }

    /// Decodes the server-owned payload into the consuming server's typed
    /// schema.
    pub fn payload<T: DeserializeOwned>(&self) -> Result<T, ServerBootstrapError> {
        serde_json::from_value(self.payload.clone()).map_err(ServerBootstrapError::Payload)
    }
}

#[derive(Debug)]
pub enum ServerBootstrapError {
    Envelope(serde_json::Error),
    Payload(serde_json::Error),
    ServerMismatch {
        document: ServerSlug,
        server: ServerSlug,
    },
    EmptyTenantKey,
}

impl fmt::Display for ServerBootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Envelope(error) => {
                write!(
                    formatter,
                    "bootstrap envelope could not be decoded: {error}"
                )
            }
            Self::Payload(error) => {
                write!(formatter, "bootstrap payload could not be decoded: {error}")
            }
            Self::ServerMismatch { document, server } => write!(
                formatter,
                "bootstrap document targets server `{document}` but `{server}` is consuming it"
            ),
            Self::EmptyTenantKey => {
                write!(formatter, "bootstrap tenant_key must not be empty")
            }
        }
    }
}

impl std::error::Error for ServerBootstrapError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn slug(value: &str) -> ServerSlug {
        ServerSlug::new(value).expect("valid slug")
    }

    #[test]
    fn decode_enforces_the_target_server() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "server": "map",
            "tenant_key": "installation",
            "payload": {"sources": []},
        }))
        .expect("document serializes");
        let document =
            ServerBootstrapDocument::decode_for(&slug("map"), &bytes).expect("map decodes");
        assert_eq!(document.tenant_key, "installation");
        assert!(matches!(
            ServerBootstrapDocument::decode_for(&slug("time"), &bytes),
            Err(ServerBootstrapError::ServerMismatch { .. })
        ));
    }

    #[test]
    fn decode_rejects_blank_tenants_and_unknown_envelope_fields() {
        let blank = serde_json::to_vec(&serde_json::json!({
            "server": "map",
            "tenant_key": "  ",
            "payload": {},
        }))
        .expect("document serializes");
        assert!(matches!(
            ServerBootstrapDocument::decode_for(&slug("map"), &blank),
            Err(ServerBootstrapError::EmptyTenantKey)
        ));
        let unknown = serde_json::to_vec(&serde_json::json!({
            "server": "map",
            "tenant_key": "installation",
            "payload": {},
            "sources": [],
        }))
        .expect("document serializes");
        assert!(matches!(
            ServerBootstrapDocument::decode_for(&slug("map"), &unknown),
            Err(ServerBootstrapError::Envelope(_))
        ));
    }

    #[test]
    fn typed_payload_decoding_is_the_server_boundary() {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Payload {
            #[serde(default)]
            names: Vec<String>,
        }
        let document = ServerBootstrapDocument {
            server: slug("map"),
            tenant_key: "installation".to_owned(),
            payload: serde_json::json!({"names": ["a"]}),
        };
        let payload: Payload = document.payload().expect("payload decodes");
        assert_eq!(payload.names, ["a"]);
        let stray = ServerBootstrapDocument {
            payload: serde_json::json!({"legacy": true}),
            ..document
        };
        assert!(matches!(
            stray.payload::<Payload>(),
            Err(ServerBootstrapError::Payload(_))
        ));
    }

    #[test]
    fn bootstrap_principals_are_server_scoped() {
        assert_eq!(
            server_bootstrap_principal(&slug("map")),
            "map-catalog-bootstrap"
        );
    }
}
