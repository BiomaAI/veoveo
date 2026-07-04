use std::{collections::BTreeMap, sync::Arc, time::Instant};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Serialize;
use veoveo_mcp_contract::{
    GatewayAction, GatewayControlPlane, GatewayControlPlaneRevision, GatewayControlPlaneRevisionId,
    GatewayControlPlaneRevisionSource,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalog};

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminOperationAuditRecord, AdminOperationFailure, AdminOperationStatus,
        authorize_admin_request, control_plane_sha256, internal_error_response,
        record_admin_operation_audit,
    },
    runtime::{AdminState, build_http_client, replace_catalog, replace_http_client},
};

const ADMIN_CONTROL_PLANE_RESULT_METHOD: &str = "admin/control-plane/result";

#[derive(Debug, Serialize)]
struct ControlPlaneReadResult {
    status: &'static str,
    revision_id: Option<GatewayControlPlaneRevisionId>,
    sha256: String,
    servers: usize,
    profiles: usize,
    control_plane: GatewayControlPlane,
}

#[derive(Debug, Serialize)]
struct ControlPlaneApplyResult {
    status: &'static str,
    revision_id: GatewayControlPlaneRevisionId,
    sha256: String,
    servers: usize,
    profiles: usize,
}

pub(crate) async fn read_control_plane(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (catalog, profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminRead,
        "admin/control-plane",
        BTreeMap::new(),
        started_at,
    ) {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };

    let sha256 = match control_plane_sha256(catalog.control_plane()) {
        Ok(sha256) => sha256,
        Err(err) => {
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    action: GatewayAction::AdminRead,
                    method: ADMIN_CONTROL_PLANE_RESULT_METHOD,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::ControlPlaneSha),
                    metadata: BTreeMap::new(),
                },
            ) {
                return internal_error_response(audit_err);
            }
            return internal_error_response(err);
        }
    };
    let revision_id = match state.control_db.load_active_revision().await {
        Ok(Some(revision)) if revision.sha256 == sha256 => Some(revision.revision_id),
        Ok(_) => None,
        Err(err) => {
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    action: GatewayAction::AdminRead,
                    method: ADMIN_CONTROL_PLANE_RESULT_METHOD,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::LatestRevisionRead),
                    metadata: BTreeMap::from([("sha256".to_string(), sha256.clone())]),
                },
            ) {
                return internal_error_response(audit_err);
            }
            return internal_error_response(err);
        }
    };
    let mut metadata = BTreeMap::from([
        ("sha256".to_string(), sha256.clone()),
        ("servers".to_string(), catalog.server_count().to_string()),
        ("profiles".to_string(), catalog.profile_count().to_string()),
    ]);
    if let Some(revision_id) = &revision_id {
        metadata.insert("revision_id".to_string(), revision_id.to_string());
    }
    if let Err(err) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        AdminOperationAuditRecord {
            action: GatewayAction::AdminRead,
            method: ADMIN_CONTROL_PLANE_RESULT_METHOD,
            started_at,
            status: AdminOperationStatus::Succeeded,
            failure: None,
            metadata,
        },
    ) {
        return internal_error_response(err);
    }

    Json(ControlPlaneReadResult {
        status: "ok",
        revision_id,
        sha256,
        servers: catalog.server_count(),
        profiles: catalog.profile_count(),
        control_plane: catalog.control_plane().clone(),
    })
    .into_response()
}

