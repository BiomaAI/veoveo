//! Knowledge catalog and index contracts shared by persistence and service adapters.
mod approval;
pub use approval::{KnowledgeCollectionApproval, KnowledgeIndexingRegistration, KnowledgeSubject};
mod evaluation;
mod generation;
mod member;
mod statistics;
mod title;
pub use evaluation::{
    EvaluationBuilder, EvaluationCaseId, EvaluationMember, EvaluationMemberId, RecallCounts,
    RetrievalCase, RetrievalDataset, RetrievalEvaluation, RetrievalMeasurement,
};
pub use generation::{ChunkSettings, GenerationId, GenerationSpec};
pub use member::{IndexedChunk, IndexedMember, metadata_text};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use statistics::CollectionStatistics;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum CollectionApproval {
    #[vocabulary(rename = "catalog_only")]
    CatalogOnly,
    #[vocabulary(rename = "index")]
    Index,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionRegistration {
    pub tenant: TenantId,
    pub descriptor: CollectionDescriptor,
    pub source_contract_revision: u32,
    pub approval: KnowledgeCollectionApproval,
    pub control_revision: Sha256Digest,
}
impl CollectionRegistration {
    pub fn validate(&self) -> Result<(), KnowledgeError> {
        self.approval.validate()?;
        if self.source_contract_revision == 0 {
            return Err(KnowledgeError("source contract revision must be positive"));
        }
        if &self.approval.collection != self.descriptor.collection() {
            return Err(KnowledgeError("approval belongs to another collection"));
        }
        if self.approval.mode == CollectionApproval::Index
            && self.descriptor.indexing() == veoveo_mcp_knowledge_extension::IndexingMode::None
        {
            return Err(KnowledgeError(
                "a non-indexable collection cannot be approved for indexing",
            ));
        }
        Ok(())
    }
    /// Apply the installation's retained-data ceiling before sending source text
    /// to embeddings and again when accepting source-bound chunks for storage.
    pub fn admit_observation(
        &self,
        observation: &veoveo_mcp_knowledge_extension::Observation,
    ) -> Result<(), KnowledgeError> {
        self.validate()?;
        observation
            .validate_collection(&self.descriptor)
            .map_err(|_| KnowledgeError("observation belongs to another collection"))?;
        if let Some(access) = observation.access()
            && (access.tenant != self.tenant
                || !access
                    .data_labels
                    .iter()
                    .all(|label| self.approval.data_labels.contains(label)))
        {
            return Err(KnowledgeError(
                "source observation exceeds the installation's tenant or data-label approval",
            ));
        }
        Ok(())
    }

    /// Binds source declaration and control-plane approval, including descriptor changes.
    /// Publication authority is checked separately against `control_revision`;
    /// unrelated installation edits do not invalidate cached source content.
    pub fn revision(&self) -> Sha256Digest {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Fingerprint<'a> {
            tenant: &'a TenantId,
            descriptor: &'a CollectionDescriptor,
            source_contract_revision: u32,
            approval: &'a KnowledgeCollectionApproval,
        }
        digest(&Fingerprint {
            tenant: &self.tenant,
            descriptor: &self.descriptor,
            source_contract_revision: self.source_contract_revision,
            approval: &self.approval,
        })
    }
}
pub(crate) fn digest(value: &impl Serialize) -> Sha256Digest {
    Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(value).expect("closed contract serialization")).into(),
    )
}
