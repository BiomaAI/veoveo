use std::{collections::BTreeMap, time::Instant};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use veoveo_mcp_contract::{
    GatewayAction, GatewayJwtRevocation, GatewayJwtRevocationAdminStatus,
    GatewayJwtRevocationApplyResult, GatewayJwtRevocationPruneResult, GatewayJwtRevocationRequest,
};
use veoveo_mcp_gateway::AuthenticatedSubject;

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminOperationAuditRecord, AdminOperationFailure, AdminOperationStatus,
        admin_revocation_metadata, authorize_admin_request, internal_error_response,
        record_admin_operation_audit,
    },
    runtime::AdminState,
};

const ADMIN_JWT_REVOCATIONS_RESULT_METHOD: &str = "admin/jwt-revocations/result";
const ADMIN_JWT_REVOCATIONS_PRUNE_RESULT_METHOD: &str = "admin/jwt-revocations/prune/result";

pub(crate) async fn revoke_jwt(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<GatewayJwtRevocationRequest>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let metadata = admin_revocation_metadata(&request);
    let (_catalog, profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminWrite,
        "admin/jwt-revocations",
        metadata.clone(),
        started_at,
    ) {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };

    let revoked_at = Utc::now();
    if request.expires_at <= revoked_at {
        if let Err(err) = record_admin_operation_audit(
            &state,
            &profile,
            &subject,
            AdminOperationAuditRecord {
                action: GatewayAction::AdminWrite,
                method: ADMIN_JWT_REVOCATIONS_RESULT_METHOD,
                started_at,
                status: AdminOperationStatus::Rejected,
                failure: Some(AdminOperationFailure::ExpiredRevocation),
                metadata,
            },
        ) {
            return internal_error_response(err);
        }
        return (
            StatusCode::BAD_REQUEST,
            "revocation expiration must be in the future",
        )
            .into_response();
    }
    let revocation = GatewayJwtRevocation {
        profile: profile.id.clone(),
        issuer: request.issuer,
        jwt_id: request.jwt_id,
        revoked_at,
        expires_at: request.expires_at,
        reason: request.reason,
    };
    if let Err(err) = state.gateway_state.record_jwt_revocation(&revocation) {
        tracing::error!("failed to persist gateway JWT revocation: {err}");
        if let Err(audit_err) = record_admin_operation_audit(
            &state,
            &profile,
            &subject,
            AdminOperationAuditRecord {
                action: GatewayAction::AdminWrite,
                method: ADMIN_JWT_REVOCATIONS_RESULT_METHOD,
                started_at,
                status: AdminOperationStatus::Failed,
                failure: Some(AdminOperationFailure::PersistJwtRevocation),
                metadata: metadata.clone(),
            },
        ) {
            return internal_error_response(audit_err);
        }
        return internal_error_response(err);
    }
    if let Err(err) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        AdminOperationAuditRecord {
            action: GatewayAction::AdminWrite,
            method: ADMIN_JWT_REVOCATIONS_RESULT_METHOD,
            started_at,
            status: AdminOperationStatus::Succeeded,
            failure: None,
            metadata,
        },
    ) {
        return internal_error_response(err);
    }
    tracing::info!(
        profile = %profile.id,
        principal = %subject.principal.id,
        issuer = %revocation.issuer,
        jwt_id = %revocation.jwt_id,
        "gateway JWT revoked"
    );
    Json(GatewayJwtRevocationApplyResult {
        status: GatewayJwtRevocationAdminStatus::Revoked,
        revocation,
    })
    .into_response()
}

pub(crate) async fn prune_jwt_revocations(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let metadata = BTreeMap::from([("operation".to_string(), "prune_jwt_revocations".to_string())]);
    let (_catalog, profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminWrite,
        "admin/jwt-revocations/prune",
        metadata.clone(),
        started_at,
    ) {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let deleted = match state
        .gateway_state
        .prune_expired_jwt_revocations(Utc::now())
    {
        Ok(deleted) => deleted,
        Err(err) => {
            tracing::error!("failed to prune expired gateway JWT revocations: {err}");
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    action: GatewayAction::AdminWrite,
                    method: ADMIN_JWT_REVOCATIONS_PRUNE_RESULT_METHOD,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::PruneJwtRevocations),
                    metadata,
                },
            ) {
                return internal_error_response(audit_err);
            }
            return internal_error_response(err);
        }
    };
    if let Err(err) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        AdminOperationAuditRecord {
            action: GatewayAction::AdminWrite,
            method: ADMIN_JWT_REVOCATIONS_PRUNE_RESULT_METHOD,
            started_at,
            status: AdminOperationStatus::Succeeded,
            failure: None,
            metadata: {
                let mut metadata = metadata;
                metadata.insert("deleted".to_string(), deleted.to_string());
                metadata
            },
        },
    ) {
        return internal_error_response(err);
    }
    tracing::info!(
        profile = %profile.id,
        principal = %subject.principal.id,
        deleted,
        "expired gateway JWT revocations pruned"
    );
    Json(GatewayJwtRevocationPruneResult {
        status: GatewayJwtRevocationAdminStatus::Pruned,
        deleted,
    })
    .into_response()
}
