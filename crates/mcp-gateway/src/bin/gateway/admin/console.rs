use std::{collections::BTreeMap, time::Instant};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::{GatewayAction, GatewayControlPlane};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, GatewayServerHealth, GatewayServerHealthState,
    probe_gateway_server_health,
};
use veoveo_platform_store::{
    AgentRecord, ArtifactBlobRecord, ArtifactGrantEdge, ArtifactOccurrenceRecord, AuditEventRecord,
    PrincipalRecord, RecordId, RecordIdKey, RecordingRecord, SegmentRecord, ShareLinkRecord,
    TaskRecord, WakeRecord, deterministic_tenant_id,
};

use crate::{
    admin::admin_profile_id,
    audit::{authorize_admin_request, internal_error_response},
    runtime::AdminState,
};

const SNAPSHOT_LIMIT: i64 = 200;

pub(crate) async fn read_console_snapshot(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (catalog, _profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminRead,
        "admin/console/snapshot",
        BTreeMap::new(),
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let tenant_key = subject
        .principal
        .tenant
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "installation".to_owned());
    let tenant = match deterministic_tenant_id(&tenant_key) {
        Ok(tenant) => tenant.record_id(),
        Err(error) => return internal_error_response(error),
    };
    let projection = match load_projection(&state, &tenant).await {
        Ok(projection) => projection,
        Err(error) => return internal_error_response(error),
    };
    let active_revision = match state.control_store.load_active_revision().await {
        Ok(revision) => revision,
        Err(error) => return internal_error_response(error),
    };
    let server_health = probe_gateway_server_health(&catalog).await;
    let snapshot = match build_snapshot(
        catalog.control_plane(),
        &subject,
        &tenant_key,
        active_revision.as_ref().map(|revision| revision.applied_at),
        projection,
        &server_health,
        state.offline_mode,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return internal_error_response(error),
    };
    Json(snapshot).into_response()
}

struct Projection {
    principals: Vec<PrincipalRecord>,
    tasks: Vec<TaskRecord>,
    artifacts: Vec<ArtifactOccurrenceRecord>,
    blobs: Vec<ArtifactBlobRecord>,
    share_links: Vec<ShareLinkRecord>,
    grants: Vec<ArtifactGrantEdge>,
    agents: Vec<AgentRecord>,
    wakes: Vec<WakeRecord>,
    recordings: Vec<RecordingRecord>,
    segments: Vec<SegmentRecord>,
    audit: Vec<AuditEventRecord>,
}

