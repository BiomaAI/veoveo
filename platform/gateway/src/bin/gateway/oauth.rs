use std::time::Instant;

use axum::{Form, extract::State, http::StatusCode};
use std::collections::BTreeSet;

use veoveo_mcp_contract::{
    AuthOutcome, AuthReasonCode, AuthorizationServerId, GatewayProfile, OAuthClientId,
    ProtectedResourceId, RecordingIngestResource, ScopeName,
};
use veoveo_mcp_gateway::GatewayCatalog;

use crate::{
    audit::{AuthAuditRecord, AuthAuditTarget, auth_audit_error_response, record_token_auth_audit},
    http_util::oauth_error_response,
    oauth_client_credentials::token_endpoint_client_credentials,
    oauth_grants::{
        TokenRequest, token_endpoint_authorization_code, token_endpoint_id_jag,
        token_endpoint_refresh_token,
    },
    runtime::{AppState, current_catalog},
};

#[path = "oauth/authorize.rs"]
mod authorize;
#[path = "oauth/callback.rs"]
mod callback;
#[path = "oauth/revoke.rs"]
mod revoke;

pub(super) use authorize::authorize_endpoint;
pub(super) use callback::authorization_callback;
pub(super) use revoke::revoke_refresh_token;

const JWT_BEARER_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

pub(super) async fn token_endpoint(
    State(state): State<AppState>,
    Form(request): Form<TokenRequest>,
) -> axum::response::Response {
    let started_at = Instant::now();
    let catalog = current_catalog(&state.catalog);
    let resource = match resolve_oauth_resource(
        &catalog,
        request.client_id.as_str(),
        request.resource.as_deref(),
    ) {
        Ok(resource) => resource,
        Err(response) => return *response,
    };
    let Some(authorization_server) = catalog.authorization_server(resource.authorization_server())
    else {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            resource.audit_target(),
            AuthAuditRecord {
                authorization_server: None,
                client_id: None,
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnknownAuthorizationServer,
                started_at,
            },
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "authorization server is unavailable",
        );
    };

    if request.grant_type == JWT_BEARER_GRANT_TYPE {
        let Some(profile) = resource.profile() else {
            return unsupported_resource_grant(&state, resource, authorization_server, started_at)
                .await;
        };
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
        let Some(profile) = resource.profile() else {
            return unsupported_resource_grant(&state, resource, authorization_server, started_at)
                .await;
        };
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

    if request.grant_type == "refresh_token" {
        let Some(profile) = resource.profile() else {
            return unsupported_resource_grant(&state, resource, authorization_server, started_at)
                .await;
        };
        return token_endpoint_refresh_token(
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
        resource,
        authorization_server,
        request,
        started_at,
    )
    .await
}

#[derive(Clone, Copy)]
pub(super) enum ResolvedOAuthResource<'a> {
    Profile(&'a GatewayProfile),
    RecordingIngest(&'a RecordingIngestResource),
}

impl<'a> ResolvedOAuthResource<'a> {
    pub(super) fn profile(self) -> Option<&'a GatewayProfile> {
        match self {
            Self::Profile(profile) => Some(profile),
            Self::RecordingIngest(_) => None,
        }
    }

    pub(super) fn protected_resource(self) -> &'a ProtectedResourceId {
        match self {
            Self::Profile(profile) => &profile.protected_resource,
            Self::RecordingIngest(resource) => &resource.protected_resource,
        }
    }

    pub(super) fn authorization_server(self) -> &'a AuthorizationServerId {
        match self {
            Self::Profile(profile) => &profile.authorization_server,
            Self::RecordingIngest(resource) => &resource.authorization_server,
        }
    }

    pub(super) fn supported_scopes(self, catalog: &GatewayCatalog) -> BTreeSet<ScopeName> {
        match self {
            Self::Profile(profile) => catalog.profile_supported_scopes(profile),
            Self::RecordingIngest(resource) => resource.required_scopes.clone(),
        }
    }

    pub(super) fn supports_client_credentials(self) -> bool {
        match self {
            Self::Profile(profile) => profile
                .auth_modes
                .contains(&veoveo_mcp_contract::AuthMode::OAuthClientCredentials),
            Self::RecordingIngest(_) => true,
        }
    }

    pub(super) fn audit_target(self) -> AuthAuditTarget<'a> {
        match self {
            Self::Profile(profile) => AuthAuditTarget::from(profile),
            Self::RecordingIngest(resource) => AuthAuditTarget {
                profile: None,
                protected_resource: &resource.protected_resource,
            },
        }
    }
}

