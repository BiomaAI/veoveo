//! Frames owns operation authority, immutable replay, and its Store adapter.
use super::{FramesState, object_from_value, value_from_object};
use crate::contract::{CoordinateOperationId, CoordinateOperationProvenance, FrameOperationUri};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
use surrealdb::{Connection, method::Query, types::SurrealValue};
use veoveo_mcp_contract::GatewayProfileId;
use veoveo_platform_store::{
    OpenObject, PlatformTable, RecordId, deterministic_principal_id, deterministic_tenant_id,
    task_record_id,
};
use veoveo_types::{DataLabelId, PrincipalId, TaskId, TenantId};

/// Operation reads and writes require the same principal, tenant, profile and clearance.
/// World sharing has its separate `FrameScope` policy.
#[derive(Clone, Debug)]
pub struct FrameOperationScope {
    principal: PrincipalId,
    tenant: Option<TenantId>,
    profile: GatewayProfileId,
    clearance: BTreeSet<DataLabelId>,
}

impl FrameOperationScope {
    pub fn new(
        principal: PrincipalId,
        tenant: Option<TenantId>,
        profile: GatewayProfileId,
        clearance: BTreeSet<DataLabelId>,
    ) -> Self {
        Self {
            principal,
            tenant,
            profile,
            clearance,
        }
    }

    fn tenant_key(&self) -> &str {
        self.tenant
            .as_ref()
            .map_or("installation", TenantId::as_str)
    }
    fn tenant_record(&self) -> Result<RecordId> {
        Ok(deterministic_tenant_id(self.tenant_key())?.record_id())
    }
    fn owner_record(&self) -> Result<RecordId> {
        Ok(deterministic_principal_id(self.tenant_key(), self.principal.as_str())?.record_id())
    }
    fn profile_record(&self) -> RecordId {
        RecordId::new(PlatformTable::Profile.as_str(), self.profile.to_string())
    }
    fn labels(&self) -> Vec<String> {
        self.clearance.iter().map(ToString::to_string).collect()
    }
    fn bind<'q, C: Connection>(&self, query: Query<'q, C>) -> Result<Query<'q, C>> {
        Ok(query
            .bind(("tenant", self.tenant_record()?))
            .bind(("owner", self.owner_record()?))
            .bind(("profile", self.profile_record()))
            .bind(("principal_key", self.principal.to_string()))
            .bind(("profile_key", self.profile.to_string()))
            .bind(("tenant_key", self.tenant.as_ref().map(ToString::to_string)))
            .bind(("clearance", self.labels())))
    }
}

#[derive(Clone, Debug, PartialEq, SurrealValue)]
struct OperationAuthority {
    profile: RecordId,
    tenant_key: Option<String>,
}

/// Driver representation only. Public callers supply typed provenance and authority.
#[derive(Clone, Debug, PartialEq, SurrealValue)]
struct OperationContent {
    tenant: RecordId,
    owner: RecordId,
    task: Option<RecordId>,
    authority: OperationAuthority,
    operation_key: String,
    kind: String,
    provenance: OpenObject,
    classification: String,
    labels: Vec<String>,
    created_at: DateTime<Utc>,
}

impl FramesState {
    pub async fn record_operation(
        &self,
        scope: &FrameOperationScope,
        task: Option<TaskId>,
        provenance: &CoordinateOperationProvenance,
    ) -> Result<()> {
        if scope
            .clearance
            .iter()
            .any(|label| label.as_str().len() > 256)
        {
            bail!("Frames operation labels must not exceed 256 bytes");
        }
        let id = provenance.operation.operation_id();
        let Some(operation) = record_id(id) else {
            bail!("persisted Frames operation identity must be op- followed by a UUIDv7");
        };
        let kind = serde_json::to_value(&provenance.kind)?
            .as_str()
            .context("coordinate operation kind must serialize as a string")?
            .to_owned();
        let content = OperationContent {
            tenant: scope.tenant_record()?,
            owner: scope.owner_record()?,
            task: task.map(task_record_id),
            authority: OperationAuthority {
                profile: scope.profile_record(),
                tenant_key: scope.tenant.as_ref().map(ToString::to_string),
            },
            operation_key: id.to_string(),
            kind,
            provenance: object_from_value(serde_json::to_value(provenance)?)?,
            classification: "gateway_labels".to_owned(),
            labels: scope.labels(),
            created_at: provenance.operation.created_at,
        };

        let result = scope
            .bind(
                self.store
                    .client()
                    .query(include_str!("queries/operations/record.surql")),
            )?
            .bind(("operation", operation))
            .bind(("task", task.map(task_record_id)))
            .bind(("content", content.clone()))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            // A concurrent winner or lost acknowledgement is accepted only after
            // observing the identical committed record under current authority.
            if self
                .operation_content(scope, provenance.operation.operation_uri())
                .await?
                .is_some_and(|existing| existing == content)
            {
                return Ok(());
            }
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn get_operation(
        &self,
        scope: &FrameOperationScope,
        uri: &FrameOperationUri,
    ) -> Result<Option<CoordinateOperationProvenance>> {
        self.operation_content(scope, uri).await?
            .map(|record| {
                let result: CoordinateOperationProvenance = serde_json::from_value(value_from_object(record.provenance))?;
                if result.operation.operation_uri() != uri || result.operation.created_at != record.created_at {
                    bail!("stored Frames provenance disagrees with its operation identity or timestamp");
                }
                Ok(result)
            }).transpose()
    }

    async fn operation_content(
        &self,
        scope: &FrameOperationScope,
        uri: &FrameOperationUri,
    ) -> Result<Option<OperationContent>> {
        let Some(operation) = record_id(uri.operation_id()) else {
            return Ok(None);
        };
        let mut response = scope
            .bind(
                self.store
                    .client()
                    .query(include_str!("queries/read_operation.surql")),
            )?
            .bind(("operation", operation))
            .bind(("operation_key", uri.operation_id().to_string()))
            .await?
            .check()?;
        let rows: Vec<OperationContent> = response.take(0)?;
        Ok(rows.into_iter().next())
    }
}

fn record_id(id: &CoordinateOperationId) -> Option<RecordId> {
    let id = uuid::Uuid::parse_str(id.as_str().strip_prefix("op-")?).ok()?;
    (id.get_version_num() == 7).then(|| {
        RecordId::new(
            crate::FramesObservationTable::CoordinateOperation.as_str(),
            surrealdb::types::Uuid::from(id),
        )
    })
}

#[cfg(test)]
mod tests;