async fn load_projection(state: &AdminState, tenant: &RecordId) -> anyhow::Result<Projection> {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConsoleSnapshot {
    installation: InstallationSummary,
    session: SessionSummary,
    services: Vec<ServiceSummary>,
    tasks: Vec<TaskSummary>,
    artifacts: Vec<ArtifactSummary>,
    agents: Vec<AgentSummary>,
    recordings: Vec<RecordingSummary>,
    servers: Vec<ServerSummary>,
    policies: Vec<PolicySummary>,
    audit: Vec<AuditSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallationSummary {
    name: String,
    version: &'static str,
    offline_mode: bool,
    database_topology: &'static str,
    generated_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionSummary {
    display_name: String,
    principal_id: String,
    tenant_id: String,
    tenant_name: String,
    available_tenants: Vec<TenantSummary>,
}

#[derive(Serialize)]
struct TenantSummary {
    id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceSummary {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    state: &'static str,
    detail: String,
    checked_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskSummary {
    id: String,
    r#type: String,
    server: String,
    owner: String,
    state: veoveo_platform_store::TaskStatus,
    recovery_class: veoveo_platform_store::RecoveryClass,
    progress: f64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result_artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactSummary {
    id: String,
    filename: String,
    media_type: String,
    byte_length: i64,
    owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    classification: String,
    labels: Vec<String>,
    release_state: veoveo_platform_store::ArtifactReleaseState,
    authorized_grants: usize,
    active_links: usize,
    grants: Vec<ArtifactGrantSummary>,
    share_links: Vec<ArtifactShareLinkSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retention_expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactGrantSummary {
    subject_kind: veoveo_platform_store::ArtifactGrantSubjectKind,
    subject: String,
    permission: veoveo_platform_store::GrantPermission,
    labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactShareLinkSummary {
    id: String,
    permission: veoveo_platform_store::GrantPermission,
    expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_downloads: Option<i64>,
    download_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    revoked_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    active: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentSummary {
    id: String,
    name: String,
    profile: String,
    state: veoveo_platform_store::AgentState,
    pending_wakes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_episode_at: Option<DateTime<Utc>>,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingSummary {
    id: String,
    application: String,
    recording_key: String,
    state: veoveo_platform_store::RecordingState,
    segments: usize,
    byte_length: i64,
    started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerSummary {
    id: String,
    name: String,
    transport: &'static str,
    endpoint: String,
    state: GatewayServerHealthState,
    checked_at: DateTime<Utc>,
    tools: usize,
    resources: usize,
    prompts: usize,
    profiles: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicySummary {
    id: String,
    name: String,
    revision: usize,
    state: &'static str,
    rules: usize,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditSummary {
    id: String,
    occurred_at: DateTime<Utc>,
    actor: String,
    action: String,
    resource: String,
    outcome: veoveo_platform_store::AuditOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
}

fn build_snapshot(
    control: &GatewayControlPlane,
    subject: &AuthenticatedSubject,
    tenant_key: &str,
    control_updated_at: Option<DateTime<Utc>>,
    projection: Projection,
    server_health: &BTreeMap<veoveo_mcp_contract::ServerSlug, GatewayServerHealth>,
    offline_mode: bool,
) -> anyhow::Result<ConsoleSnapshot> {
    let now = Utc::now();
    let principal_names: BTreeMap<_, _> = projection
        .principals
        .iter()
        .map(|principal| Ok((record_key(&principal.id)?, principal.display_name.clone())))
        .collect::<anyhow::Result<_>>()?;
    let tenant_name = control
        .tenants
        .iter()
        .find(|tenant| tenant.id.as_str() == tenant_key)
        .and_then(|tenant| tenant.title.clone())
        .unwrap_or_else(|| tenant_key.to_owned());
    let display_name = projection
        .principals
        .iter()
        .find(|principal| {
            principal.issuer == subject.principal.issuer.as_str()
                && principal.subject == subject.principal.subject.as_str()
        })
        .map(|principal| principal.display_name.clone())
        .unwrap_or_else(|| subject.principal.id.to_string());
    let blobs: BTreeMap<_, _> = projection
        .blobs
        .iter()
        .map(|blob| Ok((record_key(&blob.id)?, blob)))
        .collect::<anyhow::Result<_>>()?;
    let mut grants = BTreeMap::<String, Vec<ArtifactGrantSummary>>::new();
    for grant in &projection.grants {
        grants
            .entry(record_key(&grant.r#in)?)
            .or_default()
            .push(ArtifactGrantSummary {
                subject_kind: grant.subject_kind,
                subject: grant.subject_key.clone(),
                permission: grant.permission,
                labels: grant.labels.clone(),
                expires_at: grant.expires_at,
                created_at: grant.created_at,
            });
    }
    for artifact_grants in grants.values_mut() {
        artifact_grants.sort_by_key(|grant| std::cmp::Reverse(grant.created_at));
    }
    let mut links = BTreeMap::<String, Vec<ArtifactShareLinkSummary>>::new();
    for link in &projection.share_links {
        let active = link.revoked_at.is_none()
            && link.expires_at > now
            && link
                .max_downloads
                .is_none_or(|max| link.download_count < max);
        links
            .entry(record_key(&link.artifact)?)
            .or_default()
            .push(ArtifactShareLinkSummary {
                id: record_key(&link.id)?,
                permission: link.permission,
                expires_at: link.expires_at,
                max_downloads: link.max_downloads,
                download_count: link.download_count,
                revoked_at: link.revoked_at,
                created_at: link.created_at,
                active,
            });
    }
    for artifact_links in links.values_mut() {
        artifact_links.sort_by_key(|link| std::cmp::Reverse(link.created_at));
    }
    let mut pending_wakes = BTreeMap::<String, usize>::new();
    for wake in &projection.wakes {
        if matches!(wake.state, veoveo_platform_store::WakeState::Pending) {
            *pending_wakes.entry(record_key(&wake.agent)?).or_default() += 1;
        }
    }
    let mut recording_segments = BTreeMap::<String, (usize, i64)>::new();
    for segment in &projection.segments {
        let aggregate = recording_segments
            .entry(record_key(&segment.recording)?)
            .or_default();
        aggregate.0 += 1;
        aggregate.1 += segment.byte_len;
    }

    let services = vec![
        ServiceSummary {
            id: "surrealdb",
            name: "SurrealDB",
            kind: "database",
            state: "healthy",
            detail: "RocksDB, single node".to_owned(),
            checked_at: now,
        },
        ServiceSummary {
            id: "gateway",
            name: "MCP Gateway",
            kind: "gateway",
            state: "healthy",
            detail: format!("{} profiles active", control.profiles.len()),
            checked_at: now,
        },
    ];
    let tasks = projection
        .tasks
        .into_iter()
        .map(|task| {
            Ok(TaskSummary {
                id: record_key(&task.id)?,
                r#type: task.task_type,
                server: record_key(&task.server)?,
                owner: display_record(&principal_names, &task.owner)?,
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
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let artifacts = projection
        .artifacts
        .into_iter()
        .map(|artifact| {
            let blob_key = record_key(&artifact.blob)?;
            let blob = blobs.get(&blob_key);
            let id = record_key(&artifact.id)?;
            let artifact_grants = grants.get(&id).cloned().unwrap_or_default();
            let artifact_links = links.get(&id).cloned().unwrap_or_default();
            Ok(ArtifactSummary {
                id: id.clone(),
                filename: artifact.filename.unwrap_or_else(|| "artifact".to_owned()),
                media_type: artifact.media_type,
                byte_length: blob.map_or(0, |blob| blob.byte_len),
                owner: display_record(&principal_names, &artifact.owner)?,
                task_id: artifact.task.as_ref().map(record_key).transpose()?,
                classification: artifact.classification,
                labels: artifact.labels,
                release_state: artifact.release_state,
                authorized_grants: artifact_grants.len(),
                active_links: artifact_links.iter().filter(|link| link.active).count(),
                grants: artifact_grants,
                share_links: artifact_links,
                retention_expires_at: artifact.retention_expires_at,
                created_at: artifact.created_at,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let agents = projection
        .agents
        .into_iter()
        .map(|agent| {
            let id = record_key(&agent.id)?;
            let detail = match agent.last_episode.as_ref() {
                Some(episode) => format!("Episode {}", record_key(episode)?),
                None => "No completed episode".to_owned(),
            };
            Ok(AgentSummary {
                id: id.clone(),
                name: agent.display_name,
                profile: record_key(&agent.profile)?,
                state: agent.state,
                pending_wakes: pending_wakes.get(&id).copied().unwrap_or(0),
                last_episode_at: agent.last_episode.as_ref().map(|_| agent.updated_at),
                detail,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let recordings = projection
        .recordings
        .into_iter()
        .map(|recording| {
            let id = record_key(&recording.id)?;
            let aggregate = recording_segments.get(&id).copied().unwrap_or_default();
            Ok(RecordingSummary {
                id,
                application: recording.application_id,
                recording_key: recording.recording_key,
                state: recording.state,
                segments: aggregate.0,
                byte_length: aggregate.1,
                started_at: recording.started_at,
                ended_at: recording.ended_at,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let servers = control
        .servers
        .iter()
        .map(|server| {
            let health = server_health.get(&server.slug);
            ServerSummary {
                id: server.slug.to_string(),
                name: server.slug.to_string(),
                transport: "streamable_http",
                endpoint: server.upstream.url.to_string(),
                state: health.map_or(GatewayServerHealthState::Offline, |health| health.state),
                checked_at: health.map_or(now, |health| health.checked_at),
                tools: server.tools.len(),
                resources: usize::from(server.capabilities.resources),
                prompts: server.prompts.len(),
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
        })
        .collect();
    let updated_at = control_updated_at.unwrap_or(now);
    let policies = control
        .policies
        .iter()
        .enumerate()
        .map(|(index, policy)| PolicySummary {
            id: policy.version.to_string(),
            name: policy.version.to_string(),
            revision: index + 1,
            state: "active",
            rules: policy.rules.len(),
            updated_at,
        })
        .collect();
    let audit = projection
        .audit
        .into_iter()
        .map(|event| {
            let actor = event
                .actor
                .as_ref()
                .map(|actor| display_record(&principal_names, actor))
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
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(ConsoleSnapshot {
        installation: InstallationSummary {
            name: "Veoveo".to_owned(),
            version: env!("CARGO_PKG_VERSION"),
            offline_mode,
            database_topology: "single-node",
            generated_at: now,
        },
        session: SessionSummary {
            display_name,
            principal_id: subject.principal.id.to_string(),
            tenant_id: tenant_key.to_owned(),
            tenant_name: tenant_name.clone(),
            available_tenants: vec![TenantSummary {
                id: tenant_key.to_owned(),
                name: tenant_name,
            }],
        },
        services,
        tasks,
        artifacts,
        agents,
        recordings,
        servers,
        policies,
        audit,
    })
}

fn display_record(
    names: &BTreeMap<String, String>,
    record: &RecordId,
) -> Result<String, UnsupportedRecordKey> {
    let key = record_key(record)?;
    Ok(names.get(&key).cloned().unwrap_or(key))
}

#[derive(Debug)]
struct UnsupportedRecordKey {
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

fn record_key(record: &RecordId) -> Result<String, UnsupportedRecordKey> {
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
