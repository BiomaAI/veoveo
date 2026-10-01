//! Knowledge catalog and index contracts shared by persistence and service adapters.
mod generation;
mod member;
mod title;
pub use generation::{ChunkSettings, GenerationId, GenerationSpec};
pub use member::{IndexedChunk, IndexedMember, metadata_text};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use title::MemberTitle;
use veoveo_mcp_knowledge_extension::CollectionDescriptor;
use veoveo_types::{Sha256Digest, TenantId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnowledgeError(pub &'static str);
impl std::fmt::Display for KnowledgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for KnowledgeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CollectionApproval {
    CatalogOnly,
    Index,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionRegistration {
    pub tenant: TenantId,
    pub descriptor: CollectionDescriptor,
    pub approval: CollectionApproval,
    pub control_revision: Sha256Digest,
}
impl CollectionRegistration {
    /// Binds source declaration and control-plane approval, including descriptor changes.
    pub fn revision(&self) -> Sha256Digest {
        digest(self)
    }
}
pub(crate) fn digest(value: &impl Serialize) -> Sha256Digest {
    Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(value).expect("closed contract serialization")).into(),
    )
}
