//! Reviewed observation fields. External navigation URLs can carry signed query
//! credentials, so audit records retain the external identity without its URL.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_knowledge_extension::{
    AccessDescriptor, CollectionId, ExternalRecordId, ExternalSystemId, ModifiedBy, Observation,
    Revision,
};
use veoveo_types::Sha256Digest;

mod frozen_access;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeExternalIdentity {
    pub system: ExternalSystemId,
    pub native_id: ExternalRecordId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirrored_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeReadObservation {
    pub collection: CollectionId,
    pub revision: Revision,
    pub content_sha256: Sha256Digest,
    pub observed_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_by: Option<ModifiedBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, with = "frozen_access")]
    #[schemars(with = "Option<frozen_access::FrozenAccess>")]
    pub access: Option<AccessDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<KnowledgeExternalIdentity>,
    pub not_modified: bool,
}

impl From<&Observation> for KnowledgeReadObservation {
    fn from(observation: &Observation) -> Self {
        Self {
            collection: observation.collection().clone(),
            revision: observation.revision().clone(),
            content_sha256: observation.content_sha256().clone(),
            observed_at: observation.observed_at(),
            modified_at: observation.modified_at(),
            modified_by: observation.modified_by().cloned(),
            access: observation.access().cloned(),
            external: observation
                .external()
                .map(|external| KnowledgeExternalIdentity {
                    system: external.system.clone(),
                    native_id: external.native_id.clone(),
                    mirrored_at: external.mirrored_at,
                }),
            not_modified: observation.not_modified(),
        }
    }
}
