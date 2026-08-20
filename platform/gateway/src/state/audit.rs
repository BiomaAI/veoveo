use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;
use veoveo_mcp_contract::{
    AuditEvent, AuthAuditEvent, AuthMethod, AuthOutcome, AuthReasonCode, GatewayProfileId,
    LocalToolName, McpMethodName, PolicyEffect, PolicyReasonCode, PrincipalAuditAttributes,
    PrincipalId, ServerSlug, TenantId, TokenIssuer, TraceId,
};
use veoveo_platform_store::{
    AuditEventId, AuditEventRecord, AuditOutcome, GatewayAuditKind, OpenObject,
    deterministic_principal_id, deterministic_tenant_id,
};

use super::GatewayState;

const GATEWAY_AUDIT_NAMESPACE: Uuid = Uuid::from_u128(0xf9c572b4_0662_5bfb_9e61_32759dfdf997);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GatewayAuditCounts {
    pub auth_events: u64,
    pub policy_events: u64,
    pub tool_call_events: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayToolCallResultKind {
    Complete,
    ErrorResult,
    InputRequired,
    TaskCreated,
    OtherResponse,
    ProtocolError,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GatewayToolCallAuditEvent {
    pub event_id: TraceId,
    pub timestamp: DateTime<Utc>,
    pub trace_id: TraceId,
    pub profile: GatewayProfileId,
    pub server: ServerSlug,
    pub tool: LocalToolName,
    pub result_kind: GatewayToolCallResultKind,
    pub principal: PrincipalId,
    pub principal_attributes: PrincipalAuditAttributes,
    pub tenant: Option<TenantId>,
    pub token_issuer: TokenIssuer,
    pub latency_ms: u64,
    pub mcp_error_code: Option<i32>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayAuthAuditMethodSummary {
    pub method: AuthMethod,
    pub allow_events: u64,
    pub deny_events: u64,
    pub total_events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayAuthAuditReasonSummary {
    pub reason: AuthReasonCode,
    pub events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayAuthAuditMetadataSummary {
    pub metadata_key: String,
    pub metadata_value: String,
    pub events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayPolicyAuditMethodSummary {
    pub method: McpMethodName,
    pub allow_events: u64,
    pub deny_events: u64,
    pub total_events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayPolicyAuditReasonSummary {
    pub reason: PolicyReasonCode,
    pub events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayPolicyAuditMetadataSummary {
    pub metadata_key: String,
    pub metadata_value: String,
    pub events: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct GatewayAuditRetentionSummary {
    pub auth_events_deleted: u64,
    pub policy_events_deleted: u64,
    pub tool_call_events_deleted: u64,
}

impl GatewayState {
    pub async fn audit_counts(&self) -> Result<GatewayAuditCounts> {
        let (auth_events, policy_events, tool_call_events) = tokio::try_join!(
            self.platform
                .gateway_audit_event_count(GatewayAuditKind::Auth),
            self.platform
                .gateway_audit_event_count(GatewayAuditKind::Policy),
            self.platform
                .gateway_audit_event_count(GatewayAuditKind::ToolCall),
        )
        .context("failed to count canonical gateway audit events")?;
        Ok(GatewayAuditCounts {
            auth_events,
            policy_events,
            tool_call_events,
        })
    }

    pub async fn auth_audit_method_summary(&self) -> Result<Vec<GatewayAuthAuditMethodSummary>> {
        let mut summaries = BTreeMap::<AuthMethod, GatewayAuthAuditMethodSummary>::new();
        for event in self.auth_audit_events().await? {
            let entry = summaries
                .entry(event.method)
                .or_insert(GatewayAuthAuditMethodSummary {
                    method: event.method,
                    allow_events: 0,
                    deny_events: 0,
                    total_events: 0,
                });
            match event.outcome {
                AuthOutcome::Allow => entry.allow_events += 1,
                AuthOutcome::Deny => entry.deny_events += 1,
            }
            entry.total_events += 1;
        }
        Ok(summaries.into_values().collect())
    }

    pub async fn auth_audit_reason_summary(&self) -> Result<Vec<GatewayAuthAuditReasonSummary>> {
        let mut summaries = BTreeMap::<AuthReasonCode, GatewayAuthAuditReasonSummary>::new();
        for event in self.auth_audit_events().await? {
            let entry = summaries
                .entry(event.reason)
                .or_insert(GatewayAuthAuditReasonSummary {
                    reason: event.reason,
                    events: 0,
                });
            entry.events += 1;
        }
        Ok(summaries.into_values().collect())
    }

    pub async fn auth_audit_metadata_summary(
        &self,
        metadata_key: &str,
    ) -> Result<Vec<GatewayAuthAuditMetadataSummary>> {
        let mut summaries = BTreeMap::<String, u64>::new();
        for event in self.auth_audit_events().await? {
            if let Some(metadata_value) = event.metadata.get(metadata_key) {
                *summaries.entry(metadata_value.clone()).or_default() += 1;
            }
        }
        Ok(summaries
            .into_iter()
            .map(|(metadata_value, events)| GatewayAuthAuditMetadataSummary {
                metadata_key: metadata_key.to_owned(),
                metadata_value,
                events,
            })
            .collect())
    }

    pub async fn policy_audit_method_summary(
        &self,
    ) -> Result<Vec<GatewayPolicyAuditMethodSummary>> {
        let mut summaries = BTreeMap::<McpMethodName, GatewayPolicyAuditMethodSummary>::new();
        for event in self.policy_audit_events().await? {
            let entry = summaries.entry(event.method.clone()).or_insert_with(|| {
                GatewayPolicyAuditMethodSummary {
                    method: event.method.clone(),
                    allow_events: 0,
                    deny_events: 0,
                    total_events: 0,
                }
            });
            match event.decision.effect {
                PolicyEffect::Allow => entry.allow_events += 1,
                PolicyEffect::Deny => entry.deny_events += 1,
            }
            entry.total_events += 1;
        }
        Ok(summaries.into_values().collect())
    }

    pub async fn policy_audit_reason_summary(
        &self,
    ) -> Result<Vec<GatewayPolicyAuditReasonSummary>> {
        let mut summaries = BTreeMap::<PolicyReasonCode, GatewayPolicyAuditReasonSummary>::new();
        for event in self.policy_audit_events().await? {
            let entry =
                summaries
                    .entry(event.decision.reason)
                    .or_insert(GatewayPolicyAuditReasonSummary {
                        reason: event.decision.reason,
                        events: 0,
                    });
            entry.events += 1;
        }
        Ok(summaries.into_values().collect())
    }

    pub async fn policy_audit_metadata_summary(
        &self,
        metadata_key: &str,
    ) -> Result<Vec<GatewayPolicyAuditMetadataSummary>> {
        let mut summaries = BTreeMap::<String, u64>::new();
        for event in self.policy_audit_events().await? {
            if let Some(metadata_value) = event.metadata.get(metadata_key) {
                *summaries.entry(metadata_value.clone()).or_default() += 1;
            }
        }
        Ok(summaries
            .into_iter()
            .map(
                |(metadata_value, events)| GatewayPolicyAuditMetadataSummary {
                    metadata_key: metadata_key.to_owned(),
                    metadata_value,
                    events,
                },
            )
            .collect())
    }

    pub async fn delete_audit_events_before(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<GatewayAuditRetentionSummary> {
        let (auth_events_deleted, policy_events_deleted, tool_call_events_deleted) =
            tokio::try_join!(
                self.platform
                    .delete_gateway_audit_events_before(GatewayAuditKind::Auth, cutoff),
                self.platform
                    .delete_gateway_audit_events_before(GatewayAuditKind::Policy, cutoff),
                self.platform
                    .delete_gateway_audit_events_before(GatewayAuditKind::ToolCall, cutoff),
            )
            .context("failed to apply gateway audit retention")?;
        Ok(GatewayAuditRetentionSummary {
            auth_events_deleted,
            policy_events_deleted,
            tool_call_events_deleted,
        })
    }

    pub async fn record_audit_event(&self, event: &AuditEvent) -> Result<()> {
        let record = canonical_policy_record(event)?;
        self.platform
            .record_gateway_audit_event(GatewayAuditKind::Policy, record)
            .await
            .context("failed to record canonical gateway policy audit event")
    }

    pub async fn record_auth_audit_event(&self, event: &AuthAuditEvent) -> Result<()> {
        let record = canonical_auth_record(event)?;
        self.platform
            .record_gateway_audit_event(GatewayAuditKind::Auth, record)
            .await
            .context("failed to record canonical gateway authentication audit event")
    }

    pub async fn record_tool_call_audit_event(
        &self,
        event: &GatewayToolCallAuditEvent,
    ) -> Result<()> {
        let record = canonical_tool_call_record(event)?;
        self.platform
            .record_gateway_audit_event(GatewayAuditKind::ToolCall, record)
            .await
            .context("failed to record canonical gateway tool-call audit event")
    }

    async fn policy_audit_events(&self) -> Result<Vec<AuditEvent>> {
        decode_audit_events(
            self.platform
                .gateway_audit_events(GatewayAuditKind::Policy)
                .await
                .context("failed to read canonical gateway policy audit events")?,
        )
    }

    async fn auth_audit_events(&self) -> Result<Vec<AuthAuditEvent>> {
        decode_audit_events(
            self.platform
                .gateway_audit_events(GatewayAuditKind::Auth)
                .await
                .context("failed to read canonical gateway authentication audit events")?,
        )
    }
}

fn canonical_policy_record(event: &AuditEvent) -> Result<AuditEventRecord> {
    let action = enum_wire_value(event.action)?;
    canonical_audit_record(
        GatewayAuditKind::Policy,
        event.event_id.as_str(),
        event.timestamp,
        event.trace_id.as_str(),
        event.profile.as_str(),
        event.principal.as_ref().map(|value| value.as_str()),
        event.tenant.as_ref().map(|value| value.as_str()),
        &action,
        match event.decision.effect {
            PolicyEffect::Allow => AuditOutcome::Allowed,
            PolicyEffect::Deny => AuditOutcome::Denied,
        },
        event,
    )
}

pub(super) fn canonical_auth_record(event: &AuthAuditEvent) -> Result<AuditEventRecord> {
    canonical_audit_record(
        GatewayAuditKind::Auth,
        event.event_id.as_str(),
        event.timestamp,
        event.trace_id.as_str(),
        event.protected_resource.as_str(),
        event.principal.as_ref().map(|value| value.as_str()),
        event.tenant.as_ref().map(|value| value.as_str()),
        event.method.as_str(),
        match event.outcome {
            AuthOutcome::Allow => AuditOutcome::Allowed,
            AuthOutcome::Deny => AuditOutcome::Denied,
        },
        event,
    )
}

fn canonical_tool_call_record(event: &GatewayToolCallAuditEvent) -> Result<AuditEventRecord> {
    let outcome = match event.result_kind {
        GatewayToolCallResultKind::Complete
        | GatewayToolCallResultKind::InputRequired
        | GatewayToolCallResultKind::TaskCreated
        | GatewayToolCallResultKind::OtherResponse => AuditOutcome::Succeeded,
        GatewayToolCallResultKind::ErrorResult | GatewayToolCallResultKind::ProtocolError => {
            AuditOutcome::Failed
        }
    };
    canonical_audit_record(
        GatewayAuditKind::ToolCall,
        event.event_id.as_str(),
        event.timestamp,
        event.trace_id.as_str(),
        &format!("{}:{}", event.server, event.tool),
        Some(event.principal.as_str()),
        event.tenant.as_ref().map(|value| value.as_str()),
        "tools_call",
        outcome,
        event,
    )
}

#[allow(clippy::too_many_arguments)]
fn canonical_audit_record(
    kind: GatewayAuditKind,
    event_id: &str,
    timestamp: DateTime<Utc>,
    trace_id: &str,
    resource_id: &str,
    principal: Option<&str>,
    tenant: Option<&str>,
    action: &str,
    outcome: AuditOutcome,
    event: impl Serialize,
) -> Result<AuditEventRecord> {
    let tenant_id = tenant
        .map(deterministic_tenant_id)
        .transpose()
        .context("invalid gateway audit tenant identity")?;
    let actor = tenant
        .zip(principal)
        .map(|(tenant, principal)| deterministic_principal_id(tenant, principal))
        .transpose()
        .context("invalid gateway audit actor identity")?;
    let event_value = serde_json::to_value(event)?;
    let record_id = AuditEventId::from_uuid(Uuid::new_v5(
        &GATEWAY_AUDIT_NAMESPACE,
        format!("{}:{event_id}", kind.resource_type()).as_bytes(),
    ));
    Ok(AuditEventRecord {
        id: record_id.record_id(),
        tenant: tenant_id.map(|id| id.record_id()),
        actor: actor.map(|id| id.record_id()),
        action: action.to_owned(),
        resource_type: kind.resource_type().to_owned(),
        resource_id: Some(resource_id.to_owned()),
        outcome,
        request_id: Some(event_id.to_owned()),
        trace_id: Some(trace_id.to_owned()),
        source_ip: None,
        details: OpenObject::new(BTreeMap::from([
            (
                "gateway_kind".into(),
                serde_json::json!(kind.resource_type()),
            ),
            ("event".into(), event_value),
        ])),
        occurred_at: timestamp,
        search_text: format!("{} {action} {resource_id}", kind.resource_type()),
    })
}

fn decode_audit_events<T: serde::de::DeserializeOwned>(
    records: Vec<AuditEventRecord>,
) -> Result<Vec<T>> {
    records
        .into_iter()
        .map(|record| {
            let event = record
                .details
                .as_map()
                .get("event")
                .cloned()
                .context("canonical gateway audit record is missing its typed event")?;
            serde_json::from_value(event).context("canonical gateway audit event is invalid")
        })
        .collect()
}

fn enum_wire_value(value: impl Serialize) -> Result<String> {
    let value = serde_json::to_value(value)?;
    value
        .as_str()
        .map(ToOwned::to_owned)
        .context("gateway enum did not serialize as a string")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use veoveo_mcp_contract::PrincipalKind;

    use super::*;

    fn tool_call_event(result_kind: GatewayToolCallResultKind) -> GatewayToolCallAuditEvent {
        GatewayToolCallAuditEvent {
            event_id: TraceId::new("019ff733-07c2-7ad0-a54d-530ca1b5475e").unwrap(),
            timestamp: Utc::now(),
            trace_id: TraceId::new("019ff733-0d99-7cb2-96e2-4b9381970048").unwrap(),
            profile: GatewayProfileId::new("operator").unwrap(),
            server: ServerSlug::new("view").unwrap(),
            tool: LocalToolName::new("create_scene_composition").unwrap(),
            result_kind,
            principal: PrincipalId::new("user-1").unwrap(),
            principal_attributes: PrincipalAuditAttributes {
                kind: PrincipalKind::User,
                groups: BTreeSet::new(),
                roles: BTreeSet::new(),
                scopes: BTreeSet::new(),
                data_labels: BTreeSet::new(),
                assurances: BTreeSet::new(),
                authenticated_at: None,
            },
            tenant: Some(TenantId::new("tenant-1").unwrap()),
            token_issuer: TokenIssuer::new("https://issuer.example").unwrap(),
            latency_ms: 42,
            mcp_error_code: None,
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn tool_call_records_separate_completion_from_policy_admission() {
        let succeeded =
            canonical_tool_call_record(&tool_call_event(GatewayToolCallResultKind::Complete))
                .unwrap();
        assert_eq!(succeeded.resource_type, "gateway_tool_call");
        assert_eq!(
            succeeded.resource_id.as_deref(),
            Some("view:create_scene_composition")
        );
        assert_eq!(succeeded.action, "tools_call");
        assert_eq!(succeeded.outcome, AuditOutcome::Succeeded);

        let mut failed = tool_call_event(GatewayToolCallResultKind::ProtocolError);
        failed.mcp_error_code = Some(-32602);
        let failed = canonical_tool_call_record(&failed).unwrap();
        assert_eq!(failed.outcome, AuditOutcome::Failed);
        assert_eq!(
            failed
                .details
                .as_map()
                .get("event")
                .and_then(|event| event.get("mcp_error_code")),
            Some(&serde_json::json!(-32602))
        );
    }
}