pub(crate) async fn update_control_plane(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(control_plane): Json<GatewayControlPlane>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (_catalog, profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminWrite,
        "admin/control-plane",
        BTreeMap::new(),
        started_at,
    ) {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };

    let new_catalog = match GatewayCatalog::from_control_plane(control_plane.clone()) {
        Ok(catalog) => Arc::new(catalog),
        Err(err) => {
            tracing::warn!("rejected invalid gateway control plane update: {err}");
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    action: GatewayAction::AdminWrite,
                    method: ADMIN_CONTROL_PLANE_RESULT_METHOD,
                    started_at,
                    status: AdminOperationStatus::Rejected,
                    failure: Some(AdminOperationFailure::InvalidControlPlane),
                    metadata: BTreeMap::new(),
                },
            ) {
                return internal_error_response(audit_err);
            }
            return (StatusCode::BAD_REQUEST, "invalid gateway control plane").into_response();
        }
    };
    let new_http = match build_http_client(&new_catalog) {
        Ok(client) => client,
        Err(err) => {
            tracing::error!("failed to rebuild gateway HTTP client: {err}");
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    action: GatewayAction::AdminWrite,
                    method: ADMIN_CONTROL_PLANE_RESULT_METHOD,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::BuildHttpClient),
                    metadata: BTreeMap::new(),
                },
            ) {
                return internal_error_response(audit_err);
            }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to rebuild gateway HTTP client",
            )
                .into_response();
        }
    };
    let sha256 = match control_plane_sha256(&control_plane) {
        Ok(sha256) => sha256,
        Err(err) => {
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    action: GatewayAction::AdminWrite,
                    method: ADMIN_CONTROL_PLANE_RESULT_METHOD,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::ControlPlaneSha),
                    metadata: BTreeMap::new(),
                },
            ) {
                return internal_error_response(audit_err);
            }
            return internal_error_response(err);
        }
    };
    let revision_id =
        match GatewayControlPlaneRevisionId::new(format!("gcp-{}", uuid::Uuid::new_v4())) {
            Ok(revision_id) => revision_id,
            Err(err) => {
                if let Err(audit_err) = record_admin_operation_audit(
                    &state,
                    &profile,
                    &subject,
                    AdminOperationAuditRecord {
                        action: GatewayAction::AdminWrite,
                        method: ADMIN_CONTROL_PLANE_RESULT_METHOD,
                        started_at,
                        status: AdminOperationStatus::Failed,
                        failure: Some(AdminOperationFailure::RevisionId),
                        metadata: BTreeMap::from([("sha256".to_string(), sha256.clone())]),
                    },
                ) {
                    return internal_error_response(audit_err);
                }
                return internal_error_response(err);
            }
        };
    let revision = GatewayControlPlaneRevision {
        revision_id: revision_id.clone(),
        sha256: sha256.clone(),
        source: GatewayControlPlaneRevisionSource::AdminApi,
        applied_at: Utc::now(),
        applied_by: subject.principal.id.clone(),
        tenant: subject.principal.tenant.clone(),
        control_plane,
    };
    if let Err(err) = state.control_db.record_revision(&revision).await {
        tracing::error!("failed to persist gateway control-plane revision: {err}");
        if let Err(audit_err) = record_admin_operation_audit(
            &state,
            &profile,
            &subject,
            AdminOperationAuditRecord {
                action: GatewayAction::AdminWrite,
                method: ADMIN_CONTROL_PLANE_RESULT_METHOD,
                started_at,
                status: AdminOperationStatus::Failed,
                failure: Some(AdminOperationFailure::PersistControlPlaneRevision),
                metadata: BTreeMap::from([
                    ("revision_id".to_string(), revision_id.to_string()),
                    ("sha256".to_string(), sha256.clone()),
                ]),
            },
        ) {
            return internal_error_response(audit_err);
        }
        return internal_error_response(err);
    }

    let servers = new_catalog.server_count();
    let profiles = new_catalog.profile_count();
    if let Err(err) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        AdminOperationAuditRecord {
            action: GatewayAction::AdminWrite,
            method: ADMIN_CONTROL_PLANE_RESULT_METHOD,
            started_at,
            status: AdminOperationStatus::Succeeded,
            failure: None,
            metadata: BTreeMap::from([
                ("revision_id".to_string(), revision_id.to_string()),
                ("sha256".to_string(), sha256.clone()),
                ("servers".to_string(), servers.to_string()),
                ("profiles".to_string(), profiles.to_string()),
            ]),
        },
    ) {
        return internal_error_response(err);
    }
    replace_http_client(&state.http, new_http);
    replace_catalog(&state.catalog, new_catalog);
    tracing::info!(
        profile = %profile.id,
        principal = %subject.principal.id,
        revision_id = %revision_id,
        sha256 = %sha256,
        servers,
        profiles,
        "gateway control plane updated"
    );
    Json(ControlPlaneApplyResult {
        status: "applied",
        revision_id,
        sha256,
        servers,
        profiles,
    })
    .into_response()
}
