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
pub(super) struct Row {
    pub(super) tenant: String,
    pub(super) collection_id: String,
    pub(super) uri: String,
    pub(super) ordinal: i64,
    pub(super) text: String,
    pub(super) admission: Admission,
    pub(super) observation: Document<Observation>,
}
impl Row {
    pub(super) fn checked(
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
        scope.validate()?;
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Knowledge("candidate page exceeds its bound"));
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
        let sql = include_str!("candidates.surql")
            .replace("__ADMISSION__", include_str!("admitted.surql"))
            .replace("__TABLE__", &chunk_table(generation));
        let mut response = scope
            .bind(self.client().query(sql), generation)
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

impl CandidateScope {
    pub(super) fn validate(&self) -> Result<(), StoreError> {
        if self.collections.len() > 1024
            || self.work_contexts.len() > 1024
            || self.subjects.len() > 1024
            || self.clearance.len() > 1024
            || self.scopes.len() > 1024
        {
            return Err(StoreError::Knowledge("candidate scope exceeds its bound"));
        }
        Ok(())
    }
    pub(super) fn bind<'a, C: surrealdb::Connection>(
        &self,
        query: surrealdb::method::Query<'a, C>,
        generation: GenerationId,
    ) -> surrealdb::method::Query<'a, C> {
        query
            .bind(("tenant", self.tenant.to_string()))
            .bind(("profile", self.profile.to_string()))
            .bind((
                "scopes",
                self.scopes
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("active_context", self.active_work_context.to_string()))
            .bind(("generation", generation_record(generation)))
            .bind((
                "active",
                RecordId::new("knowledge_active", self.tenant.as_str()),
            ))
            .bind((
                "collections",
                self.collections
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "contexts",
                self.work_contexts
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "subjects",
                self.subjects
                    .iter()
                    .map(serialize_subject)
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "clearance",
                self.clearance
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
    }
}