async fn unsupported_resource_grant(
    state: &AppState,
    resource: ResolvedOAuthResource<'_>,
    authorization_server: &veoveo_mcp_contract::ResourceAuthorizationServer,
    started_at: Instant,
) -> axum::response::Response {
    if let Err(err) = record_token_auth_audit(
        &state.gateway_state,
        resource.audit_target(),
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: None,
            principal: None,
            jwt_id: None,
            outcome: AuthOutcome::Deny,
            reason: AuthReasonCode::UnsupportedGrantType,
            started_at,
        },
    )
    .await
    {
        return auth_audit_error_response(err);
    }
    oauth_error_response(
        StatusCode::BAD_REQUEST,
        "unsupported_grant_type",
        "grant type is not supported for this protected resource",
    )
}

pub(super) fn resolve_oauth_resource<'a>(
    catalog: &'a GatewayCatalog,
    raw_client_id: &str,
    raw_resource: Option<&str>,
) -> Result<ResolvedOAuthResource<'a>, Box<axum::response::Response>> {
    let client_id = OAuthClientId::new(raw_client_id.trim()).map_err(|_| invalid_client())?;
    let client = catalog
        .oauth_client(&client_id)
        .ok_or_else(invalid_client)?;
    let resource = match raw_resource
        .map(str::trim)
        .filter(|resource| !resource.is_empty())
    {
        Some(resource) => resolve_registered_resource(catalog, resource).ok_or_else(|| {
            Box::new(oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_target",
                "requested resource is not registered",
            ))
        })?,
        None => {
            if client.allowed_resources.len() != 1 {
                return Err(Box::new(oauth_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "resource is required when an OAuth client can access multiple protected resources",
                )));
            }
            let protected_resource = client
                .allowed_resources
                .iter()
                .next()
                .expect("one resource");
            resolve_registered_resource(catalog, protected_resource.as_str()).ok_or_else(|| {
                Box::new(oauth_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_target",
                    "OAuth client resource is not registered",
                ))
            })?
        }
    };
    if &client.authorization_server != resource.authorization_server()
        || !client
            .allowed_resources
            .contains(resource.protected_resource())
    {
        return Err(invalid_client());
    }
    Ok(resource)
}

fn resolve_registered_resource<'a>(
    catalog: &'a GatewayCatalog,
    resource: &str,
) -> Option<ResolvedOAuthResource<'a>> {
    catalog
        .profile_by_protected_resource(resource)
        .map(ResolvedOAuthResource::Profile)
        .or_else(|| {
            catalog
                .recording_ingest_resource_by_protected_resource(resource)
                .map(ResolvedOAuthResource::RecordingIngest)
        })
}

fn invalid_client() -> Box<axum::response::Response> {
    Box::new(oauth_error_response(
        StatusCode::UNAUTHORIZED,
        "invalid_client",
        "client is not registered for this protected resource",
    ))
}

pub(super) fn resolve_oauth_profile<'a>(
    catalog: &'a GatewayCatalog,
    raw_client_id: &str,
    raw_resource: Option<&str>,
) -> Result<&'a GatewayProfile, Box<axum::response::Response>> {
    match resolve_oauth_resource(catalog, raw_client_id, raw_resource)? {
        ResolvedOAuthResource::Profile(profile) => Ok(profile),
        ResolvedOAuthResource::RecordingIngest(_) => Err(Box::new(oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            "requested resource does not support interactive OAuth grants",
        ))),
    }
}
