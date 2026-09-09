//! Current signed policy binding and session ownership, before exposing metadata.

use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub(super) struct Authority {
    pub policy: contract::ArtifactUploadPolicy,
    pub policy_digest: String,
    pub version: platform::ArtifactUploadAuthorityVersion,
    pub context: platform::WorkContextRecord,
    pub identity: platform::PlatformIdentity,
}

#[derive(Deserialize)]
struct ProfileUploadPolicy {
    artifact_upload: Option<contract::ArtifactUploadPolicy>,
}

impl UploadService {
    pub(super) async fn authorize(
        &self,
        caller: &contract::VerifiedArtifactUploadIdentity,
    ) -> Result<Authority, UploadFault> {
        let identity = &caller.identity;
        let actor = &identity.actor;
        let denied = contract::UploadErrorCode::Denied;
        if identity.server.as_str() != contract::ARTIFACT_UPLOAD_AUDIENCE
            || identity.expires_at <= Utc::now()
        {
            return Err(contract::UploadErrorCode::Unauthenticated.into());
        }
        let tenant = actor.tenant.as_ref().ok_or(denied)?;
        if &identity.authority.tenant != tenant
            || !actor
                .scopes
                .iter()
                .any(|scope| scope.as_str() == "artifact:upload")
            || !identity
                .authority
                .membership
                .allows(contract::WorkContextMembershipLevel::Contributor)
        {
            return Err(denied.into());
        }
        let profile = self
            .database
            .artifact_upload_profile(identity.profile.as_str())
            .await?
            .ok_or(denied)?;
        let profile: ProfileUploadPolicy = serde_json::from_value(
            serde_json::to_value(profile).map_err(|_| UploadFault::unavailable())?,
        )
        .map_err(|_| UploadFault::unavailable())?;
        let policy = profile.artifact_upload.ok_or(denied)?;
        policy.validate().map_err(|_| UploadFault::unavailable())?;
        let version = self
            .database
            .artifact_upload_authority_version(
                tenant.as_str(),
                identity.authority.work_context.as_str(),
                identity.profile.as_str(),
            )
            .await?
            .ok_or(denied)?;
        if version.control_plane_sha256 != caller.authorization.control_plane_sha256.as_str()
            || version.context_digest != caller.authorization.context_digest.as_str()
            || version.policy_revision != identity.authority.policy_revision.as_str()
            || version.profile_policy_digest.is_none()
        {
            return Err(denied.into());
        }
        let tenant_id = platform::deterministic_tenant_id(tenant.as_str())?;
        let context = self
            .database
            .work_context_by_key(tenant_id, identity.authority.work_context.as_str())
            .await?
            .ok_or(denied)?;
        let authority = crate::ledger::surreal::platform_authority(&identity.authority);
        if context.policy_revision != authority.policy_revision
            || context.output_policy.owner_kind != authority.owner_kind
            || context.output_policy.owner_key != authority.owner_key
            || context.output_policy.initial_grants != authority.initial_grants
            || context.output_policy.classification != authority.classification
            || context.output_policy.data_labels != authority.data_labels
        {
            return Err(denied.into());
        }
        let policy_digest = hex::encode(Sha256::digest(
            serde_json::to_vec(&policy).map_err(|_| UploadFault::unavailable())?,
        ));
        Ok(Authority {
            policy,
            policy_digest,
            version,
            context,
            identity: platform::PlatformIdentity {
                tenant_id,
                principal_id: platform::deterministic_principal_id(
                    tenant.as_str(),
                    actor.id.as_str(),
                )?,
                tenant_key: tenant.to_string(),
                principal_key: actor.id.to_string(),
            },
        })
    }

    pub(super) async fn owned(
        &self,
        caller: &contract::VerifiedArtifactUploadIdentity,
        id: contract::ArtifactUploadId,
    ) -> Result<(Authority, platform::ArtifactUploadRecord), UploadFault> {
        // Foreign identifiers disclose no descriptor, counters, or terminal state.
        let row = self
            .database
            .artifact_upload(id.as_uuid())
            .await?
            .ok_or(contract::UploadErrorCode::NotFound)?;
        let identity = &caller.identity;
        if identity
            .actor
            .tenant
            .as_ref()
            .is_none_or(|tenant| tenant.as_str() != row.tenant_key)
            || identity.actor.id.as_str() != row.actor_key
            || identity.profile.as_str() != row.profile_key
            || identity.authority.work_context.as_str() != row.authority.context_key
            || identity.actor.issuer.as_str() != row.actor_issuer
            || identity.actor.subject.as_str() != row.actor_subject
        {
            return Err(contract::UploadErrorCode::NotFound.into());
        }
        let authority = self.authorize(caller).await?;
        if !matches!(
            row.state,
            platform::ArtifactUploadState::Completed
                | platform::ArtifactUploadState::Cancelled
                | platform::ArtifactUploadState::Expired
                | platform::ArtifactUploadState::Failed
        ) && (row.context_digest != authority.version.context_digest
            || row.policy_digest != authority.policy_digest
            || Some(&row.profile_policy_digest) != authority.version.profile_policy_digest.as_ref())
        {
            return Err(contract::UploadErrorCode::Denied.into());
        }
        Ok((authority, row))
    }

