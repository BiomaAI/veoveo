//! Caller ownership is selected before an upload descriptor can be decoded.
use super::*;
use veoveo_artifact_contract::ArtifactUploadId;
use veoveo_types::{
    GatewayProfileId, PrincipalId, TenantId, TokenIssuer, TokenSubject, WorkContextId,
};

/// Signed identities required to select a caller-owned upload. This selection does
/// not grant access; the service also checks the current profile and Work Context.
#[derive(Debug, Clone, Copy)]
pub struct ArtifactUploadOwner<'a> {
    pub tenant: &'a TenantId,
    pub actor: &'a PrincipalId,
    pub profile: &'a GatewayProfileId,
    pub work_context: &'a WorkContextId,
    pub issuer: &'a TokenIssuer,
    pub subject: &'a TokenSubject,
}

impl PlatformStore {
    pub async fn owned_artifact_upload(
        &self,
        id: ArtifactUploadId,
        owner: ArtifactUploadOwner<'_>,
    ) -> Result<Option<ArtifactUploadRecord>, StoreError> {
        let mut response = self
            .db
            .query(include_str!("owned.surql"))
            .bind(("upload", upload_record_id(id.as_uuid())))
            .bind(("tenant_key", owner.tenant.to_string()))
            .bind((
                "tenant",
                crate::deterministic_tenant_id(owner.tenant.as_str())?.record_id(),
            ))
            .bind(("actor_key", owner.actor.to_string()))
            .bind((
                "actor",
                crate::deterministic_principal_id(owner.tenant.as_str(), owner.actor.as_str())?
                    .record_id(),
            ))
            .bind(("profile", owner.profile.to_string()))
            .bind(("context_key", owner.work_context.to_string()))
            .bind((
                "context",
                crate::deterministic_work_context_id(
                    owner.tenant.as_str(),
                    owner.work_context.as_str(),
                )?
                .record_id(),
            ))
            .bind(("issuer", owner.issuer.to_string()))
            .bind(("subject", owner.subject.to_string()))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }
}
