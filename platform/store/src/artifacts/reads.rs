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

    /// Bind one checked caller scope to an Artifact query or an admitting subquery.
    pub fn bind<'a, C: surrealdb::Connection>(
        &self,
        query: surrealdb::method::Query<'a, C>,
    ) -> surrealdb::method::Query<'a, C> {
        query
            .bind(("tenant", self.tenant.record_id()))
            .bind(("subjects", self.subjects.clone()))
            .bind((
                "principals",
                self.subjects
                    .iter()
                    .filter(|id| id.table.as_str() == "principal")
                    .cloned()
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "groups",
                self.subjects
                    .iter()
                    .filter(|id| id.table.as_str() == "principal_group")
                    .cloned()
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "clearance",
                self.clearance
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("context", self.context.map(|id| id.record_id())))
            .bind((
                "context_key",
                self.context_key.as_ref().map(ToString::to_string),
            ))
    }
}

impl PlatformStore {
    /// Earliest visible access expiry across the complete selected collection.
    /// `None` selects all artifacts; an empty slice selects no members.
    pub async fn artifact_read_deadline(
        &self,
        scope: ArtifactReadScope,
        members: Option<&[ArtifactId]>,
    ) -> Result<Option<DateTime<Utc>>, StoreError> {
        let mut response = scope
            .bind(
                self.db
                    .query(include_str!("../queries/artifacts/read_deadline.surql")),
            )
            .bind(("all", members.is_none()))
            .bind((
                "members",
                members
                    .unwrap_or_default()
                    .iter()
                    .map(|id| id.record_id())
                    .collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        Ok(response.take(response.num_statements() - 1)?)
    }

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
        let mut response = scope
            .bind(
                self.db
                    .query(include_str!("../queries/artifacts/read_page.surql")),
            )
            .bind(("artifact", artifact.map(|id| id.record_id())))
            .bind(("cursor", cursor.map(|id| id.record_id())))
            .bind(("limit", i64::try_from(limit.min(101)).unwrap()))
            .await?
            .check()?;
        Ok(response.take(response.num_statements() - 1)?)
    }
}
