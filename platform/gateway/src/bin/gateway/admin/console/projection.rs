use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::{
    Exposure, GatewayControlPlane, OwnedRoutePurpose, ResourceSelector, ServerManifest,
};
use veoveo_mcp_gateway::{GatewayServerHealth, GatewayServerHealthState};
use veoveo_platform_store::{
    AgentRecord, ArtifactBlobRecord, ArtifactGrantEdge, ArtifactOccurrenceRecord, AuditEventRecord,
    PrincipalRecord, RecordId, RecordIdKey, RecordingRecord, SegmentRecord, ShareLinkRecord,
    TaskRecord, WakeRecord,
};

use crate::runtime::AdminState;

pub(crate) const SNAPSHOT_LIMIT: i64 = 200;

pub(crate) struct Projection {
    pub(crate) principals: Vec<PrincipalRecord>,
    pub(crate) tasks: Vec<TaskRecord>,
    pub(crate) artifacts: Vec<ArtifactOccurrenceRecord>,
    pub(crate) blobs: Vec<ArtifactBlobRecord>,
    pub(crate) share_links: Vec<ShareLinkRecord>,
    pub(crate) grants: Vec<ArtifactGrantEdge>,
    pub(crate) agents: Vec<AgentRecord>,
    pub(crate) wakes: Vec<WakeRecord>,
    pub(crate) recordings: Vec<RecordingRecord>,
    pub(crate) segments: Vec<SegmentRecord>,
    pub(crate) audit: Vec<AuditEventRecord>,
}

