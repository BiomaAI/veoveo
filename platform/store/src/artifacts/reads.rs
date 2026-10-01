//! Artifact read admission precedes row decoding and page limits.
use super::*;
use std::collections::BTreeSet;
use veoveo_types::{DataLabelId, GroupId as GroupKey, WorkContextId as WorkContextKey};

pub struct ArtifactReadScope {
    tenant: TenantId,
    subjects: Vec<RecordId>,
    clearance: BTreeSet<DataLabelId>,
    context: Option<crate::WorkContextId>,
    context_key: Option<WorkContextKey>,
}

impl ArtifactReadScope {
    pub fn new(
        identity: &PlatformIdentity,
        groups: impl IntoIterator<Item = GroupKey>,
        clearance: BTreeSet<DataLabelId>,
        context: Option<WorkContextKey>,
    ) -> Result<Self, StoreError> {
        let mut subjects = vec![identity.principal_id.record_id()];
        for group in groups {
            subjects.push(
                crate::deterministic_group_id(&identity.tenant_key, group.as_str())?.record_id(),
            );
        }
        Ok(Self {
            tenant: identity.tenant_id,
            subjects,
            clearance,
            context: context
                .as_ref()
                .map(|key| deterministic_work_context_id(&identity.tenant_key, key.as_str()))
                .transpose()?,
            context_key: context,
        })
    }
}

impl PlatformStore {
    pub async fn artifact_read(
        &self,
        scope: ArtifactReadScope,
        artifact: ArtifactId,
    ) -> Result<Option<ArtifactAggregate>, StoreError> {
        Ok(self
            .artifact_reads(scope, Some(artifact), None, 1)
            .await?
            .pop())
    }

    pub async fn artifact_read_page(
        &self,
        scope: ArtifactReadScope,
        cursor: Option<ArtifactId>,
        limit: usize,
    ) -> Result<Vec<ArtifactAggregate>, StoreError> {
        self.artifact_reads(scope, None, cursor, limit).await
    }

    async fn artifact_reads(
        &self,
        scope: ArtifactReadScope,
        artifact: Option<ArtifactId>,
        cursor: Option<ArtifactId>,
        limit: usize,
    ) -> Result<Vec<ArtifactAggregate>, StoreError> {
        let mut response = self
            .db
            .query(include_str!("read_page.surql"))
            .bind(("tenant", scope.tenant.record_id()))
            .bind(("subjects", scope.subjects))
            .bind((
                "clearance",
                scope
                    .clearance
                    .into_iter()
                    .map(String::from)
                    .collect::<Vec<_>>(),
            ))
            .bind(("context", scope.context.map(|id| id.record_id())))
            .bind(("context_key", scope.context_key.map(String::from)))
            .bind(("artifact", artifact.map(|id| id.record_id())))
            .bind(("cursor", cursor.map(|id| id.record_id())))
            .bind(("limit", i64::try_from(limit.min(101)).unwrap()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}
