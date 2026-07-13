use std::{collections::BTreeMap, sync::Arc, time::Instant};

use axum::{
    http::{HeaderMap, HeaderValue, StatusCode, header::WWW_AUTHENTICATE},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    AuditEvent, AuthAuditEvent, AuthMethod, AuthOutcome, AuthReasonCode, GatewayAction,
    GatewayControlPlane, GatewayJwtRevocationRequest, GatewayProfile, GatewayProfileId, JwtId,
    McpMethodName, OAuthClientId, PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyTarget,
    Principal, PrincipalAuditAttributes, PrincipalId, ResourceAuthorizationServer, TokenSubject,
    TraceId,
};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, GatewayCatalog, GatewayState, PolicyRequest,
    merge_principal_audit_metadata, principal_audit_metadata, www_authenticate_challenge,
};

use crate::runtime::{AdminState, ProfileAuthState, current_catalog};

pub(super) async fn authorize_admin_request(
    state: &AdminState,
    profile_id: &GatewayProfileId,
    subject: AuthenticatedSubject,
    action: GatewayAction,
    audit_method: &str,
    audit_metadata: BTreeMap<String, String>,
    started_at: Instant,
) -> std::result::Result<(Arc<GatewayCatalog>, GatewayProfile, AuthenticatedSubject), Box<Response>>
{
    authorize_admin_target_request(
        state,
        profile_id,
        subject,
        AdminAuthorizationRequest {
            action,
            target: PolicyTarget::Gateway,
            method: audit_method,
            metadata: audit_metadata,
            started_at,
        },
    )
    .await
}

pub(super) struct AdminAuthorizationRequest<'a> {
    pub(super) action: GatewayAction,
    pub(super) target: PolicyTarget,
    pub(super) method: &'a str,
    pub(super) metadata: BTreeMap<String, String>,
    pub(super) started_at: Instant,
}

pub(super) async fn authorize_admin_target_request(
    state: &AdminState,
    profile_id: &GatewayProfileId,
    subject: AuthenticatedSubject,
    request: AdminAuthorizationRequest<'_>,
) -> std::result::Result<(Arc<GatewayCatalog>, GatewayProfile, AuthenticatedSubject), Box<Response>>
{
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(profile_id).cloned() else {
        return Err(Box::new(StatusCode::NOT_FOUND.into_response()));
    };
    let trace_id = match TraceId::new(uuid::Uuid::new_v4().to_string()) {
        Ok(trace_id) => trace_id,
        Err(err) => return Err(Box::new(internal_error_response(err))),
    };
    let decision = catalog.decide(PolicyRequest {
        principal: &subject.principal,
        profile: profile_id,
        action: request.action,
        target: &request.target,
        trace_id: &trace_id,
    });
    if let Err(err) = record_admin_audit(
        &state.gateway_state,
        &profile,
        &subject,
        AdminAuditRecord {
            action: request.action,
            target: request.target,
            decision: decision.clone(),
            method: request.method,
            metadata: request.metadata,
            started_at: request.started_at,
        },
    )
    .await
    {
        return Err(Box::new(internal_error_response(err)));
    }
    if decision.effect != PolicyEffect::Allow {
        tracing::warn!(
            profile = %profile_id,
            principal = %subject.principal.id,
            action = ?request.action,
            reason = ?decision.reason,
            "gateway admin request denied"
        );
        return Err(Box::new(StatusCode::FORBIDDEN.into_response()));
    }

    Ok((catalog, profile, subject))
}

pub(super) fn control_plane_sha256(control_plane: &GatewayControlPlane) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(control_plane)?;
    let digest = Sha256::digest(bytes);
    Ok(digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>())
}

struct AdminAuditRecord<'a> {
    action: GatewayAction,
    target: PolicyTarget,
    decision: PolicyDecision,
    method: &'a str,
    metadata: BTreeMap<String, String>,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum AdminOperationStatus {
    Succeeded,
    Rejected,
    Failed,
}

impl AdminOperationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum AdminOperationFailure {
    ArtifactGrant,
    ArtifactGrantRevoke,
    ArtifactReleaseState,
    ArtifactShareLink,
    ArtifactShareLinkRevoke,
    BuildHttpClient,
    CancelTask,
    ConnectFinalTaskExtension,
    ControlPlaneSha,
    ExpiredRevocation,
    InvalidControlPlane,
    IssueInternalToken,
    LatestRevisionRead,
    PersistControlPlaneRevision,
    PersistJwtRevocation,
    PruneJwtRevocations,
    RevisionId,
    TaskOwnership,
    TaskRoute,
}

