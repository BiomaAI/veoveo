use std::time::Instant;
use veoveo_mcp_contract::audit::{AdministrativeOperation, AuditTarget};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_contract::{ArtifactId, ArtifactShareLinkId};
use veoveo_mcp_contract::{
    ArtifactPlane, ArtifactPlaneError, CreateArtifactShareLinkRequest, GatewayAction,
    GatewayProfile, PlaneCaller, PolicyTarget, PutGrantRequest, SetArtifactReleaseStateRequest,
};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_types::{AccessSubject, ResourceUri};

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminAuthorizationRequest, AdminOperationAuditRecord, AdminOperationFailure,
        AdminOperationStatus, authorize_admin_target_request, internal_error_response,
        record_admin_target_operation_audit,
    },
    runtime::{AdminState, current_http_client},
};

const INTERNAL_ARTIFACT_TOKEN_TTL_SECONDS: i64 = 60;

#[derive(Clone, Copy, Debug)]
enum ArtifactOperation {
    SetReleaseState,
    Grant,
    RevokeGrant,
    CreateShareLink,
    RevokeShareLink,
}

impl ArtifactOperation {
    const fn name(self) -> &'static str {
        match self {
            Self::SetReleaseState => "set_release_state",
            Self::Grant => "grant_artifact",
            Self::RevokeGrant => "revoke_artifact_grant",
            Self::CreateShareLink => "create_artifact_share_link",
            Self::RevokeShareLink => "revoke_artifact_share_link",
        }
    }

    const fn audit_operation(self) -> AdministrativeOperation {
        match self {
            Self::SetReleaseState => AdministrativeOperation::ArtifactRelease,
            Self::Grant => AdministrativeOperation::ArtifactGrant,
            Self::RevokeGrant => AdministrativeOperation::ArtifactRevoke,
            Self::CreateShareLink => AdministrativeOperation::ArtifactShare,
            Self::RevokeShareLink => AdministrativeOperation::ArtifactUnshare,
        }
    }

    const fn failure(self) -> AdminOperationFailure {
        match self {
            Self::SetReleaseState => AdminOperationFailure::ArtifactReleaseState,
            Self::Grant => AdminOperationFailure::ArtifactGrant,
            Self::RevokeGrant => AdminOperationFailure::ArtifactGrantRevoke,
            Self::CreateShareLink => AdminOperationFailure::ArtifactShareLink,
            Self::RevokeShareLink => AdminOperationFailure::ArtifactShareLinkRevoke,
        }
    }
}

struct AuthorizedArtifactOperation {
    artifact_id: ArtifactId,
    plane: HttpArtifactPlane,
    caller: PlaneCaller,
    profile: GatewayProfile,
    subject: AuthenticatedSubject,
    target: PolicyTarget,
    operation: ArtifactOperation,
}

