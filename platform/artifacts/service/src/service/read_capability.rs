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
            Some(actor.clone()),
            Some(actor.tenant),
            "artifact.read_capability.issue",
            None,
            AuditOutcome::Allowed,
            serde_json::Map::from_iter([
                ("capability_id".into(), serde_json::json!(capability_id)),
                ("task_id".into(), serde_json::json!(request.task_id)),
            ]),
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
        ArtifactReadCapabilitySecret::new(secret)
            .map_err(|_| ArtifactPlaneError::Unauthenticated)?;
        let authentication = ReadCapabilityAuthentication {
            capability_id,
            token_hash: secret_hash(HASH_DOMAIN, secret),
            task_id,
        };
        let capability = self
            .repository
            .read_capability(&authentication)
            .await
            .map_err(transport)?
            .ok_or(ArtifactPlaneError::Unauthenticated)?;
        let stored = self.load(artifact_id).await?;
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
        let details = serde_json::Map::from_iter([
            ("capability_id".into(), serde_json::json!(capability_id)),
            ("task_id".into(), serde_json::json!(task_id)),
            ("decision".into(), serde_json::json!(decision)),
        ]);
        self.audit(
            Some(capability.actor.clone()),
            Some(stored.tenant.clone()),
            "artifact.read_capability.authorize",
            Some(artifact_id),
            if decision.is_allowed() {
                AuditOutcome::Allowed
            } else {
                AuditOutcome::Denied
            },
            details,
        )
        .await?;
        if decision != AccessDecision::Allow {
            return Err(ArtifactPlaneError::Denied(decision));
        }
        if !self
            .repository
            .admit_read_capability(&authentication, artifact_id, stored.metadata.byte_len)
            .await
            .map_err(transport)?
        {
            return Err(ArtifactPlaneError::Conflict(
                "read delegation quota exhausted or authority changed".into(),
            ));
        }
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
            return Err(ArtifactPlaneError::NotFound);
        }
        self.audit(
            Some(actor.clone()),
            Some(actor.tenant),
            "artifact.read_capability.revoke",
            None,
            AuditOutcome::Allowed,
            serde_json::Map::from_iter([(
                "capability_id".into(),
                serde_json::json!(capability_id),
            )]),
        )
        .await
    }
}
