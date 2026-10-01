//! Gateway-owned conversion from admitted identity and policy into audit contracts.
use crate::{
    AuthenticatedSubject, GatewayState,
    request_observation::{RequestStage, StageTimer},
};
use anyhow::{Context, Result};
use std::num::NonZeroU64;
use veoveo_audit_contract::*;
use veoveo_mcp_contract::{GatewayProfileId, PolicyTarget, PrincipalKind};

impl AuthenticatedSubject {
    pub fn audit_context(&self, profile: &GatewayProfileId) -> Result<AuditContext> {
        Ok(AuditContext {
            actor: self.audit_actor()?,
            authority: self.audit_authority(profile),
            request: self.audit.clone(),
        })
    }
    pub fn audit_actor(&self) -> Result<AuditActor> {
        let managed_agent = self
            .access_token
            .managed_agent
            .as_ref()
            .map(|agent| {
                Ok::<_, anyhow::Error>(AuditManagedExecution {
                    instance: agent.instance.clone(),
                    generation: NonZeroU64::new(u64::try_from(agent.generation)?)
                        .context("zero agent generation")?,
                    dispatch_epoch: NonZeroU64::new(u64::try_from(agent.epoch)?)
                        .context("zero agent epoch")?,
                    episode: None,
                })
            })
            .transpose()?;
        Ok(AuditActor {
            principal: self.actor.id.clone(),
            kind: match self.actor.kind {
                PrincipalKind::User => AuditPrincipalKind::User,
                PrincipalKind::Service => AuditPrincipalKind::Service,
            },
            tenant: self.actor.tenant.clone(),
            oauth_client: Some(self.access_token.oauth_client_id.clone()),
            session_family: self.access_token.session_family.clone(),
            delegating_principal: (self.principal.id != self.actor.id)
                .then(|| self.principal.id.clone()),
            managed_agent,
        })
    }
    pub fn audit_authority(&self, profile: &GatewayProfileId) -> AuditAuthority {
        AuditAuthority {
            profile: Some(profile.clone()),
            work_context: Some(self.authority.work_context.clone()),
            policy_revision: Some(self.authority.policy_revision.clone()),
            scopes: self.actor.scopes.clone(),
            data_labels: self.actor.data_labels.clone(),
        }
    }
    pub fn audit_draft(
        &self,
        profile: &GatewayProfileId,
        target: AuditTarget,
        detail: AuditDetail,
        outcome: AuditOutcome,
        reason: AuditReason,
    ) -> Result<AuditDraft> {
        Ok(
            AuditDraft::builder(self.audit.clone(), target, detail, outcome, reason)
                .actor(self.audit_actor()?)
                .authority(self.audit_authority(profile))
                .build()?,
        )
    }
}
/// MCP projections use typed resource addresses. Domain-only policy targets are
/// recorded by their producer with its own target, never erased to a gateway label.
pub fn mcp_audit_target(target: &PolicyTarget) -> Result<AuditTarget> {
    Ok(match target {
        PolicyTarget::Gateway => AuditTarget::Installation,
        PolicyTarget::Server { server } => AuditTarget::Server {
            server: server.clone(),
        },
        PolicyTarget::Tool { server, tool } => AuditTarget::Tool {
            server: server.clone(),
            tool: tool.clone(),
        },
        PolicyTarget::Resource { server, uri } => AuditTarget::Resource {
            server: server.clone(),
            uri: uri.clone(),
        },
        PolicyTarget::ResourceTemplate { server, uri } => AuditTarget::ResourceTemplate {
            server: server.clone(),
            uri: uri.clone(),
        },
        PolicyTarget::Prompt { server, prompt } => AuditTarget::Prompt {
            server: server.clone(),
            prompt: prompt.clone(),
        },
        PolicyTarget::Task { server, task_id } => AuditTarget::TaskRoute {
            server: server.clone(),
            route: task_id.clone(),
        },
        PolicyTarget::PlatformTask { task_id, .. } => AuditTarget::Task { task: *task_id },
        PolicyTarget::Artifact {
            server,
            artifact_uri,
        } => AuditTarget::Resource {
            server: server.clone(),
            uri: artifact_uri.clone(),
        },
        PolicyTarget::Usage { server, usage_uri } => AuditTarget::Resource {
            server: server.clone(),
            uri: usage_uri.clone(),
        },
        PolicyTarget::RecordingProducer { producer } => {
            recording_ingest_audit_target(producer, None)?
        }
        PolicyTarget::RecordingStream {
            producer,
            stream_id,
        } => recording_ingest_audit_target(producer, Some(stream_id))?,
    })
}
impl GatewayState {
    pub fn set_audit_health(&self, health: veoveo_audit::AuditHealth) -> Result<()> {
        self.audit_health
            .set(health)
            .map_err(|_| anyhow::anyhow!("audit service already registered"))
    }
    pub fn audit_ready(&self) -> bool {
        self.audit_health
            .get()
            .is_some_and(veoveo_audit::AuditHealth::ready)
            && self
                .audit_writer
                .get()
                .is_some_and(veoveo_audit::AuditWriter::is_running)
    }
    pub async fn audit_writer(&self) -> &veoveo_audit::AuditWriter {
        self.audit_writer
            .get_or_init(|| async { veoveo_audit::AuditWriter::start(self.platform.clone()) })
            .await
    }
    pub async fn record_audit(&self, draft: AuditDraft) -> Result<()> {
        let _timer = StageTimer::start(RequestStage::Audit);
        self.audit_writer()
            .await
            .record(draft)
            .await
            .context("required audit commit failed")
    }
    pub async fn record_indexing_audit(
        &self,
        read: veoveo_audit_contract::IndexingRead,
    ) -> Result<()> {
        let _timer = StageTimer::start(RequestStage::Audit);
        self.audit_writer()
            .await
            .record_indexing(read)
            .await
            .context("required indexing audit commit failed")
    }
}

