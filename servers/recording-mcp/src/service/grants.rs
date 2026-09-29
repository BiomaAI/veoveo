//! Recording grant issuance and caller scope at the authenticated service boundary.
use super::{RecordingService, VIEWER_GRANT_TTL};
use anyhow::Result;
use chrono::Utc;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{
    RecordingAccessScope, RecordingDatasetId, RecordingId, RecordingReadGrantClass,
    RecordingReadGrantDraft, RecordingReadGrantId, RecordingReadGrantRecord,
    RecordingReadGrantRequest,
};

impl RecordingService {
    pub async fn issue_read_grant(
        &self,
        identity: &GatewayInternalIdentity,
        dataset_id: RecordingDatasetId,
        grant_class: RecordingReadGrantClass,
        recording_ids: Vec<RecordingId>,
        catalog_revision: String,
        requested_grant: Option<crate::contract::RecordingReadGrantId>,
    ) -> Result<RecordingReadGrantRecord> {
        let platform = self.platform_identity(identity).await?;
        let scope = recording_access_scope(identity, &platform)?;
        let request = RecordingReadGrantRequest::new(
            dataset_id,
            grant_class,
            recording_ids,
            catalog_revision,
        )?;
        if let Some(grant_id) = requested_grant
            && let Some(grant) = self
                .store
                .reusable_recording_read_grant(
                    &scope,
                    &request,
                    RecordingReadGrantId::from_uuid(grant_id.as_uuid()),
                )
                .await?
        {
            return Ok(grant);
        }
        Ok(self
            .store
            .create_recording_read_grant(RecordingReadGrantDraft {
                scope,
                request,
                expires_at: Utc::now() + VIEWER_GRANT_TTL,
            })
            .await?)
    }
}

pub(super) fn recording_access_scope(
    identity: &GatewayInternalIdentity,
    platform_identity: &veoveo_platform_store::PlatformIdentity,
) -> Result<RecordingAccessScope> {
    Ok(RecordingAccessScope {
        tenant_id: platform_identity.tenant_id,
        actor_id: platform_identity.principal_id,
        work_context_id: veoveo_platform_store::deterministic_work_context_id(
            &platform_identity.tenant_key,
            identity.authority.work_context.as_str(),
        )?,
        policy_revision: identity.authority.policy_revision.clone(),
        data_labels: identity.actor.data_labels.clone(),
    })
}