pub(crate) async fn load_projection(
    state: &AdminState,
    tenant: &RecordId,
) -> anyhow::Result<Projection> {
    let mut response = state
        .control_store
        .platform_store()
        .client()
        .query(
            r#"
            SELECT * FROM principal WHERE tenant = $tenant ORDER BY display_name ASC LIMIT $limit;
            SELECT * FROM task WHERE tenant = $tenant ORDER BY updated_at DESC LIMIT $limit;
            SELECT * FROM artifact_occurrence WHERE tenant = $tenant ORDER BY created_at DESC LIMIT $limit;
            SELECT * FROM artifact_blob WHERE tenant = $tenant LIMIT $limit;
            SELECT * FROM share_link WHERE tenant = $tenant ORDER BY created_at DESC LIMIT $limit;
            SELECT * FROM artifact_grant WHERE in IN (SELECT VALUE id FROM artifact_occurrence WHERE tenant = $tenant) LIMIT $limit;
            SELECT * FROM agent WHERE tenant = $tenant ORDER BY updated_at DESC LIMIT $limit;
            SELECT * FROM wake WHERE tenant = $tenant ORDER BY created_at DESC LIMIT $limit;
            SELECT * FROM recording WHERE tenant = $tenant ORDER BY started_at DESC LIMIT $limit;
            SELECT * FROM segment WHERE tenant = $tenant ORDER BY created_at DESC LIMIT $limit;
            SELECT * FROM audit_event WHERE tenant = $tenant ORDER BY occurred_at DESC LIMIT $limit;
            "#,
        )
        .bind(("tenant", tenant.clone()))
        .bind(("limit", SNAPSHOT_LIMIT))
        .await?
        .check()?;
    Ok(Projection {
        principals: response.take(0)?,
        tasks: response.take(1)?,
        artifacts: response.take(2)?,
        blobs: response.take(3)?,
        share_links: response.take(4)?,
        grants: response.take(5)?,
        agents: response.take(6)?,
        wakes: response.take(7)?,
        recordings: response.take(8)?,
        segments: response.take(9)?,
        audit: response.take(10)?,
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskSummary {
    pub(crate) id: String,
    pub(crate) r#type: String,
    pub(crate) server: String,
    pub(crate) owner: String,
    pub(crate) state: veoveo_platform_store::TaskStatus,
    pub(crate) recovery_class: veoveo_platform_store::RecoveryClass,
    pub(crate) progress: f64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result_artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArtifactSummary {
    pub(crate) id: String,
    pub(crate) filename: String,
    pub(crate) media_type: String,
    pub(crate) byte_length: i64,
    pub(crate) owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) task_id: Option<String>,
    pub(crate) classification: String,
    pub(crate) labels: Vec<String>,
    pub(crate) release_state: veoveo_platform_store::ArtifactReleaseState,
    pub(crate) authorized_grants: usize,
    pub(crate) active_links: usize,
    pub(crate) grants: Vec<ArtifactGrantSummary>,
    pub(crate) share_links: Vec<ArtifactShareLinkSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) retention_expires_at: Option<DateTime<Utc>>,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArtifactGrantSummary {
    pub(crate) subject_kind: veoveo_platform_store::ArtifactGrantSubjectKind,
    pub(crate) subject: String,
    pub(crate) permission: veoveo_platform_store::GrantPermission,
    pub(crate) labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<DateTime<Utc>>,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArtifactShareLinkSummary {
    pub(crate) id: String,
    pub(crate) permission: veoveo_platform_store::GrantPermission,
    pub(crate) expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_downloads: Option<i64>,
    pub(crate) download_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) revoked_at: Option<DateTime<Utc>>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) active: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) profile: String,
    pub(crate) state: veoveo_platform_store::AgentState,
    pub(crate) pending_wakes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_episode_at: Option<DateTime<Utc>>,
    pub(crate) detail: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordingSummary {
    pub(crate) id: String,
    pub(crate) application: String,
    pub(crate) recording_key: String,
    pub(crate) state: veoveo_platform_store::RecordingState,
    pub(crate) segments: usize,
    pub(crate) byte_length: i64,
    pub(crate) started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ended_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) uri_scheme: String,
    pub(crate) transport: &'static str,
    pub(crate) endpoint: String,
    pub(crate) state: GatewayServerHealthState,
    pub(crate) checked_at: DateTime<Utc>,
    pub(crate) capabilities: ServerCapabilitiesSummary,
    pub(crate) tools: Vec<String>,
    pub(crate) compatibility_helpers: Vec<String>,
    pub(crate) resources: Vec<String>,
    pub(crate) prompts: Vec<String>,
    pub(crate) required_scopes: Vec<String>,
    pub(crate) owned_routes: Vec<ServerRouteSummary>,
    pub(crate) profiles: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerCapabilitiesSummary {
    pub(crate) tools: bool,
    pub(crate) resources: bool,
    pub(crate) resource_templates: bool,
    pub(crate) resource_subscriptions: bool,
    pub(crate) prompts: bool,
    pub(crate) completions: bool,
    pub(crate) tasks: bool,
    pub(crate) notifications: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerRouteSummary {
    pub(crate) path: String,
    pub(crate) purpose: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicySummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) revision: usize,
    pub(crate) state: &'static str,
    pub(crate) rules: usize,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuditSummary {
    pub(crate) id: String,
    pub(crate) occurred_at: DateTime<Utc>,
    pub(crate) actor: String,
    pub(crate) action: String,
    pub(crate) resource: String,
    pub(crate) outcome: veoveo_platform_store::AuditOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) trace_id: Option<String>,
}

pub(crate) fn task_summary(
    task: TaskRecord,
    principal_names: &BTreeMap<String, String>,
) -> anyhow::Result<TaskSummary> {
    Ok(TaskSummary {
        id: record_key(&task.id)?,
        r#type: task.task_type,
        server: record_key(&task.server)?,
        owner: display_record(principal_names, &task.owner)?,
        state: task.status,
        recovery_class: task.recovery_class,
        progress: task.progress,
        created_at: task.created_at,
        updated_at: task.updated_at,
        result_artifact_id: task.result_artifact.as_ref().map(record_key).transpose()?,
        message: task
            .error
            .as_ref()
            .and_then(|error| error.as_map().get("message"))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
    })
}

pub(crate) fn artifact_grant_summary(grant: &ArtifactGrantEdge) -> ArtifactGrantSummary {
    ArtifactGrantSummary {
        subject_kind: grant.subject_kind,
        subject: grant.subject_key.clone(),
        permission: grant.permission,
        labels: grant.labels.clone(),
        expires_at: grant.expires_at,
        created_at: grant.created_at,
    }
}

pub(crate) fn share_link_summary(
    link: &ShareLinkRecord,
    now: DateTime<Utc>,
) -> anyhow::Result<ArtifactShareLinkSummary> {
    let active = link.revoked_at.is_none()
        && link.expires_at > now
        && link
            .max_downloads
            .is_none_or(|max| link.download_count < max);
    Ok(ArtifactShareLinkSummary {
        id: record_key(&link.id)?,
        permission: link.permission,
        expires_at: link.expires_at,
        max_downloads: link.max_downloads,
        download_count: link.download_count,
        revoked_at: link.revoked_at,
        created_at: link.created_at,
        active,
    })
}

pub(crate) fn artifact_summary(
    artifact: ArtifactOccurrenceRecord,
    byte_length: i64,
    grants: Vec<ArtifactGrantSummary>,
    share_links: Vec<ArtifactShareLinkSummary>,
    principal_names: &BTreeMap<String, String>,
) -> anyhow::Result<ArtifactSummary> {
    Ok(ArtifactSummary {
        id: record_key(&artifact.id)?,
        filename: artifact.filename.unwrap_or_else(|| "artifact".to_owned()),
        media_type: artifact.media_type,
        byte_length,
        owner: display_record(principal_names, &artifact.owner)?,
        task_id: artifact.task.as_ref().map(record_key).transpose()?,
        classification: artifact.classification,
        labels: artifact.labels,
        release_state: artifact.release_state,
        authorized_grants: grants.len(),
        active_links: share_links.iter().filter(|link| link.active).count(),
        grants,
        share_links,
        retention_expires_at: artifact.retention_expires_at,
        created_at: artifact.created_at,
    })
}

pub(crate) fn agent_summary(
    agent: AgentRecord,
    pending_wakes: usize,
) -> anyhow::Result<AgentSummary> {
    let id = record_key(&agent.id)?;
    let detail = match agent.last_episode.as_ref() {
        Some(episode) => format!("Episode {}", record_key(episode)?),
        None => "No completed episode".to_owned(),
    };
    Ok(AgentSummary {
        id,
        name: agent.display_name,
        profile: record_key(&agent.profile)?,
        state: agent.state,
        pending_wakes,
        last_episode_at: agent.last_episode.as_ref().map(|_| agent.updated_at),
        detail,
    })
}

pub(crate) fn recording_summary(
    recording: RecordingRecord,
    segments: usize,
    byte_length: i64,
) -> anyhow::Result<RecordingSummary> {
    Ok(RecordingSummary {
        id: record_key(&recording.id)?,
        application: recording.application_id,
        recording_key: recording.recording_key,
        state: recording.state,
        segments,
        byte_length,
        started_at: recording.started_at,
        ended_at: recording.ended_at,
    })
}

pub(crate) fn audit_summary(
    event: AuditEventRecord,
    principal_names: &BTreeMap<String, String>,
) -> anyhow::Result<AuditSummary> {
    let actor = event
        .actor
        .as_ref()
        .map(|actor| display_record(principal_names, actor))
        .transpose()?
        .unwrap_or_else(|| "system".to_owned());
    Ok(AuditSummary {
        id: record_key(&event.id)?,
        occurred_at: event.occurred_at,
        actor,
        action: event.action,
        resource: match event.resource_id {
            Some(id) => format!("{}:{id}", event.resource_type),
            None => event.resource_type,
        },
        outcome: event.outcome,
        source_ip: event.source_ip,
        trace_id: event.trace_id,
    })
}

pub(crate) fn server_summary(
    server: &ServerManifest,
    control: &GatewayControlPlane,
    health: Option<&GatewayServerHealth>,
    now: DateTime<Utc>,
) -> ServerSummary {
    let resources = control
        .profiles
        .iter()
        .filter_map(|profile| {
            profile
                .servers
                .iter()
                .find(|item| item.server == server.slug)
        })
        .flat_map(|exposure| match &exposure.resources {
            Exposure::All => vec![format!("{}://**", server.uri_scheme)],
            Exposure::Listed(selectors) => selectors
                .iter()
                .map(resource_selector_label)
                .collect::<Vec<_>>(),
            Exposure::None => Vec::new(),
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    ServerSummary {
        id: server.slug.to_string(),
        name: server.slug.to_string(),
        uri_scheme: server.uri_scheme.to_string(),
        transport: "streamable_http",
        endpoint: server.upstream.url.to_string(),
        state: health.map_or(GatewayServerHealthState::Offline, |health| health.state),
        checked_at: health.map_or(now, |health| health.checked_at),
        capabilities: ServerCapabilitiesSummary {
            tools: server.capabilities.tools,
            resources: server.capabilities.resources,
            resource_templates: server.capabilities.resource_templates,
            resource_subscriptions: server.capabilities.resource_subscriptions,
            prompts: server.capabilities.prompts,
            completions: server.capabilities.completions,
            tasks: server.capabilities.tasks,
            notifications: server.capabilities.notifications,
        },
        tools: server.tools.iter().map(ToString::to_string).collect(),
        compatibility_helpers: server
            .compatibility_helpers
            .iter()
            .map(ToString::to_string)
            .collect(),
        resources,
        prompts: server.prompts.iter().map(ToString::to_string).collect(),
        required_scopes: server
            .required_scopes
            .iter()
            .map(ToString::to_string)
            .collect(),
        owned_routes: server
            .owned_routes
            .iter()
            .map(|route| ServerRouteSummary {
                path: route.path.to_string(),
                purpose: owned_route_purpose_label(route.purpose).to_owned(),
            })
            .collect(),
        profiles: control
            .profiles
            .iter()
            .filter(|profile| {
                profile
                    .servers
                    .iter()
                    .any(|item| item.server == server.slug)
            })
            .map(|profile| profile.id.to_string())
            .collect(),
    }
}

fn resource_selector_label(selector: &ResourceSelector) -> String {
    match selector {
        ResourceSelector::Scheme { scheme } => format!("{scheme}://**"),
        ResourceSelector::UriPrefix { prefix } => format!("{prefix}**"),
        ResourceSelector::Template { uri_template } => uri_template.to_string(),
    }
}

const fn owned_route_purpose_label(purpose: OwnedRoutePurpose) -> &'static str {
    match purpose {
        OwnedRoutePurpose::Webhook => "webhook",
        OwnedRoutePurpose::ArtifactBytes => "artifact_bytes",
        OwnedRoutePurpose::ProviderFetchableFiles => "provider_fetchable_files",
        OwnedRoutePurpose::Health => "health",
    }
}

pub(crate) fn display_record(
    names: &BTreeMap<String, String>,
    record: &RecordId,
) -> Result<String, UnsupportedRecordKey> {
    let key = record_key(record)?;
    Ok(names.get(&key).cloned().unwrap_or(key))
}

#[derive(Debug)]
pub(crate) struct UnsupportedRecordKey {
    table: String,
    key_kind: &'static str,
}

impl std::fmt::Display for UnsupportedRecordKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "console projection does not support {} record keys on table {}",
            self.key_kind, self.table,
        )
    }
}