pub fn policy_reason(reason: veoveo_mcp_contract::PolicyReasonCode) -> AuditReason {
    use veoveo_mcp_contract::PolicyReasonCode;
    match reason {
        PolicyReasonCode::PolicyAllow => AuditReason::Accepted,
        PolicyReasonCode::PolicyDeny => AuditReason::PolicyDenied,
        PolicyReasonCode::UnknownProfile => AuditReason::UnknownProfile,
        PolicyReasonCode::UnknownServer => AuditReason::UnknownServer,
        PolicyReasonCode::UnknownTool => AuditReason::UnknownTool,
        PolicyReasonCode::UnknownResource => AuditReason::UnknownResource,
        PolicyReasonCode::UnknownPrompt => AuditReason::UnknownPrompt,
        PolicyReasonCode::UnknownTask => AuditReason::UnknownTask,
        PolicyReasonCode::UnknownArtifact => AuditReason::UnknownArtifact,
        PolicyReasonCode::UnknownPrincipal => AuditReason::UnknownPrincipal,
        PolicyReasonCode::UnknownScope => AuditReason::UnknownScope,
        PolicyReasonCode::UnknownDataLabel => AuditReason::UnknownDataLabel,
        PolicyReasonCode::UnknownTenant => AuditReason::UnknownTenant,
        PolicyReasonCode::UnknownTokenIssuer => AuditReason::UnknownTokenIssuer,
        PolicyReasonCode::MissingPrincipal => AuditReason::MissingPrincipal,
        PolicyReasonCode::MissingTenant => AuditReason::MissingTenant,
        PolicyReasonCode::MissingGroup => AuditReason::MissingGroup,
        PolicyReasonCode::MissingRole => AuditReason::MissingRole,
        PolicyReasonCode::MissingScope => AuditReason::MissingScope,
        PolicyReasonCode::MissingDataLabel => AuditReason::MissingDataLabel,
        PolicyReasonCode::MissingPrincipalAssurance => AuditReason::MissingPrincipalAssurance,
        PolicyReasonCode::TokenAudienceMismatch => AuditReason::TokenAudienceMismatch,
        PolicyReasonCode::TokenExpired => AuditReason::ExpiredCredential,
        PolicyReasonCode::TokenNotYetValid => AuditReason::TokenNotYetValid,
        PolicyReasonCode::ReplayDetected => AuditReason::Replay,
    }
}

impl GatewayState {
    pub async fn record_policy_admission(
        &self,
        subject: &AuthenticatedSubject,
        profile: &GatewayProfileId,
        target: &PolicyTarget,
        detail: AuditDetail,
        decision: &veoveo_mcp_contract::PolicyDecision,
    ) -> Result<()> {
        let allowed = decision.effect == veoveo_mcp_contract::PolicyEffect::Allow;
        let draft = subject.audit_draft(
            profile,
            mcp_audit_target(target)?,
            detail,
            if allowed {
                AuditOutcome::Allowed
            } else {
                AuditOutcome::Denied
            },
            policy_reason(decision.reason),
        )?;
        self.record_audit(draft).await
    }
}

/// Private resource identity for the gateway-owned producer protocol, not an MCP read route.
pub fn recording_ingest_audit_target(
    producer: &veoveo_mcp_contract::RecordingProducerId,
    stream: Option<&veoveo_mcp_contract::RecordingIngestStreamId>,
) -> Result<AuditTarget> {
    use veoveo_types::{ResourceUriBuilder, ServerSlug, UriSegment};
    let mut uri = ResourceUriBuilder::new("recording-ingest://producers")?
        .segment(UriSegment::new(producer.as_str())?);
    if let Some(stream) = stream {
        uri = uri
            .segment(UriSegment::new("streams")?)
            .segment(UriSegment::new(stream.as_str())?);
    }
    Ok(AuditTarget::Resource {
        server: ServerSlug::new("recording-hub")?,
        uri: uri.build()?,
    })
}