impl AdminOperationFailure {
    fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactGrant => "artifact_grant",
            Self::ArtifactGrantRevoke => "artifact_grant_revoke",
            Self::ArtifactReleaseState => "artifact_release_state",
            Self::ArtifactShareLink => "artifact_share_link",
            Self::ArtifactShareLinkRevoke => "artifact_share_link_revoke",
            Self::BuildHttpClient => "build_http_client",
            Self::CancelTask => "cancel_task",
            Self::ConnectFinalTaskExtension => "connect_final_task_extension",
            Self::ControlPlaneSha => "control_plane_sha",
            Self::ExpiredRevocation => "expired_revocation",
            Self::InvalidControlPlane => "invalid_control_plane",
            Self::IssueInternalToken => "issue_internal_token",
            Self::LatestRevisionRead => "latest_revision_read",
            Self::PersistControlPlaneRevision => "persist_control_plane_revision",
            Self::PersistJwtRevocation => "persist_jwt_revocation",
            Self::PruneJwtRevocations => "prune_jwt_revocations",
            Self::RevisionId => "revision_id",
            Self::TaskOwnership => "task_ownership",
            Self::TaskRoute => "task_route",
        }
    }
}

pub(super) struct AdminOperationAuditRecord<'a> {
    pub(super) action: GatewayAction,
    pub(super) method: &'a str,
    pub(super) started_at: Instant,
    pub(super) status: AdminOperationStatus,
    pub(super) failure: Option<AdminOperationFailure>,
    pub(super) metadata: BTreeMap<String, String>,
}

pub(super) fn admin_revocation_metadata(
    request: &GatewayJwtRevocationRequest,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert("operation".to_string(), "revoke_jwt".to_string());
    metadata.insert("target_profile".to_string(), request.profile.to_string());
    metadata.insert("issuer".to_string(), request.issuer.to_string());
    metadata.insert("jwt_id".to_string(), request.jwt_id.to_string());
    metadata.insert("expires_at".to_string(), request.expires_at.to_rfc3339());
    if let Some(reason) = &request.reason {
        metadata.insert("reason".to_string(), reason.clone());
    }
    metadata
}

pub(super) async fn record_admin_operation_audit(
    state: &AdminState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    record: AdminOperationAuditRecord<'_>,
) -> anyhow::Result<()> {
    record_admin_target_operation_audit(state, profile, subject, PolicyTarget::Gateway, record)
        .await
}

pub(super) async fn record_admin_target_operation_audit(
    state: &AdminState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    target: PolicyTarget,
    record: AdminOperationAuditRecord<'_>,
) -> anyhow::Result<()> {
    let mut metadata = record.metadata;
    metadata.insert(
        "operation_status".to_string(),
        record.status.as_str().to_string(),
    );
    if let Some(failure) = record.failure {
        metadata.insert(
            "operation_failure".to_string(),
            failure.as_str().to_string(),
        );
    }

    let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let decision = PolicyDecision {
        effect: PolicyEffect::Allow,
        reason: PolicyReasonCode::PolicyAllow,
        evaluated_at: Utc::now(),
        profile: profile.id.clone(),
        action: record.action,
        target: target.clone(),
        principal: Some(subject.principal.id.clone()),
        tenant: subject.principal.tenant.clone(),
        policy_version: None,
        rule_id: None,
        trace_id,
    };
    record_admin_audit(
        &state.gateway_state,
        profile,
        subject,
        AdminAuditRecord {
            action: record.action,
            target,
            decision,
            method: record.method,
            metadata,
            started_at: record.started_at,
        },
    )
    .await
}