pub(crate) async fn set_artifact_release_state(
    State(state): State<AdminState>,
    AxumPath((profile, artifact_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<SetArtifactReleaseStateRequest>,
) -> Response {
    let started_at = Instant::now();
    let context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::SetReleaseState,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return *response,
    };
    let result = context
        .plane
        .set_release_state(&context.caller, &context.artifact_id, request.release_state)
        .await;
    match result {
        Ok(metadata) => {
            if let Err(error) = record_artifact_result(
                &state,
                &context,
                started_at,
                AdminOperationStatus::Succeeded,
                None,
            )
            .await
            {
                return internal_error_response(error);
            }
            Json(metadata).into_response()
        }
        Err(error) => artifact_error_response(&state, context, started_at, error).await,
    }
}

pub(crate) async fn grant_artifact(
    State(state): State<AdminState>,
    AxumPath((profile, artifact_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<PutGrantRequest>,
) -> Response {
    let started_at = Instant::now();
    let context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::Grant,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return *response,
    };
    let result = context
        .plane
        .grant(
            &context.caller,
            &context.artifact_id,
            request.subject,
            request.level,
        )
        .await;
    artifact_empty_result(&state, context, started_at, result).await
}

pub(crate) async fn revoke_artifact_grant(
    State(state): State<AdminState>,
    AxumPath((profile, artifact_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(grant_subject): Json<AccessSubject>,
) -> Response {
    let started_at = Instant::now();
    let context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::RevokeGrant,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return *response,
    };
    let result = context
        .plane
        .revoke(&context.caller, &context.artifact_id, &grant_subject)
        .await;
    artifact_empty_result(&state, context, started_at, result).await
}

pub(crate) async fn create_artifact_share_link(
    State(state): State<AdminState>,
    AxumPath((profile, artifact_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<CreateArtifactShareLinkRequest>,
) -> Response {
    let started_at = Instant::now();
    let context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::CreateShareLink,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return *response,
    };
    let result = context
        .plane
        .create_share_link(&context.caller, &context.artifact_id, request)
        .await;
    match result {
        Ok(link) => {
            if let Err(error) = record_artifact_result(
                &state,
                &context,
                started_at,
                AdminOperationStatus::Succeeded,
                None,
            )
            .await
            {
                return internal_error_response(error);
            }
            (StatusCode::CREATED, Json(link)).into_response()
        }
        Err(error) => artifact_error_response(&state, context, started_at, error).await,
    }
}

pub(crate) async fn revoke_artifact_share_link(
    State(state): State<AdminState>,
    AxumPath((profile, artifact_id, link_id)): AxumPath<(String, String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Ok(link_id) = ArtifactShareLinkId::parse(link_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::RevokeShareLink,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return *response,
    };
    let result = context
        .plane
        .revoke_share_link(&context.caller, &context.artifact_id, &link_id)
        .await;
    artifact_empty_result(&state, context, started_at, result).await
}

#[allow(clippy::too_many_arguments)]
async fn authorize_artifact_operation(
    state: &AdminState,
    profile: String,
    artifact_id: String,
    subject: AuthenticatedSubject,
    operation: ArtifactOperation,

    started_at: Instant,
) -> Result<AuthorizedArtifactOperation, Box<Response>> {
    let Some(profile_id) = admin_profile_id(profile) else {
        return Err(StatusCode::NOT_FOUND.into_response().into());
    };
    let Ok(artifact_id) = ArtifactId::parse(artifact_id) else {
        return Err(StatusCode::NOT_FOUND.into_response().into());
    };
    let artifact_uri = ResourceUri::new(artifact_id.plane_uri())
        .map_err(|_| StatusCode::NOT_FOUND.into_response())?;
    let target = PolicyTarget::Artifact {
        server: state.artifact_server.clone(),
        artifact_uri,
    };
    let (_catalog, profile, subject) = authorize_admin_target_request(
        state,
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            audit_target: Some(AuditTarget::Artifact {
                artifact: artifact_id,
            }),
            action: GatewayAction::AdminWrite,
            target: target.clone(),
            operation: operation.audit_operation(),
            started_at,
        },
    )
    .await?;
    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_ARTIFACT_TOKEN_TTL_SECONDS),
    );
    let internal_token = match state.internal_token_issuer.issue(
        profile_id,
        state.artifact_server.clone(),
        subject.actor.clone(),
        subject.authority.clone(),
        Some(subject.request_context()),
        expires_at,
    ) {
        Ok(token) => token,
        Err(error) => {
            tracing::error!("failed to issue artifact service identity: {error}");
            if let Err(audit_error) = record_artifact_operation(
                state,
                &profile,
                &subject,
                artifact_id,
                target,
                operation,
                started_at,
                AdminOperationStatus::Failed,
                Some(AdminOperationFailure::IssueInternalToken),
            )
            .await
            {
                return Err(internal_error_response(audit_error).into());
            }
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response().into());
        }
    };
    let memberships = internal_token.identity.actor.group_memberships();
    let caller = PlaneCaller {
        bearer_token: internal_token.bearer_token,
        identity: internal_token.identity,
        memberships,
    };
    Ok(AuthorizedArtifactOperation {
        artifact_id,
        plane: HttpArtifactPlane::with_client(
            &state.artifact_service_url,
            current_http_client(&state.http),
        ),
        caller,
        profile,
        subject,
        target,
        operation,
    })
}

async fn artifact_empty_result(
    state: &AdminState,
    context: AuthorizedArtifactOperation,
    started_at: Instant,
    result: Result<(), ArtifactPlaneError>,
) -> Response {
    match result {
        Ok(()) => {
            if let Err(error) = record_artifact_result(
                state,
                &context,
                started_at,
                AdminOperationStatus::Succeeded,
                None,
            )
            .await
            {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => artifact_error_response(state, context, started_at, error).await,
    }
}

async fn artifact_error_response(
    state: &AdminState,
    context: AuthorizedArtifactOperation,
    started_at: Instant,
    error: ArtifactPlaneError,
) -> Response {
    let (status, operation_status) = match error {
        ArtifactPlaneError::NotFound => (StatusCode::NOT_FOUND, AdminOperationStatus::Rejected),
        ArtifactPlaneError::Denied(_) => (StatusCode::FORBIDDEN, AdminOperationStatus::Rejected),
        ArtifactPlaneError::InvalidRequest(_) => {
            (StatusCode::BAD_REQUEST, AdminOperationStatus::Rejected)
        }
        ArtifactPlaneError::Conflict(_) => (StatusCode::CONFLICT, AdminOperationStatus::Rejected),
        ArtifactPlaneError::Unauthenticated | ArtifactPlaneError::Transport(_) => {
            (StatusCode::BAD_GATEWAY, AdminOperationStatus::Failed)
        }
    };
    tracing::warn!(
        artifact_id = %context.artifact_id,
        operation = context.operation.name(),
        "artifact service operation failed: {error}"
    );
    if let Err(audit_error) = record_artifact_result(
        state,
        &context,
        started_at,
        operation_status,
        Some(context.operation.failure()),
    )
    .await
    {
        return internal_error_response(audit_error);
    }
    status.into_response()
}

async fn record_artifact_result(
    state: &AdminState,
    context: &AuthorizedArtifactOperation,
    started_at: Instant,
    status: AdminOperationStatus,
    failure: Option<AdminOperationFailure>,
) -> anyhow::Result<()> {
    record_artifact_operation(
        state,
        &context.profile,
        &context.subject,
        context.artifact_id,
        context.target.clone(),
        context.operation,
        started_at,
        status,
        failure,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn record_artifact_operation(
    state: &AdminState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    artifact_id: ArtifactId,
    target: PolicyTarget,
    operation: ArtifactOperation,
    started_at: Instant,
    status: AdminOperationStatus,
    failure: Option<AdminOperationFailure>,
) -> anyhow::Result<()> {
    record_admin_target_operation_audit(
        state,
        profile,
        subject,
        target,
        AdminOperationAuditRecord {
            audit_target: Some(AuditTarget::Artifact {
                artifact: artifact_id,
            }),
            action: GatewayAction::AdminWrite,
            operation: operation.audit_operation(),
            started_at,
            status,
            failure,
        },
    )
    .await
}