impl std::error::Error for UnsupportedRecordKey {}

pub(crate) fn record_key(record: &RecordId) -> Result<String, UnsupportedRecordKey> {
    match &record.key {
        RecordIdKey::String(value) => Ok(value.clone()),
        RecordIdKey::Uuid(value) => Ok(value.to_string()),
        RecordIdKey::Number(value) => Ok(value.to_string()),
        RecordIdKey::Array(_) => Err(unsupported_record_key(record, "array")),
        RecordIdKey::Object(_) => Err(unsupported_record_key(record, "object")),
        RecordIdKey::Range(_) => Err(unsupported_record_key(record, "range")),
    }
}

fn unsupported_record_key(record: &RecordId, key_kind: &'static str) -> UnsupportedRecordKey {
    UnsupportedRecordKey {
        table: record.table.as_str().to_owned(),
        key_kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_link_snapshot_never_serializes_bearer_hash_material() {
        let now = Utc::now();
        let value = serde_json::to_value(ArtifactShareLinkSummary {
            id: "0197f78e-f2f0-7a6e-8a5d-f41c691e4471".to_owned(),
            permission: veoveo_platform_store::GrantPermission::Read,
            expires_at: now,
            max_downloads: Some(3),
            download_count: 1,
            revoked_at: None,
            created_at: now,
            active: true,
        })
        .expect("share-link summary serializes");
        let encoded = serde_json::to_string(&value).expect("JSON serializes");

        assert!(!encoded.contains("token_hash"));
        assert!(!encoded.contains("tokenHash"));
        assert!(!encoded.contains("url"));
        assert_eq!(value["active"], true);
    }
}
