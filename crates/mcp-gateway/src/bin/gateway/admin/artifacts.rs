use std::{collections::BTreeMap, time::Instant};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    AccessLevel, ArtifactId, ArtifactPlane, ArtifactPlaneError, ArtifactShareLinkId,
    CreateArtifactShareLinkRequest, GatewayAction, GatewayProfile, PlaneCaller, PolicyTarget,
    PutGrantRequest, ResourceUri, SetArtifactReleaseStateRequest, Subject,
};
use veoveo_mcp_gateway::AuthenticatedSubject;

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

    const fn method(self) -> &'static str {
        match self {
            Self::SetReleaseState => "admin/artifacts/release-state",
            Self::Grant => "admin/artifacts/grants",
            Self::RevokeGrant => "admin/artifacts/grants/revoke",
            Self::CreateShareLink => "admin/artifacts/share-links",
            Self::RevokeShareLink => "admin/artifacts/share-links/revoke",
        }
    }

    const fn result_method(self) -> &'static str {
        match self {
            Self::SetReleaseState => "admin/artifacts/release-state/result",
            Self::Grant => "admin/artifacts/grants/result",
            Self::RevokeGrant => "admin/artifacts/grants/revoke/result",
            Self::CreateShareLink => "admin/artifacts/share-links/result",
            Self::RevokeShareLink => "admin/artifacts/share-links/revoke/result",
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
    metadata: BTreeMap<String, String>,
}

pub(crate) async fn set_artifact_release_state(
    State(state): State<AdminState>,
    AxumPath((profile, artifact_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<SetArtifactReleaseStateRequest>,
) -> Response {
    let started_at = Instant::now();
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "release_state".to_owned(),
        release_state_name(request.release_state).to_owned(),
    );
    let context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::SetReleaseState,
        metadata,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
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
    let metadata = grant_metadata(&request.subject, request.level);
    let context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::Grant,
        metadata,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
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
    Json(grant_subject): Json<Subject>,
) -> Response {
    let started_at = Instant::now();
    let metadata = grant_subject_metadata(&grant_subject);
    let context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::RevokeGrant,
        metadata,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
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
    let mut metadata = BTreeMap::new();
    if let Some(expires_at) = request.expires_at {
        metadata.insert("expires_at".to_owned(), expires_at.to_rfc3339());
    }
    if let Some(max_downloads) = request.max_downloads {
        metadata.insert("max_downloads".to_owned(), max_downloads.to_string());
    }
    let mut context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::CreateShareLink,
        metadata,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
    };
    let result = context
        .plane
        .create_share_link(&context.caller, &context.artifact_id, request)
        .await;
    match result {
        Ok(link) => {
            context
                .metadata
                .insert("link_id".to_owned(), link.link_id.to_string());
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
    let metadata = BTreeMap::from([("link_id".to_owned(), link_id.to_string())]);
    let context = match authorize_artifact_operation(
        &state,
        profile,
        artifact_id,
        subject,
        ArtifactOperation::RevokeShareLink,
        metadata,
        started_at,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
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
    mut metadata: BTreeMap<String, String>,
    started_at: Instant,
) -> Result<AuthorizedArtifactOperation, Response> {
    let Some(profile_id) = admin_profile_id(profile) else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };
    let Ok(artifact_id) = ArtifactId::parse(artifact_id) else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };
    let artifact_uri = ResourceUri::new(artifact_id.plane_uri())
        .map_err(|_| StatusCode::NOT_FOUND.into_response())?;
    let target = PolicyTarget::Artifact {
        server: state.artifact_server.clone(),
        artifact_uri,
    };
    metadata.insert("operation".to_owned(), operation.name().to_owned());
    metadata.insert("artifact_id".to_owned(), artifact_id.to_string());
    let (_catalog, profile, subject) = authorize_admin_target_request(
        state,
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            action: GatewayAction::AdminWrite,
            target: target.clone(),
            method: operation.method(),
            metadata: metadata.clone(),
            started_at,
        },
    )
    .await
    .map_err(|response| *response)?;
    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_ARTIFACT_TOKEN_TTL_SECONDS),
    );
    let internal_token = match state.internal_token_issuer.issue(
        profile_id,
        state.artifact_server.clone(),
        subject.principal.clone(),
        expires_at,
    ) {
        Ok(token) => token,
        Err(error) => {
            tracing::error!("failed to issue artifact service identity: {error}");
            if let Err(audit_error) = record_artifact_operation(
                state,
                &profile,
                &subject,
                target,
                operation,
                started_at,
                AdminOperationStatus::Failed,
                Some(AdminOperationFailure::IssueInternalToken),
                metadata,
            )
            .await
            {
                return Err(internal_error_response(audit_error));
            }
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    let memberships = internal_token.identity.principal.group_memberships();
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
        metadata,
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
        context.target.clone(),
        context.operation,
        started_at,
        status,
        failure,
        context.metadata.clone(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn record_artifact_operation(
    state: &AdminState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    target: PolicyTarget,
    operation: ArtifactOperation,
    started_at: Instant,
    status: AdminOperationStatus,
    failure: Option<AdminOperationFailure>,
    metadata: BTreeMap<String, String>,
) -> anyhow::Result<()> {
    record_admin_target_operation_audit(
        state,
        profile,
        subject,
        target,
        AdminOperationAuditRecord {
            action: GatewayAction::AdminWrite,
            method: operation.result_method(),
            started_at,
            status,
            failure,
            metadata,
        },
    )
    .await
}

fn grant_metadata(subject: &Subject, level: AccessLevel) -> BTreeMap<String, String> {
    let mut metadata = grant_subject_metadata(subject);
    metadata.insert(
        "grant_level".to_owned(),
        access_level_name(level).to_owned(),
    );
    metadata
}

fn grant_subject_metadata(subject: &Subject) -> BTreeMap<String, String> {
    let (kind, id) = match subject {
        Subject::User(id) => ("user", id.to_string()),
        Subject::Group(id) => ("group", id.to_string()),
    };
    BTreeMap::from([
        ("grant_subject_kind".to_owned(), kind.to_owned()),
        ("grant_subject".to_owned(), id),
    ])
}

const fn access_level_name(level: AccessLevel) -> &'static str {
    match level {
        AccessLevel::Read => "read",
        AccessLevel::Write => "write",
        AccessLevel::Admin => "admin",
    }
}

const fn release_state_name(state: veoveo_mcp_contract::ArtifactReleaseState) -> &'static str {
    match state {
        veoveo_mcp_contract::ArtifactReleaseState::Private => "private",
        veoveo_mcp_contract::ArtifactReleaseState::Releasable => "releasable",
        veoveo_mcp_contract::ArtifactReleaseState::Released => "released",
    }
}
