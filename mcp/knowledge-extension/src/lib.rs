//! Typed knowledge-source declarations and observations. Domain servers own their
//! collections; this crate supplies wire validation and optional MCP adapters.
#[cfg(feature = "mcp")]
pub mod client;
pub mod docs;
mod identity;
pub mod models;
#[cfg(feature = "mcp")]
pub mod server;

pub use identity::{
    CollectionId, CollectionName, DocumentId, EntityKind, ExternalRecordId, ExternalSystemId,
    Revision,
};
pub use models::*;

pub const EXTENSION_ID: &str = "ai.veoveo/knowledge-source";
pub const OBSERVATION_KEY: &str = "ai.veoveo/knowledge-observation";

/// Diagnostics contain field names, never resource contents or identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnowledgeError(pub &'static str);

impl std::fmt::Display for KnowledgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for KnowledgeError {}