    pub async fn policy(
        &self,
        caller: &contract::VerifiedArtifactUploadIdentity,
    ) -> Result<contract::EffectiveArtifactUploadPolicy, UploadFault> {
        let authority = self.authorize(caller).await?;
        let usage = self
            .database
            .artifact_upload_storage_usage(authority.identity.tenant_id)
            .await?;
        let used = usage
            .map(|u| {
                u.committed_bytes
                    .saturating_add(u.reserved_bytes)
                    .saturating_add(u.cleanup_bytes)
            })
            .unwrap_or(0);
        let owner_is_actor = authority.context.output_policy.owner_kind
            == platform::ArtifactGrantSubjectKind::Principal
            && authority.context.output_policy.owner_key == caller.identity.actor.id.as_str();
        Ok(contract::EffectiveArtifactUploadPolicy {
            allowed: true,
            explanation: "Files become governed artifacts after verification.".into(),
            actor: caller.identity.actor.id.clone(),
            work_context: caller.identity.authority.work_context.clone(),
            destination_name: authority.context.title,
            access_description: if owner_is_actor {
                "You own these artifacts. Sharing follows this Work Context.".into()
            } else {
                "Ownership and sharing follow this Work Context's configured access.".into()
            },
            available_bytes: Some(
                authority
                    .policy
                    .tenant_quota_bytes
                    .get()
                    .saturating_sub(u64::try_from(used).map_err(|_| UploadFault::unavailable())?),
            ),
            policy: Some(authority.policy),
        })
    }
}

impl Authority {
    pub fn admission(
        &self,
        caller: &contract::VerifiedArtifactUploadIdentity,
        request_id: contract::ArtifactUploadRequestId,
        descriptor: contract::CreateArtifactUpload,
        layout: &contract::UploadLayout,
    ) -> Result<platform::ArtifactUploadRecord, UploadFault> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();
        let lifetime = now + TimeDelta::seconds(self.policy.lifetime_seconds.get() as i64);
        let actor = &caller.identity.actor;
        let reserved = descriptor.byte_len.unwrap_or_else(|| {
            (layout.part_bytes.get() * u64::from(layout.parallel_parts.get()))
                .min(layout.max_total_bytes.get())
        });
        Ok(platform::ArtifactUploadRecord {
            id: platform::upload_record_id(id),
            tenant: self.identity.tenant_id.record_id(),
            tenant_key: self.identity.tenant_key.clone(),
            actor: self.identity.principal_id.record_id(),
            actor_key: self.identity.principal_key.clone(),
            actor_kind: match actor.kind {
                contract::PrincipalKind::User => platform::PrincipalKind::User,
                contract::PrincipalKind::Service => platform::PrincipalKind::Service,
            },
            actor_issuer: actor.issuer.to_string(),
            actor_subject: actor.subject.to_string(),
            profile_key: caller.identity.profile.to_string(),
            work_context: self.context.id.clone(),
            authority: crate::ledger::surreal::platform_authority(&caller.identity.authority),
            context_digest: self.version.context_digest.clone(),
            policy_digest: self.policy_digest.clone(),
            profile_policy_digest: self
                .version
                .profile_policy_digest
                .clone()
                .ok_or(contract::UploadErrorCode::Denied)?,
            request_id: request_id.as_uuid(),
            descriptor: platform::ArtifactUploadDescriptor {
                filename: descriptor.filename,
                mime_type: descriptor.mime_type,
                byte_len: descriptor.byte_len.map(|size| size as i64),
                sha256: descriptor.sha256.map(Into::into),
            },
            layout: platform::ArtifactUploadLayout {
                part_bytes: layout.part_bytes.get() as i64,
                max_parts: i64::from(layout.max_parts.get()),
                max_total_bytes: layout.max_total_bytes.get() as i64,
                parallel_parts: i64::from(layout.parallel_parts.get()),
            },
            state: platform::ArtifactUploadState::Open,
            reserved_bytes: reserved as i64,
            accepted_bytes: 0,
            accepted_part_count: 0,
            object_key: format!("tenants/{}/uploads/{id}", self.identity.tenant_id),
            multipart_id: None,
            generation: 0,
            lease_owner: None,
            lease_until: None,
            manifest: None,
            artifact: platform::ArtifactId::new().record_id(),
            verified_sha256: None,
            completed_at: None,
            failure: None,
            cleanup_pending: false,
            cleanup_bytes: 0,
            inactivity_seconds: self.policy.inactivity_seconds.get() as i64,
            created_at: now,
            updated_at: now,
            expires_at: now + TimeDelta::seconds(self.policy.inactivity_seconds.get() as i64),
            lifetime_ends_at: lifetime,
        })
    }
}
