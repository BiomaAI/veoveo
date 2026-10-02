//! Wake at stored access deadlines, including members beyond the first index page.
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use veoveo_artifact_contract::ArtifactId;
use veoveo_mcp_contract::PlaneCaller;
use veoveo_platform_store::{
    ArtifactReadScope, PlatformIdentity, deterministic_principal_id, deterministic_tenant_id,
};

use super::ArtifactSubscriptions;

impl ArtifactSubscriptions {
    pub(crate) async fn deadline(
        &self,
        caller: &PlaneCaller,
        members: Option<&[ArtifactId]>,
    ) -> Result<DateTime<Utc>> {
        let identity = &caller.identity;
        let tenant = identity
            .actor
            .tenant
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Artifact observation requires a tenant"))?;
        ensure!(
            *tenant == identity.authority.tenant,
            "Artifact tenant mismatch"
        );
        let scope = ArtifactReadScope::new(
            &PlatformIdentity {
                tenant_id: deterministic_tenant_id(tenant.as_str())?,
                principal_id: deterministic_principal_id(
                    tenant.as_str(),
                    identity.actor.id.as_str(),
                )?,
                tenant_key: tenant.to_string(),
                principal_key: identity.actor.id.to_string(),
            },
            caller
                .memberships
                .iter()
                .map(|membership| membership.group.clone()),
            caller.clearance().clone(),
            Some(identity.authority.work_context.clone()),
        )?;
        let mut response = scope
            .bind(
                self.store.client().query(
                    include_str!("deadlines.surql")
                        .replace("{{ADMISSION}}", ArtifactReadScope::ADMISSION),
                ),
            )
            .bind(("all", members.is_none()))
            .bind((
                "members",
                members
                    .unwrap_or_default()
                    .iter()
                    .map(|id| {
                        veoveo_platform_store::ArtifactId::from_uuid(id.as_uuid()).record_id()
                    })
                    .collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        let deadline: Option<DateTime<Utc>> = response.take(response.num_statements() - 1)?;
        Ok(deadline.map_or(identity.expires_at, |deadline| {
            deadline.min(identity.expires_at)
        }))
    }
}
