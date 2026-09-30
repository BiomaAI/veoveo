use super::*;
use crate::ledger::{ReadCapabilityAuthentication, ReadCapabilityDraft};
use veoveo_mcp_contract::{
    ArtifactReadCapabilityId, ArtifactReadCapabilityScope, ArtifactReadCapabilitySecret,
    ArtifactTaskId, IssueArtifactReadCapabilityRequest, IssuedArtifactReadCapability,
};

const HASH_DOMAIN: &[u8] = b"veoveo.artifact-read.v1";

impl<R: ArtifactRepository, S: BlobStore> ArtifactService<R, S> {
    pub async fn issue_read_capability(
        &self,
        caller: &PlaneCaller,
        request: IssueArtifactReadCapabilityRequest,
    ) -> Result<IssuedArtifactReadCapability, ArtifactPlaneError> {
        let now = Utc::now();
        if request.expires_at <= now
            || request.expires_at > now + CAPABILITY_MAX_TTL
            || request.max_artifact_count.get() > 10_000
            || request.max_total_bytes.get() > i64::MAX as u64
        {
            return Err(ArtifactPlaneError::InvalidRequest("read delegation must expire within 24 hours and admit at most 10000 distinct artifacts within the signed 64-bit byte bound".into()));
        }
        let actor = Self::actor(caller)?;
        let context = self
            .repository
            .read_context(&actor, &caller.identity.authority.work_context)
            .await
            .map_err(transport)?
            .ok_or(ArtifactPlaneError::Unauthenticated)?;
        if context.policy_revision != caller.identity.authority.policy_revision {
            return Err(ArtifactPlaneError::Unauthenticated);
        }
        let capability_id = ArtifactReadCapabilityId::new();
        let secret = random_secret()?;
        self.repository
            .create_read_capability(ReadCapabilityDraft {
                capability_id,
                actor: actor.clone(),
                authority: caller.identity.authority.clone(),
                context,
                profile: caller.identity.profile.clone(),
                server: caller.identity.server.clone(),
                task_id: request.task_id,
                token_hash: secret_hash(HASH_DOMAIN, &secret),
                labels: caller.clearance().clone(),
                memberships: caller.memberships.clone(),
                max_artifact_count: request.max_artifact_count.get(),
                max_total_bytes: request.max_total_bytes.get(),
                expires_at: request.expires_at,
            })
            .await
            .map_err(transport)?;
        self.audit(
            Some(&actor.audit),
            ArtifactAction::surface(
                ArtifactActivity::ReadCapabilityIssue,
                ArtifactLedgerAddress::ReadCapability(capability_id),
            ),
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        )
        .await?;
        Ok(IssuedArtifactReadCapability {
            capability_id,
            secret: ArtifactReadCapabilitySecret::new(secret)?,
            task_id: request.task_id,
            expires_at: request.expires_at,
        })
    }

    pub async fn read_capability_scope(
        &self,
        capability_id: ArtifactReadCapabilityId,
        secret: &str,
        task_id: ArtifactTaskId,
    ) -> Result<ArtifactReadCapabilityScope, ArtifactPlaneError> {
        ArtifactReadCapabilitySecret::new(secret)
            .map_err(|_| ArtifactPlaneError::Unauthenticated)?;
        let cap = self
            .repository
            .read_capability(&ReadCapabilityAuthentication {
                capability_id,
                token_hash: secret_hash(HASH_DOMAIN, secret),
                task_id,
            })
            .await
            .map_err(transport)?
            .ok_or(ArtifactPlaneError::Unauthenticated)?;
        Ok(ArtifactReadCapabilityScope {
            task_id: cap.task_id,
            principal_id: cap.actor.principal,
            principal_kind: cap.actor.kind,
            issuer: cap.actor.issuer,
            subject: cap.actor.subject,
            tenant: cap.actor.tenant,
            data_labels: cap.labels,
            max_total_bytes: NonZeroU64::new(cap.max_total_bytes).ok_or_else(|| {
                ArtifactPlaneError::Transport("corrupt read capability byte limit".into())
            })?,
        })
    }