async fn record_admin_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    record: AdminAuditRecord<'_>,
) -> anyhow::Result<()> {
    let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let latency_ms = u64::try_from(record.started_at.elapsed().as_millis())?;
    gateway_state
        .record_audit_event(&AuditEvent {
            event_id,
            timestamp: record.decision.evaluated_at,
            trace_id: record.decision.trace_id.clone(),
            profile: profile.id.clone(),
            method: McpMethodName::new(record.method)?,
            action: record.action,
            target: record.target,
            decision: record.decision,
            principal: Some(subject.principal.id.clone()),
            principal_attributes: Some(PrincipalAuditAttributes::from(&subject.principal)),
            tenant: subject.principal.tenant.clone(),
            token_issuer: Some(subject.access_token.issuer.clone()),
            latency_ms: Some(latency_ms),
            metadata: merge_principal_audit_metadata(record.metadata, &subject.principal),
        })
        .await?;
    Ok(())
}

pub(super) async fn record_auth_audit(
    state: &ProfileAuthState,
    profile: &GatewayProfile,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    subject: Option<&AuthenticatedSubject>,
    started_at: Instant,
) -> anyhow::Result<()> {
    let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let principal = subject.map(|value| value.principal.id.clone());
    let tenant = subject.and_then(|value| value.principal.tenant.clone());
    let token_issuer = subject.map(|value| value.access_token.issuer.clone());
    let token_subject = subject.map(|value| value.access_token.subject.clone());
    let jwt_id = subject.and_then(|value| value.access_token.jwt_id.clone());
    let latency_ms = u64::try_from(started_at.elapsed().as_millis())?;
    state
        .gateway_state
        .record_auth_audit_event(&AuthAuditEvent {
            event_id,
            timestamp: Utc::now(),
            trace_id,
            profile: profile.id.clone(),
            protected_resource: profile.protected_resource.clone(),
            outcome,
            reason,
            method: AuthMethod::BearerJwt,
            principal,
            principal_attributes: subject
                .map(|value| PrincipalAuditAttributes::from(&value.principal)),
            tenant,
            token_issuer,
            token_subject,
            jwt_id,
            latency_ms: Some(latency_ms),
            metadata: subject
                .map(|value| principal_audit_metadata(&value.principal))
                .unwrap_or_default(),
        })
        .await
}

pub(super) struct AuthAuditRecord<'a> {
    pub(super) authorization_server: Option<&'a ResourceAuthorizationServer>,
    pub(super) client_id: Option<&'a OAuthClientId>,
    pub(super) principal: Option<&'a Principal>,
    pub(super) jwt_id: Option<&'a JwtId>,
    pub(super) outcome: AuthOutcome,
    pub(super) reason: AuthReasonCode,
    pub(super) started_at: Instant,
}

pub(super) async fn record_token_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let token_issuer = record
        .authorization_server
        .map(|value| value.issuer.clone());
    let token_subject = record
        .client_id
        .map(|value| TokenSubject::new(value.as_str()))
        .transpose()?;
    let principal = match (record.authorization_server, record.client_id) {
        (Some(authorization_server), Some(client_id)) => Some(PrincipalId::new(format!(
            "{}#{}",
            authorization_server.issuer, client_id
        ))?),
        _ => None,
    };
    let latency_ms = u64::try_from(record.started_at.elapsed().as_millis())?;
    gateway_state
        .record_auth_audit_event(&AuthAuditEvent {
            event_id,
            timestamp: Utc::now(),
            trace_id,
            profile: profile.id.clone(),
            protected_resource: profile.protected_resource.clone(),
            outcome: record.outcome,
            reason: record.reason,
            method: AuthMethod::ClientCredentialsPrivateKeyJwt,
            principal,
            principal_attributes: record.principal.map(PrincipalAuditAttributes::from),
            tenant: None,
            token_issuer,
            token_subject,
            jwt_id: record.jwt_id.cloned(),
            latency_ms: Some(latency_ms),
            metadata: record
                .principal
                .map(principal_audit_metadata)
                .unwrap_or_default(),
        })
        .await
}

