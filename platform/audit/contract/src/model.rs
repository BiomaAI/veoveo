use crate::*;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, net::IpAddr, num::NonZeroU64};
use veoveo_types::{
    DataLabelId, GatewayProfileId, GatewayRefreshFamilyId, LocalToolName, OAuthClientId,
    PolicyVersion, PrincipalId, PromptName, ResourceTemplateUri, ResourceUri, ScopeName,
    ServerSlug, TaskId, TenantId, WorkContextId,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "tenant",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AuditPartition {
    Installation,
    Tenant(TenantId),
}
impl AuditPartition {
    pub fn storage_key(&self) -> String {
        match self {
            Self::Installation => "installation".into(),
            Self::Tenant(id) => format!("tenant:{id}"),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuditPrincipalKind {
    User,
    Service,
    Capability,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditManagedExecution {
    pub instance: veoveo_types::AgentManagedInstanceId,
    pub generation: NonZeroU64,
    pub dispatch_epoch: NonZeroU64,
    pub episode: Option<AuditEpisodeId>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditActor {
    pub principal: PrincipalId,
    pub kind: AuditPrincipalKind,
    pub tenant: Option<TenantId>,
    pub oauth_client: Option<OAuthClientId>,
    pub session_family: Option<GatewayRefreshFamilyId>,
    pub delegating_principal: Option<PrincipalId>,
    pub managed_agent: Option<AuditManagedExecution>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditAuthority {
    pub profile: Option<GatewayProfileId>,
    pub work_context: Option<WorkContextId>,
    pub policy_revision: Option<PolicyVersion>,
    pub scopes: BTreeSet<ScopeName>,
    pub data_labels: BTreeSet<DataLabelId>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuditTarget {
    PlatformResource {
        uri: ResourceUri,
    },
    Server {
        server: ServerSlug,
    },
    Artifact {
        artifact: veoveo_artifact_contract::ArtifactId,
    },
    Computer {
        computer: veoveo_computers_contract::ComputerId,
    },
    Task {
        task: TaskId,
    },
    TaskRoute {
        server: ServerSlug,
        route: veoveo_types::CanonicalTaskId,
    },
    Principal {
        tenant: TenantId,
        principal: PrincipalId,
    },
    WorkContext {
        tenant: TenantId,
        context: WorkContextId,
    },
    Tool {
        server: ServerSlug,
        tool: LocalToolName,
    },
    Resource {
        server: ServerSlug,
        uri: ResourceUri,
    },
    ResourceTemplate {
        server: ServerSlug,
        uri: ResourceTemplateUri,
    },
    Prompt {
        server: ServerSlug,
        prompt: PromptName,
    },
    Discovery {
        server: Option<ServerSlug>,
        collection: DiscoveryKind,
    },
    Profile {
        profile: GatewayProfileId,
    },
    Client {
        client: OAuthClientId,
    },
    AuditLog {
        partition: AuditPartition,
    },
    Installation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditRequest {
    pub id: AuditRequestId,
    pub trace_id: AuditTraceId,
    pub span_id: AuditSpanId,
    pub source_ip: Option<IpAddr>,
}
/// Verified attribution retained by a domain operation for its lifecycle records.
/// Producers construct this from their authenticated protocol context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditContext {
    pub actor: AuditActor,
    pub authority: AuditAuthority,
    pub request: AuditRequest,
}
impl AuditContext {
    pub fn draft(
        &self,
        target: AuditTarget,
        detail: AuditDetail,
        outcome: AuditOutcome,
        reason: AuditReason,
    ) -> Result<AuditDraft, AuditValidationError> {
        AuditDraft::builder(self.request.clone(), target, detail, outcome, reason)
            .actor(self.actor.clone())
            .authority(self.authority.clone())
            .build()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AuditSchema {
    #[serde(rename = "veoveo.ai/audit-record/v1")]
    V1,
}

/// Immutable constructor-checked action. The database supplies recorded_at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "AuditDraftWire", into = "AuditDraftWire")]
pub struct AuditDraft(AuditDraftWire);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AuditDraftWire {
    id: AuditRecordId,
    schema: AuditSchema,
    partition: AuditPartition,
    actor: Option<AuditActor>,
    authority: AuditAuthority,
    target: AuditTarget,
    detail: AuditDetail,
    outcome: AuditOutcome,
    reason: AuditReason,
    request: AuditRequest,
    occurred_at: DateTime<Utc>,
    latency_ms: Option<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuditValidationError {
    #[error("audit detail and target do not describe the same action")]
    Target,
    #[error("audit counters must fit the exact I-JSON integer range")]
    CounterRange,
    #[error("audit partition must match the authenticated actor's tenant")]
    Partition,
    #[error("an accepted outcome cannot carry a denial reason")]
    Outcome,
    #[error("audit cursor belongs to a different partition")]
    Cursor,
    #[error("audit time range must increase")]
    TimeRange,
    #[error("daily audit bounds and cursor must use midnight UTC")]
    DayBoundary,
    #[error("audit page size must be between 1 and 1000")]
    PageSize,
}
impl TryFrom<AuditDraftWire> for AuditDraft {
    type Error = AuditValidationError;
    fn try_from(wire: AuditDraftWire) -> Result<Self, Self::Error> {
        let partition = wire
            .actor
            .as_ref()
            .and_then(|a| a.tenant.clone())
            .map_or(AuditPartition::Installation, AuditPartition::Tenant);
        if partition != wire.partition {
            return Err(AuditValidationError::Partition);
        }
        let accepted = matches!(
            wire.outcome,
            AuditOutcome::Allowed | AuditOutcome::Succeeded
        );
        if accepted != (wire.reason == AuditReason::Accepted) {
            return Err(AuditValidationError::Outcome);
        }
        let target_matches = match (&wire.detail, &wire.target) {
            (
                AuditDetail::ToolAdmission | AuditDetail::ToolCompletion { .. },
                AuditTarget::Tool { .. },
            ) => true,
            (AuditDetail::ToolAdmission | AuditDetail::ToolCompletion { .. }, _) => false,
            (
                AuditDetail::Discovery { collection, .. },
                AuditTarget::Discovery {
                    collection: target, ..
                },
            ) => collection == target,
            (AuditDetail::Discovery { .. }, _) => false,
            (
                AuditDetail::Task { .. },
                AuditTarget::Task { .. } | AuditTarget::TaskRoute { .. },
            ) => true,
            (AuditDetail::Task { .. }, _) => false,
            _ => true,
        };
        if !target_matches {
            return Err(AuditValidationError::Target);
        }
        const MAX_COUNTER: u64 = (1u64 << 53) - 1;
        let counters_ok = wire.latency_ms.is_none_or(|value| value <= MAX_COUNTER)
            && wire
                .actor
                .as_ref()
                .and_then(|actor| actor.managed_agent.as_ref())
                .is_none_or(|agent| {
                    agent.generation.get() <= MAX_COUNTER
                        && agent.dispatch_epoch.get() <= MAX_COUNTER
                })
            && match &wire.detail {
                AuditDetail::ToolCompletion { duration_ms, .. } => *duration_ms <= MAX_COUNTER,
                AuditDetail::Artifact { bytes, .. } => {
                    bytes.is_none_or(|bytes| bytes <= MAX_COUNTER)
                }
                AuditDetail::DictationSummary {
                    chunks,
                    duration_ms,
                    ..
                } => *chunks <= MAX_COUNTER && *duration_ms <= MAX_COUNTER,
                AuditDetail::IndexingWindow { reads, denials, .. } => {
                    *reads <= MAX_COUNTER && *denials <= MAX_COUNTER
                }
                _ => true,
            };
        if !counters_ok {
            return Err(AuditValidationError::CounterRange);
        }
        Ok(Self(wire))
    }
}
impl From<AuditDraft> for AuditDraftWire {
    fn from(draft: AuditDraft) -> Self {
        draft.0
    }
}
impl AuditDraft {
    pub fn builder(
        request: AuditRequest,
        target: AuditTarget,
        detail: AuditDetail,
        outcome: AuditOutcome,
        reason: AuditReason,
    ) -> AuditDraftBuilder {
        AuditDraftBuilder(AuditDraftWire {
            id: AuditRecordId::new(),
            schema: AuditSchema::V1,
            partition: AuditPartition::Installation,
            actor: None,
            authority: AuditAuthority::default(),
            target,
            detail,
            outcome,
            reason,
            request,
            occurred_at: Utc::now(),
            latency_ms: None,
        })
    }
    pub fn id(&self) -> AuditRecordId {
        self.0.id
    }
    pub fn partition(&self) -> &AuditPartition {
        &self.0.partition
    }
    pub fn actor(&self) -> Option<&AuditActor> {
        self.0.actor.as_ref()
    }
    pub fn authority(&self) -> &AuditAuthority {
        &self.0.authority
    }
    pub fn target(&self) -> &AuditTarget {
        &self.0.target
    }
    pub fn detail(&self) -> &AuditDetail {
        &self.0.detail
    }
    pub fn outcome(&self) -> AuditOutcome {
        self.0.outcome
    }
    pub fn reason(&self) -> AuditReason {
        self.0.reason
    }
    pub fn request(&self) -> &AuditRequest {
        &self.0.request
    }
    pub fn occurred_at(&self) -> DateTime<Utc> {
        self.0.occurred_at
    }
    pub fn latency_ms(&self) -> Option<u64> {
        self.0.latency_ms
    }
}
pub struct AuditDraftBuilder(AuditDraftWire);
impl AuditDraftBuilder {
    pub fn actor(mut self, actor: AuditActor) -> Self {
        self.0.partition = actor
            .tenant
            .clone()
            .map_or(AuditPartition::Installation, AuditPartition::Tenant);
        self.0.actor = Some(actor);
        self
    }
    pub fn authority(mut self, authority: AuditAuthority) -> Self {
        self.0.authority = authority;
        self
    }
    pub fn identity(mut self, id: AuditRecordId) -> Self {
        self.0.id = id;
        self
    }
    pub fn occurred_at(mut self, at: DateTime<Utc>) -> Self {
        self.0.occurred_at = at;
        self
    }
    pub fn latency_ms(mut self, milliseconds: u64) -> Self {
        self.0.latency_ms = Some(milliseconds);
        self
    }
    pub fn build(self) -> Result<AuditDraft, AuditValidationError> {
        self.0.try_into()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditRecord {
    pub draft: AuditDraft,
    pub recorded_at: DateTime<Utc>,
}

impl AuditRequest {
    /// Start a new non-HTTP operation. HTTP ingress uses its admitted trace context.
    pub fn background() -> Self {
        let trace = uuid::Uuid::new_v4().simple().to_string();
        let span = uuid::Uuid::new_v4().simple().to_string()[..16].to_owned();
        Self {
            id: AuditRequestId::new(),
            trace_id: AuditTraceId::parse(trace).expect("UUID trace"),
            span_id: AuditSpanId::parse(span).expect("UUID span"),
            source_ip: None,
        }
    }
}
