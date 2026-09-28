//! Read-only DTOs for the deployed unversioned policy-event envelope.
//! URI fields stay textual here so future concrete validation cannot reinterpret
//! a historical completion declaration before its action has been inspected.
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;
use veoveo_mcp_contract::{
    AuditEvent, CanonicalTaskId, GatewayAction, GatewayProfileId, LocalToolName, McpMethodName,
    PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyRuleId, PolicyTarget,
    PrincipalAuditAttributes, PromptName, RecordingIngestStreamId, RecordingProducerId, ServerSlug,
    TokenIssuer, TraceId,
};
use veoveo_types::{PolicyVersion, PrincipalId, ResourceTemplateUri, ResourceUri, TenantId};

#[derive(Deserialize)]
pub(super) struct Event {
    event_id: TraceId,
    timestamp: DateTime<Utc>,
    trace_id: TraceId,
    profile: GatewayProfileId,
    method: McpMethodName,
    action: GatewayAction,
    target: Target,
    decision: Decision,
    principal: Option<PrincipalId>,
    principal_attributes: Option<PrincipalAuditAttributes>,
    tenant: Option<TenantId>,
    token_issuer: Option<TokenIssuer>,
    latency_ms: Option<u64>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Decision {
    effect: PolicyEffect,
    reason: PolicyReasonCode,
    evaluated_at: DateTime<Utc>,
    profile: GatewayProfileId,
    action: GatewayAction,
    target: Target,
    principal: Option<PrincipalId>,
    tenant: Option<TenantId>,
    policy_version: Option<PolicyVersion>,
    rule_id: Option<PolicyRuleId>,
    trace_id: TraceId,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum Target {
    Gateway,
    Server {
        server: ServerSlug,
    },
    Tool {
        server: ServerSlug,
        tool: LocalToolName,
    },
    Resource {
        server: ServerSlug,
        uri: String,
    },
    Prompt {
        server: ServerSlug,
        prompt: PromptName,
    },
    Task {
        server: ServerSlug,
        task_id: CanonicalTaskId,
    },
    Artifact {
        server: ServerSlug,
        artifact_uri: String,
    },
    Usage {
        server: ServerSlug,
        usage_uri: String,
    },
    RecordingProducer {
        producer: RecordingProducerId,
    },
    RecordingStream {
        producer: RecordingProducerId,
        stream_id: RecordingIngestStreamId,
    },
}

impl Event {
    pub(super) fn into_current(self) -> Result<AuditEvent> {
        Ok(AuditEvent {
            event_id: self.event_id,
            timestamp: self.timestamp,
            trace_id: self.trace_id,
            profile: self.profile,
            method: self.method,
            action: self.action,
            target: self.target.into_current(self.action)?,
            decision: self.decision.into_current()?,
            principal: self.principal,
            principal_attributes: self.principal_attributes,
            tenant: self.tenant,
            token_issuer: self.token_issuer,
            latency_ms: self.latency_ms,
            metadata: self.metadata,
        })
    }
}

impl Decision {
    fn into_current(self) -> Result<PolicyDecision> {
        Ok(PolicyDecision {
            effect: self.effect,
            reason: self.reason,
            evaluated_at: self.evaluated_at,
            profile: self.profile,
            action: self.action,
            target: self.target.into_current(self.action)?,
            principal: self.principal,
            tenant: self.tenant,
            policy_version: self.policy_version,
            rule_id: self.rule_id,
            trace_id: self.trace_id,
        })
    }
}

impl Target {
    fn into_current(self, action: GatewayAction) -> Result<PolicyTarget> {
        Ok(match self {
            Self::Gateway => PolicyTarget::Gateway,
            Self::Server { server } => PolicyTarget::Server { server },
            Self::Tool { server, tool } => PolicyTarget::Tool { server, tool },
            Self::Resource { server, uri }
                if matches!(
                    action,
                    GatewayAction::CompletionComplete | GatewayAction::ResourcesTemplatesList
                ) =>
            {
                template(server, uri)?
            }
            Self::Resource { server, uri } => PolicyTarget::Resource {
                server,
                uri: resource(uri)?,
            },
            Self::Prompt { server, prompt } => PolicyTarget::Prompt { server, prompt },
            Self::Task { server, task_id } => PolicyTarget::Task { server, task_id },
            Self::Artifact {
                server,
                artifact_uri,
            } if action == GatewayAction::ResourcesTemplatesList => template(server, artifact_uri)?,
            Self::Artifact {
                server,
                artifact_uri,
            } => PolicyTarget::Artifact {
                server,
                artifact_uri: resource(artifact_uri)?,
            },
            Self::Usage { server, usage_uri }
                if action == GatewayAction::ResourcesTemplatesList =>
            {
                template(server, usage_uri)?
            }
            Self::Usage { server, usage_uri } => PolicyTarget::Usage {
                server,
                usage_uri: resource(usage_uri)?,
            },
            Self::RecordingProducer { producer } => PolicyTarget::RecordingProducer { producer },
            Self::RecordingStream {
                producer,
                stream_id,
            } => PolicyTarget::RecordingStream {
                producer,
                stream_id,
            },
        })
    }
}

fn template(server: ServerSlug, uri: String) -> Result<PolicyTarget> {
    let uri = ResourceTemplateUri::new(uri).map_err(|_| anyhow!(
        "gateway policy audit v1 contains an invalid resource template; retain the record and use compatible inspection tooling"
    ))?;
    Ok(PolicyTarget::ResourceTemplate { server, uri })
}

fn resource(uri: String) -> Result<ResourceUri> {
    ResourceUri::new(uri).map_err(|_| anyhow!(
        "gateway policy audit v1 contains an invalid resource address; retain the record and use compatible inspection tooling"
    ))
}
