//! Typed Artifact-plane operations and their audit acknowledgement rules.

use super::*;
use veoveo_mcp_contract::audit::{
    AuditActor, AuditAuthority, AuditContext, AuditDetail, AuditDraft, AuditPrincipalKind,
    AuditRequest, AuditTarget,
};
use veoveo_types::{PrincipalId, ResourceUri};

#[derive(PartialEq, Eq, Hash)]
struct WindowKey {
    tenant: Option<veoveo_types::TenantId>,
    principal: PrincipalId,
    artifact: ArtifactId,
    start: i64,
}

/// Each replica coalesces repeated ranges after the first acknowledged commit.
/// Eviction is safe because Store also claims the same window in the transaction.
#[derive(Default)]
pub(super) struct AuditWindows {
    entries: std::sync::Mutex<
        std::collections::HashMap<WindowKey, std::sync::Arc<tokio::sync::OnceCell<()>>>,
    >,
}

impl AuditWindows {
    fn cell(&self, draft: &AuditDraft) -> Option<std::sync::Arc<tokio::sync::OnceCell<()>>> {
        let AuditDetail::Artifact {
            activity: ArtifactActivity::Download,
            window_start: Some(start),
            ..
        } = draft.detail()
        else {
            return None;
        };
        let AuditTarget::Artifact { artifact } = draft.target() else {
            return None;
        };
        if draft.outcome() != AuditOutcome::Allowed {
            return None;
        }
        let actor = draft.actor()?;
        let key = WindowKey {
            tenant: actor.tenant.clone(),
            principal: actor.principal.clone(),
            artifact: *artifact,
            start: start.timestamp(),
        };
        let mut entries = self.entries.lock().expect("Artifact audit window cache");
        if let Some(cell) = entries.get(&key) {
            return Some(cell.clone());
        }
        entries.retain(|previous, _| previous.start >= key.start - 300);
        if entries.len() >= 4096 {
            entries.clear();
        }
        let cell = std::sync::Arc::new(tokio::sync::OnceCell::new());
        entries.insert(key, cell.clone());
        Some(cell)
    }
}

pub(super) struct ArtifactAction {
    activity: ArtifactActivity,
    target: AuditTarget,
    requested: Option<AccessLevel>,
    subject: Option<AccessSubject>,
    release_state: Option<ArtifactReleaseState>,
    related: Option<ResourceUri>,
    bytes: Option<u64>,
}

impl ArtifactAction {
    pub(super) fn artifact(activity: ArtifactActivity, artifact: ArtifactId) -> Self {
        Self::new(activity, AuditTarget::Artifact { artifact })
    }

    pub(super) fn surface(activity: ArtifactActivity, surface: ArtifactLedgerAddress) -> Self {
        Self::new(
            activity,
            AuditTarget::PlatformResource { uri: surface.uri() },
        )
    }

    fn new(activity: ArtifactActivity, target: AuditTarget) -> Self {
        Self {
            activity,
            target,
            requested: None,
            subject: None,
            release_state: None,
            related: None,
            bytes: None,
        }
    }

    pub(super) fn requested(mut self, level: AccessLevel) -> Self {
        self.requested = Some(level);
        self
    }

    pub(super) fn subject(mut self, subject: AccessSubject) -> Self {
        self.subject = Some(subject);
        self
    }

    pub(super) fn release_state(mut self, state: ArtifactReleaseState) -> Self {
        self.release_state = Some(state);
        self
    }

    pub(super) fn related(mut self, surface: ArtifactLedgerAddress) -> Self {
        self.related = Some(surface.uri());
        self
    }

    pub(super) fn bytes(mut self, bytes: u64) -> Self {
        self.bytes = Some(bytes);
        self
    }

