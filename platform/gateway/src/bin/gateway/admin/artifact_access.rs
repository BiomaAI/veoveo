use std::{collections::BTreeMap, time::Instant};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use serde::Serialize;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    ArtifactAccessRequestId, ArtifactId, ArtifactPlane, ArtifactPlaneError,
    CreateArtifactAccessRequest, DecideArtifactAccessRequest, GatewayAction,
    ListArtifactAccessRequests, PlaneCaller,
};
use veoveo_mcp_gateway::AuthenticatedSubject;

use crate::{
    admin::admin_profile_id,
    audit::authorize_admin_request,
    runtime::{AdminState, current_http_client},
};

const INTERNAL_TOKEN_TTL_SECONDS: i64 = 60;

pub(crate) async fn create_artifact_access_request(
    State(state): State<AdminState>,
    AxumPath((profile, artifact_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<CreateArtifactAccessRequest>,
) -> Response {
    let Ok(artifact_id) = ArtifactId::parse(artifact_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (plane, caller) = match authorized_plane(
        &state,
        profile,
        subject,
        GatewayAction::AdminRead,
        "admin/artifact-access-requests/create",
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return response,
    };
    match plane
        .create_access_request(&caller, &artifact_id, request)
        .await
    {
        Ok(created) => (StatusCode::CREATED, Json(created)).into_response(),
        Err(error) => artifact_error(error),
    }
}

pub(crate) async fn list_artifact_access_requests(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(request): Query<ListArtifactAccessRequests>,
) -> Response {
    let (plane, caller) = match authorized_plane(
        &state,
        profile,
        subject,
        GatewayAction::AdminRead,
        "admin/artifact-access-requests/list",
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return response,
    };
    match plane.list_access_requests(&caller, request).await {
        Ok(page) => Json(page).into_response(),
        Err(error) => artifact_error(error),
    }
}

pub(crate) async fn decide_artifact_access_request(
    State(state): State<AdminState>,
    AxumPath((profile, request_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(decision): Json<DecideArtifactAccessRequest>,
) -> Response {
    let Ok(request_id) = ArtifactAccessRequestId::parse(request_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (plane, caller) = match authorized_plane(
        &state,
        profile,
        subject,
        GatewayAction::AdminWrite,
        "admin/artifact-access-requests/decision",
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return response,
    };
    match plane
        .decide_access_request(&caller, &request_id, decision)
        .await
    {
        Ok(decided) => Json(decided).into_response(),
        Err(error) => artifact_error(error),
    }
}

pub(crate) async fn cancel_artifact_access_request(
    State(state): State<AdminState>,
    AxumPath((profile, request_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let Ok(request_id) = ArtifactAccessRequestId::parse(request_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (plane, caller) = match authorized_plane(
        &state,
        profile,
        subject,
        GatewayAction::AdminRead,
        "admin/artifact-access-requests/cancel",
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return response,
    };
    match plane.cancel_access_request(&caller, &request_id).await {
        Ok(cancelled) => Json(cancelled).into_response(),
        Err(error) => artifact_error(error),
    }
}

async fn authorized_plane(
    state: &AdminState,
    profile: String,
    subject: AuthenticatedSubject,
    action: GatewayAction,
    method: &str,
) -> Result<(HttpArtifactPlane, PlaneCaller), Response> {
    let Some(profile_id) = admin_profile_id(profile) else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };
    let (_, _, subject) = authorize_admin_request(
        state,
        &profile_id,
        subject,
        action,
        method,
        BTreeMap::new(),
        Instant::now(),
    )
    .await
    .map_err(|response| *response)?;
    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_TOKEN_TTL_SECONDS),
    );
    let token = state
        .internal_token_issuer
        .issue(
            profile_id,
            state.artifact_server.clone(),
            subject.actor,
            subject.authority,
            expires_at,
        )
        .map_err(|error| {
            tracing::error!("failed to issue artifact access identity: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;
    let memberships = token.identity.actor.group_memberships();
    Ok((
        HttpArtifactPlane::with_client(
            &state.artifact_service_url,
            current_http_client(&state.http),
        ),
        PlaneCaller {
            bearer_token: token.bearer_token,
            identity: token.identity,
            memberships,
        },
    ))
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn artifact_error(error: ArtifactPlaneError) -> Response {
    let status = match error {
        ArtifactPlaneError::NotFound => StatusCode::NOT_FOUND,
        ArtifactPlaneError::Denied(_) => StatusCode::FORBIDDEN,
        ArtifactPlaneError::Unauthenticated => StatusCode::UNAUTHORIZED,
        ArtifactPlaneError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        ArtifactPlaneError::Conflict(_) => StatusCode::CONFLICT,
        ArtifactPlaneError::Transport(_) => StatusCode::BAD_GATEWAY,
    };
    (
        status,
        Json(ErrorBody {
            error: error.to_string(),
        }),
    )
        .into_response()
}
