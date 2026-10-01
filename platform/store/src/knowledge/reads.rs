use super::admission::Admission;
use super::*;
use crate::PlatformStore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_mcp_knowledge_extension::Observation;
use veoveo_types::{
    AccessSubject, DataLabelId, GatewayProfileId, ResourceUri, ScopeName, WorkContextId,
};

/// Constructed from current caller policy, including source-profile admission.
/// The service applies its canonical access decision to these narrowed candidates.
#[derive(Debug, Clone)]
pub struct CandidateScope {
    pub tenant: TenantId,
    pub profile: GatewayProfileId,
    pub active_work_context: WorkContextId,
    pub collections: BTreeSet<CollectionId>,
    pub work_contexts: BTreeSet<WorkContextId>,
    pub subjects: BTreeSet<AccessSubject>,
    pub scopes: BTreeSet<ScopeName>,
    pub clearance: BTreeSet<DataLabelId>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeCandidate {
    pub generation: GenerationId,
    pub tenant: TenantId,
    pub uri: ResourceUri,
    pub ordinal: u16,
    pub text: String,
    pub observation: Observation,
}
#[derive(Debug, Clone)]
pub struct CandidateCursor {
    tenant: TenantId,
    generation: GenerationId,
    collection: CollectionId,
    uri: ResourceUri,
    ordinal: u16,
}
impl KnowledgeCandidate {
    pub fn cursor(&self) -> CandidateCursor {
        CandidateCursor {
            tenant: self.tenant.clone(),
            generation: self.generation,
            collection: self.observation.collection().clone(),
            uri: self.uri.clone(),
            ordinal: self.ordinal,
        }
    }
}
#[derive(SurrealValue)]
struct Row {
    tenant: String,
    collection_id: String,
    uri: String,
    ordinal: i64,
    text: String,
    admission: Admission,
    observation: Document<Observation>,
}
impl Row {
    fn checked(
        self,
        scope: &CandidateScope,
        generation: GenerationId,
    ) -> Result<KnowledgeCandidate, StoreError> {
        let observation = self.observation.0;
        if self.tenant != scope.tenant.as_str()
            || self.collection_id != observation.collection().to_string()
            || self.admission != Admission::from(observation.access())
            || !(0..256).contains(&self.ordinal)
        {
            return integrity();
        }
        if observation
            .access()
            .is_some_and(|a| a.tenant != scope.tenant)
        {
            return integrity();
        }
        Ok(KnowledgeCandidate {
            generation,
            tenant: scope.tenant.clone(),
            uri: ResourceUri::new(self.uri)
                .map_err(|_| StoreError::Knowledge("invalid stored resource URI"))?,
            ordinal: self.ordinal as u16,
            text: self.text,
            observation,
        })
    }
}
impl PlatformStore {
    pub async fn knowledge_candidates_page(
        &self,
        scope: &CandidateScope,
        generation: GenerationId,
        after: Option<&CandidateCursor>,
        limit: u16,
    ) -> Result<Vec<KnowledgeCandidate>, StoreError> {
        if !(1..=100).contains(&limit)
            || scope.collections.len() > 1024
            || scope.work_contexts.len() > 1024
            || scope.subjects.len() > 1024
            || scope.clearance.len() > 1024
            || scope.scopes.len() > 1024
        {
            return Err(StoreError::Knowledge(
                "candidate scope or page exceeds its bound",
            ));
        }
        if after.is_some_and(|cursor| {
            cursor.tenant != scope.tenant
                || cursor.generation != generation
                || cursor.ordinal >= 256
                || !scope.collections.contains(&cursor.collection)
        }) {
            return Err(StoreError::Knowledge(
                "candidate cursor belongs to another tenant, generation or collection",
            ));
        }
        let sql = include_str!("candidates.surql").replace("__TABLE__", &chunk_table(generation));
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", scope.tenant.to_string()))
            .bind(("profile", scope.profile.to_string()))
            .bind((
                "scopes",
                scope
                    .scopes
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("active_context", scope.active_work_context.to_string()))
            .bind(("generation", generation_record(generation)))
            .bind((
                "active",
                RecordId::new("knowledge_active", scope.tenant.as_str()),
            ))
            .bind((
                "collections",
                scope
                    .collections
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "contexts",
                scope
                    .work_contexts
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "subjects",
                scope
                    .subjects
                    .iter()
                    .map(serialize_subject)
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "clearance",
                scope
                    .clearance
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("after_collection", after.map(|c| c.collection.to_string())))
            .bind(("after_uri", after.map(|c| c.uri.to_string())))
            .bind(("after_ordinal", after.map(|c| i64::from(c.ordinal))))
            .bind(("limit", i64::from(limit)))
            .await?
            .knowledge_check()?;
        let rows: Vec<Row> = response.take(0)?;
        rows.into_iter()
            .map(|row| row.checked(scope, generation))
            .collect()
    }
}
