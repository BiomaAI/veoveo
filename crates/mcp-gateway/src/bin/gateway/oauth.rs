use std::time::Instant;

use axum::{
    Form,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::IntoResponse,
};
use veoveo_mcp_contract::{AuthOutcome, AuthReasonCode, GatewayProfileId};

use crate::{
    audit::{AuthAuditRecord, auth_audit_error_response, record_token_auth_audit},
    http_util::oauth_error_response,
    oauth_client_credentials::token_endpoint_client_credentials,
    oauth_grants::{TokenRequest, token_endpoint_authorization_code, token_endpoint_id_jag},
    runtime::{AppState, current_catalog},
};

#[path = "oauth/authorize.rs"]
mod authorize;
#[path = "oauth/callback.rs"]
mod callback;

pub(super) use authorize::authorize_endpoint;
pub(super) use callback::authorization_callback;

const JWT_BEARER_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

pub(super) async fn token_endpoint(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
    Form(request): Form<TokenRequest>,
) -> axum::response::Response {
    let started_at = Instant::now();
    let Ok(profile_id) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(&profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(authorization_server) = catalog.authorization_server(&profile.authorization_server)
    else {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: None,
                client_id: None,
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnknownAuthorizationServer,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "authorization server is unavailable",
        );
    };

    if request.grant_type == JWT_BEARER_GRANT_TYPE {
        return token_endpoint_id_jag(
            &state,
            &catalog,
            profile,
            authorization_server,
            request,
            started_at,
        )
        .await;
    }

    if request.grant_type == "authorization_code" {
        return token_endpoint_authorization_code(
            &state,
            &catalog,
            profile,
            authorization_server,
            request,
            started_at,
        )
        .await;
    }

    token_endpoint_client_credentials(
        &state,
        &catalog,
        profile,
        authorization_server,
        request,
        started_at,
    )
    .await
}
