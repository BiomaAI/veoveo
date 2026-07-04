use std::time::Instant;

use axum::{Form, extract::State, http::StatusCode};
use veoveo_mcp_contract::{AuthOutcome, AuthReasonCode, GatewayProfile, OAuthClientId};
use veoveo_mcp_gateway::GatewayCatalog;

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
    Form(request): Form<TokenRequest>,
) -> axum::response::Response {
    let started_at = Instant::now();
    let catalog = current_catalog(&state.catalog);
    let profile = match resolve_oauth_profile(
        &catalog,
        request.client_id.as_str(),
        request.resource.as_deref(),
    ) {
        Ok(profile) => profile,
        Err(response) => return response,
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

pub(super) fn resolve_oauth_profile<'a>(
    catalog: &'a GatewayCatalog,
    raw_client_id: &str,
    raw_resource: Option<&str>,
) -> Result<&'a GatewayProfile, axum::response::Response> {
    let client_id = OAuthClientId::new(raw_client_id.trim()).map_err(|_| {
        oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client is not registered for this authorization server",
        )
    })?;
    let client = catalog.oauth_client(&client_id).ok_or_else(|| {
        oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client is not registered for this authorization server",
        )
    })?;
    let profile = match raw_resource
        .map(str::trim)
        .filter(|resource| !resource.is_empty())
    {
        Some(resource) => catalog
            .profile_by_protected_resource(resource)
            .ok_or_else(|| {
                oauth_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_target",
                    "requested resource is not a registered gateway profile",
                )
            })?,
        None => catalog.client_single_profile(client).ok_or_else(|| {
            oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "resource is required when an OAuth client can access multiple profiles",
            )
        })?,
    };
    if client.authorization_server != profile.authorization_server
        || !client.allowed_profiles.contains(&profile.id)
    {
        return Err(oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client is not allowed for this gateway profile",
        ));
    }
    Ok(profile)
}
