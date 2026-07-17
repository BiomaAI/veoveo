mod health;
mod projection;
mod stream;

use std::{collections::BTreeMap, time::Instant};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::{GatewayAction, GatewayControlPlane, ServerSlug};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayServerHealth};
use veoveo_platform_store::{ChangefeedCursor, SegmentState, deterministic_tenant_id};

pub(crate) use health::{ServerHealthMonitor, spawn_server_health_prober};
use projection::{
    AgentSummary, ArtifactGrantSummary, ArtifactShareLinkSummary, ArtifactSummary, AuditSummary,
    PolicySummary, RecordingSummary, ServerSummary, TaskSummary,
};
use projection::{
    Projection, agent_summary, artifact_grant_summary, artifact_summary, audit_summary,
    load_projection, record_key, recording_summary, server_summary, share_link_summary,
    task_summary,
};
pub(crate) use stream::{ConsoleStreamRuntime, spawn_console_wake_hub, stream_console};

use crate::{
    admin::admin_profile_id,
    audit::{authorize_admin_request, internal_error_response},
    runtime::AdminState,
};

const DEFAULT_INSTALLATION_NAME: &str = "Veoveo";
const DEFAULT_PRODUCT_LABEL: &str = "Operations";

pub(crate) async fn authorize_console_cluster(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminRead,
        "admin/console/cluster",
        BTreeMap::new(),
        started_at,
    )
    .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(response) => *response,
    }
}

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
    // The stream cursor is anchored on the database clock BEFORE the
    // projection reads so replay from it overlaps the snapshot state
    // instead of gapping it.
    let stream_cursor = match state
        .control_store
        .platform_store()
        .changefeed_cursor_now()
        .await
    {
        Ok(cursor) => cursor,
        Err(error) => return internal_error_response(anyhow::Error::from(error)),
    };
    let projection = match load_projection(&state, &tenant).await {
        Ok(projection) => projection,
        Err(error) => return internal_error_response(error),
    };
    let active_revision = match state.control_store.load_active_revision().await {
        Ok(revision) => revision,
        Err(error) => return internal_error_response(error),
    };
    // Health comes from the background prober; the first snapshot within
    // one probe interval of boot may briefly report servers offline.
    let server_health = state.server_health.snapshot();
    let snapshot = match build_snapshot(
        catalog.control_plane(),
        &subject,
        &tenant_key,
        active_revision.as_ref().map(|revision| revision.applied_at),
        projection,
        &server_health,
        state.offline_mode,
        stream_cursor,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return internal_error_response(error),
    };
    Json(snapshot).into_response()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConsoleSnapshot {
    installation: InstallationSummary,
    session: SessionSummary,
    stream: StreamInfo,
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
    product_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    logo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accent_color: Option<String>,
    version: &'static str,
    offline_mode: bool,
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

/// Console live-stream bootstrap: the changefeed cursor the browser passes
/// to `GET /admin/{profile}/console/stream` so replay begins where this
/// snapshot's view of the world ends.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamInfo {
    cursor: String,
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

#[expect(
    clippy::too_many_arguments,
    reason = "snapshot assembly aggregates the full console surface"
)]
fn build_snapshot(
    control: &GatewayControlPlane,
    subject: &AuthenticatedSubject,
    tenant_key: &str,
    control_updated_at: Option<DateTime<Utc>>,
    projection: Projection,
    server_health: &BTreeMap<ServerSlug, GatewayServerHealth>,
    offline_mode: bool,
    stream_cursor: ChangefeedCursor,
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
    let blob_lengths: BTreeMap<_, _> = projection
        .blobs
        .iter()
        .map(|blob| Ok((record_key(&blob.id)?, blob.byte_len)))
        .collect::<anyhow::Result<_>>()?;
    let mut grants = BTreeMap::<String, Vec<ArtifactGrantSummary>>::new();
    for grant in &projection.grants {
        grants
            .entry(record_key(&grant.r#in)?)
            .or_default()
            .push(artifact_grant_summary(grant));
    }
    for artifact_grants in grants.values_mut() {
        artifact_grants.sort_by_key(|grant| std::cmp::Reverse(grant.created_at));
    }
    let mut links = BTreeMap::<String, Vec<ArtifactShareLinkSummary>>::new();
    for link in &projection.share_links {
        links
            .entry(record_key(&link.artifact)?)
            .or_default()
            .push(share_link_summary(link, now)?);
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
    let mut recording_segments = BTreeMap::<String, (usize, usize, i64)>::new();
    for segment in &projection.segments {
        let aggregate = recording_segments
            .entry(record_key(&segment.recording)?)
            .or_default();
        aggregate.0 += 1;
        if matches!(segment.state, SegmentState::Frozen | SegmentState::Sealed) {
            aggregate.1 += 1;
            aggregate.2 += segment.byte_len;
        }
    }

    let services = vec![
        ServiceSummary {
            id: "surrealdb",
            name: "SurrealDB",
            kind: "database",
            state: "healthy",
            detail: "Control store · RocksDB".to_owned(),
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
        .map(|task| task_summary(task, &principal_names))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let artifacts = projection
        .artifacts
        .into_iter()
        .map(|artifact| {
            let blob_key = record_key(&artifact.blob)?;
            let byte_length = blob_lengths.get(&blob_key).copied().unwrap_or(0);
            let id = record_key(&artifact.id)?;
            artifact_summary(
                artifact,
                byte_length,
                grants.get(&id).cloned().unwrap_or_default(),
                links.get(&id).cloned().unwrap_or_default(),
                &principal_names,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let agents = projection
        .agents
        .into_iter()
        .map(|agent| {
            let id = record_key(&agent.id)?;
            agent_summary(agent, pending_wakes.get(&id).copied().unwrap_or(0))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let recordings = projection
        .recordings
        .into_iter()
        .map(|recording| {
            let id = record_key(&recording.id)?;
            let aggregate = recording_segments.get(&id).copied().unwrap_or_default();
            recording_summary(recording, aggregate.0, aggregate.1, aggregate.2)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let servers = control
        .servers
        .iter()
        .map(|server| server_summary(server, control, server_health.get(&server.slug), now))
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
        .map(|event| audit_summary(event, &principal_names))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let branding = control.branding.as_ref();
    Ok(ConsoleSnapshot {
        installation: InstallationSummary {
            name: branding
                .map(|branding| branding.name.trim().to_owned())
                .unwrap_or_else(|| DEFAULT_INSTALLATION_NAME.to_owned()),
            product_label: branding
                .and_then(|branding| branding.product_label.clone())
                .unwrap_or_else(|| DEFAULT_PRODUCT_LABEL.to_owned()),
            logo: branding.and_then(|branding| branding.logo.clone()),
            accent_color: branding.and_then(|branding| branding.accent_color.clone()),
            version: env!("CARGO_PKG_VERSION"),
            offline_mode,
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
        stream: StreamInfo {
            cursor: stream_cursor.versionstamp().to_string(),
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