pub(super) async fn record_id_jag_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let token_issuer = record
        .authorization_server
        .map(|value| value.issuer.clone());
    let token_subject = match (record.principal, record.client_id) {
        (Some(principal), _) => Some(principal.subject.clone()),
        (None, Some(client_id)) => Some(TokenSubject::new(client_id.as_str())?),
        (None, None) => None,
    };
    let principal_id = match (
        record.principal,
        record.authorization_server,
        record.client_id,
    ) {
        (Some(principal), _, _) => Some(principal.id.clone()),
        (None, Some(authorization_server), Some(client_id)) => Some(PrincipalId::new(format!(
            "{}#{}",
            authorization_server.issuer, client_id
        ))?),
        _ => None,
    };
    let tenant = record.principal.and_then(|value| value.tenant.clone());
    let latency_ms = u64::try_from(record.started_at.elapsed().as_millis())?;
    gateway_state
        .record_auth_audit_event(&AuthAuditEvent {
            event_id,
            timestamp: Utc::now(),
            trace_id,
            profile: profile.id.clone(),
            protected_resource: profile.protected_resource.clone(),
            outcome: record.outcome,
            reason: record.reason,
            method: AuthMethod::EnterpriseManagedIdJag,
            principal: principal_id,
            principal_attributes: record.principal.map(PrincipalAuditAttributes::from),
            tenant,
            token_issuer,
            token_subject,
            jwt_id: record.jwt_id.cloned(),
            latency_ms: Some(latency_ms),
            metadata: record
                .principal
                .map(principal_audit_metadata)
                .unwrap_or_default(),
        })
        .await
}

pub(super) async fn record_oidc_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    record_user_grant_auth_audit(
        gateway_state,
        profile,
        record,
        AuthMethod::OidcAuthorizationCodePkce,
    )
    .await
}

pub(super) async fn record_refresh_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    let event = refresh_auth_audit_event(profile, record)?;
    gateway_state.record_auth_audit_event(&event).await
}

pub(super) fn refresh_auth_audit_event(
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<AuthAuditEvent> {
    user_grant_auth_audit_event(profile, record, AuthMethod::RefreshToken)
}

async fn record_user_grant_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
    method: AuthMethod,
) -> anyhow::Result<()> {
    let event = user_grant_auth_audit_event(profile, record, method)?;
    gateway_state.record_auth_audit_event(&event).await
}

fn user_grant_auth_audit_event(
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
    method: AuthMethod,
) -> anyhow::Result<AuthAuditEvent> {
    let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let token_issuer = record
        .authorization_server
        .map(|value| value.issuer.clone());
    let token_subject = match (record.principal, record.client_id) {
        (Some(principal), _) => Some(principal.subject.clone()),
        (None, Some(client_id)) => Some(TokenSubject::new(client_id.as_str())?),
        (None, None) => None,
    };
    let principal_id = match (
        record.principal,
        record.authorization_server,
        record.client_id,
    ) {
        (Some(principal), _, _) => Some(principal.id.clone()),
        (None, Some(authorization_server), Some(client_id)) => Some(PrincipalId::new(format!(
            "{}#{}",
            authorization_server.issuer, client_id
        ))?),
        _ => None,
    };
    let tenant = record.principal.and_then(|value| value.tenant.clone());
    let latency_ms = u64::try_from(record.started_at.elapsed().as_millis())?;
    Ok(AuthAuditEvent {
        event_id,
        timestamp: Utc::now(),
        trace_id,
        profile: profile.id.clone(),
        protected_resource: profile.protected_resource.clone(),
        outcome: record.outcome,
        reason: record.reason,
        method,
        principal: principal_id,
        principal_attributes: record.principal.map(PrincipalAuditAttributes::from),
        tenant,
        token_issuer,
        token_subject,
        jwt_id: record.jwt_id.cloned(),
        latency_ms: Some(latency_ms),
        metadata: record
            .principal
            .map(principal_audit_metadata)
            .unwrap_or_default(),
    })
}

pub(super) fn auth_audit_error_response(err: anyhow::Error) -> Response {
    tracing::error!("failed to record gateway auth audit event: {err}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

pub(super) fn internal_error_response(err: impl std::fmt::Display) -> Response {
    tracing::error!("gateway internal error: {err}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

pub(super) fn unauthorized(
    state: &ProfileAuthState,
    profile: &GatewayProfile,
    reason: &'static str,
) -> Response {
    let metadata_url = format!(
        "{}/.well-known/oauth-protected-resource/mcp/{}",
        state.public_base_url, profile.id
    );
    let challenge = www_authenticate_challenge(&metadata_url, &profile.required_scopes);
    let Ok(challenge) = HeaderValue::from_str(&challenge) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut headers = HeaderMap::new();
    headers.insert(WWW_AUTHENTICATE, challenge);
    tracing::debug!(profile = %profile.id, reason, "gateway authorization challenge");
    (
        StatusCode::UNAUTHORIZED,
        headers,
        "authorization required for gateway profile",
    )
        .into_response()
}