    fn draft(
        self,
        context: Option<&AuditContext>,
        outcome: AuditOutcome,
        reason: AuditReason,
    ) -> Result<AuditDraft, ArtifactPlaneError> {
        // SQL gives all replicas the same immutable actor/artifact/window guard.
        // Denials always retain their individual request record.
        let window_start = (self.activity == ArtifactActivity::Download
            && outcome == AuditOutcome::Allowed)
            .then(|| {
                let seconds = Utc::now().timestamp();
                chrono::DateTime::from_timestamp(seconds.div_euclid(300) * 300, 0)
                    .expect("current five-minute UTC window")
            });
        let detail = AuditDetail::Artifact {
            activity: self.activity,
            requested: self.requested,
            subject: self.subject,
            release_state: self.release_state,
            related: self.related,
            bytes: self.bytes,
            window_start,
        };
        match context {
            Some(context) => context.draft(self.target, detail, outcome, reason),
            None => AuditDraft::builder(
                AuditRequest::background(),
                self.target,
                detail,
                outcome,
                reason,
            )
            .build(),
        }
        .map_err(|error| ArtifactPlaneError::Transport(error.to_string()))
    }
}

pub(super) fn access_reason(decision: AccessDecision) -> AuditReason {
    match decision {
        AccessDecision::Allow => AuditReason::Accepted,
        AccessDecision::DenyTenant => AuditReason::TenantMismatch,
        AccessDecision::DenyClearance => AuditReason::InsufficientClearance,
        AccessDecision::DenyNeedToKnow => AuditReason::InsufficientAccess,
    }
}

pub(super) fn share_context(link: ArtifactShareLinkId, stored: &StoredArtifact) -> AuditContext {
    AuditContext {
        actor: AuditActor {
            principal: PrincipalId::new(ArtifactLedgerAddress::Share(link).uri().as_str())
                .expect("share identity is a checked private URI"),
            kind: AuditPrincipalKind::Capability,
            tenant: Some(stored.tenant.clone()),
            oauth_client: None,
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        },
        // A public link conveys no gateway profile, Work Context membership or clearance.
        authority: AuditAuthority::default(),
        request: AuditRequest::background(),
    }
}

impl<R: ArtifactRepository, S: BlobStore> ArtifactService<R, S> {
    pub(super) async fn complete_artifact<T>(
        &self,
        context: &AuditContext,
        action: ArtifactAction,
        result: Result<T, ArtifactPlaneError>,
    ) -> Result<T, ArtifactPlaneError> {
        let (outcome, reason) = match &result {
            Ok(_) => (AuditOutcome::Succeeded, AuditReason::Accepted),
            Err(error) => (
                AuditOutcome::Failed,
                match error {
                    ArtifactPlaneError::NotFound => AuditReason::NotFound,
                    ArtifactPlaneError::Denied(decision) => access_reason(*decision),
                    ArtifactPlaneError::Unauthenticated => AuditReason::Unauthenticated,
                    ArtifactPlaneError::InvalidRequest(_) => AuditReason::InvalidRequest,
                    ArtifactPlaneError::Conflict(_) => AuditReason::Conflict,
                    ArtifactPlaneError::Transport(_) => AuditReason::Unavailable,
                },
            ),
        };
        self.audit(Some(context), action, outcome, reason).await?;
        result
    }

    pub(super) async fn audit(
        &self,
        context: Option<&AuditContext>,
        action: ArtifactAction,
        outcome: AuditOutcome,
        reason: AuditReason,
    ) -> Result<(), ArtifactPlaneError> {
        let draft = action.draft(context, outcome, reason)?;
        if let Some(cell) = self.audit_windows.cell(&draft) {
            cell.get_or_try_init(|| async { self.repository.append_audit(draft).await })
                .await
                .map_err(transport)?;
            return Ok(());
        }
        match outcome {
            AuditOutcome::Allowed | AuditOutcome::Denied => {
                self.repository.append_audit(draft).await.map_err(transport)
            }
            AuditOutcome::Succeeded | AuditOutcome::Failed => {
                self.repository.complete_audit(draft).await;
                Ok(())
            }
        }
    }
}