    async fn authorized_capability_read(
        &self,
        capability_id: ArtifactReadCapabilityId,
        secret: &str,
        task_id: ArtifactTaskId,
        artifact_id: ArtifactId,
    ) -> Result<StoredArtifact, ArtifactPlaneError> {
        let action = || {
            ArtifactAction::artifact(ArtifactActivity::Download, artifact_id)
                .related(ArtifactLedgerAddress::ReadCapability(capability_id))
                .requested(AccessLevel::Read)
        };
        if ArtifactReadCapabilitySecret::new(secret).is_err() {
            self.audit(
                None,
                action(),
                AuditOutcome::Denied,
                AuditReason::InvalidCredential,
            )
            .await?;
            return Err(ArtifactPlaneError::Unauthenticated);
        }
        let authentication = ReadCapabilityAuthentication {
            capability_id,
            token_hash: secret_hash(HASH_DOMAIN, secret),
            task_id,
        };
        let capability = self
            .repository
            .read_capability(&authentication)
            .await
            .map_err(transport)?;
        let Some(capability) = capability else {
            self.audit(
                None,
                action(),
                AuditOutcome::Denied,
                AuditReason::InvalidCredential,
            )
            .await?;
            return Err(ArtifactPlaneError::Unauthenticated);
        };
        let mut context = capability.actor.audit.clone();
        context.request = veoveo_mcp_contract::audit::AuditRequest::background();
        let stored = match self.load(artifact_id).await {
            Ok(stored) => stored,
            Err(error) => return self.complete_artifact(&context, action(), Err(error)).await,
        };
        // Delegation is an access ceiling, never a replacement gateway identity.
        // The occurrence and its grants/labels are loaded for every read.
        let decision = decide(&AccessRequest {
            caller_id: &capability.actor.principal,
            caller_tenant: Some(&capability.actor.tenant),
            caller_labels: &capability.labels,
            memberships: &capability.memberships,
            artifact_tenant: &stored.tenant,
            artifact_labels: &stored.labels,
            grants: &stored.grants,
            context_membership: (stored.metadata.compliance.work_context.as_ref()
                == Some(&capability.authority.work_context))
            .then_some(capability.authority.membership),
            requested: AccessLevel::Read,
        });
        if decision != AccessDecision::Allow {
            self.audit(
                Some(&context),
                action(),
                AuditOutcome::Denied,
                access_reason(decision),
            )
            .await?;
            return Err(ArtifactPlaneError::Denied(decision));
        }
        if !self
            .repository
            .admit_read_capability(&authentication, artifact_id, stored.metadata.byte_len)
            .await
            .map_err(transport)?
        {
            self.audit(
                Some(&context),
                action(),
                AuditOutcome::Denied,
                AuditReason::Conflict,
            )
            .await?;
            return Err(ArtifactPlaneError::Conflict(
                "read delegation quota exhausted or authority changed".into(),
            ));
        }
        self.audit(
            Some(&context),
            action(),
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        )
        .await?;
        Ok(stored)
    }

    pub async fn head_with_read_capability(
        &self,
        capability_id: ArtifactReadCapabilityId,
        secret: &str,
        task_id: ArtifactTaskId,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        Ok(self
            .authorized_capability_read(capability_id, secret, task_id, artifact_id)
            .await?
            .metadata)
    }

    pub async fn download_with_read_capability(
        &self,
        capability_id: ArtifactReadCapabilityId,
        secret: &str,
        task_id: ArtifactTaskId,
        artifact_id: ArtifactId,
        range: Option<ArtifactByteRange>,
        body: DownloadBody,
    ) -> Result<ArtifactDownload, ArtifactPlaneError> {
        let stored = self
            .authorized_capability_read(capability_id, secret, task_id, artifact_id)
            .await?;
        self.delivery(stored, range, body).await
    }

    pub async fn revoke_read_capability(
        &self,
        caller: &PlaneCaller,
        capability_id: ArtifactReadCapabilityId,
    ) -> Result<(), ArtifactPlaneError> {
        let actor = Self::actor(caller)?;
        if !self
            .repository
            .revoke_read_capability(capability_id, &actor)
            .await
            .map_err(transport)?
        {
            self.audit(
                Some(&actor.audit),
                ArtifactAction::surface(
                    ArtifactActivity::ReadCapabilityRevoke,
                    ArtifactLedgerAddress::ReadCapability(capability_id),
                ),
                AuditOutcome::Denied,
                AuditReason::NotFound,
            )
            .await?;
            return Err(ArtifactPlaneError::NotFound);
        }
        self.audit(
            Some(&actor.audit),
            ArtifactAction::surface(
                ArtifactActivity::ReadCapabilityRevoke,
                ArtifactLedgerAddress::ReadCapability(capability_id),
            ),
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )
        .await
    }
}
