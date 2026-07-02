use std::time::Instant;

use axum::{http::StatusCode, response::IntoResponse};
use chrono::Utc;
use veoveo_mcp_contract::{
    AuthMode, AuthOutcome, AuthReasonCode, GatewayProfile, OAuthClientAuthMethod, OAuthClientId,
    OAuthGrantType, ResourceAuthorizationServer,
};
use veoveo_mcp_gateway::{ClientAssertionConfig, ClientAssertionVerifier, GatewayCatalog};

use crate::{
    audit::{AuthAuditRecord, auth_audit_error_response, record_token_auth_audit},
    http_util::{
        TokenResponse, allowed_gateway_jwt_algorithms, load_jwks, oauth_error_response,
        token_response,
    },
    oauth_grants::{TokenRequest, requested_token_scopes},
    runtime::{AppState, current_http_client},
    tokens::{ACCESS_TOKEN_TTL_SECONDS, issue_client_credentials_access_token},
};

const CLIENT_ASSERTION_TYPE_JWT_BEARER: &str =
    "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

pub(super) async fn token_endpoint_client_credentials(
    state: &AppState,
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    request: TokenRequest,
    started_at: Instant,
) -> axum::response::Response {
    if request.grant_type != "client_credentials"
        || !profile
            .auth_modes
            .contains(&AuthMode::OAuthClientCredentials)
    {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: None,
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnsupportedGrantType,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "grant type is not supported for this gateway profile",
        );
    }

    let client_id = match OAuthClientId::new(request.client_id.trim()) {
        Ok(client_id) => client_id,
        Err(_) => {
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: None,
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidClient,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication failed",
            );
        }
    };
    let Some(client) = catalog.oauth_client(&client_id) else {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    };
    if client.authorization_server != profile.authorization_server
        || !client.allowed_profiles.contains(&profile.id)
        || !client
            .grant_types
            .contains(&OAuthGrantType::ClientCredentials)
        || !client
            .auth_methods
            .contains(&OAuthClientAuthMethod::PrivateKeyJwt)
    {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    }

    if request.client_assertion_type.as_deref() != Some(CLIENT_ASSERTION_TYPE_JWT_BEARER) {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClientAssertion,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    }
    let Some(assertion) = request.client_assertion.as_deref() else {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClientAssertion,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    };

    let Some(client_jwks_source) = client.jwks.as_ref() else {
        tracing::error!(client = %client_id, "private-key JWT client is missing JWKS source");
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidAuthConfig,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "client authentication is not configured",
        );
    };
    let http = current_http_client(&state.http);
    let client_jwks = match load_jwks(&http, client_jwks_source).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!(client = %client_id, "failed to load OAuth client JWKS: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::AuthorizationServerUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication failed",
            );
        }
    };
    let assertion_config = match ClientAssertionConfig::new(
        client_id.clone(),
        authorization_server.token_endpoint.as_str(),
        allowed_gateway_jwt_algorithms(),
    ) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("invalid client assertion verifier config: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthConfig,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "client authentication is not configured",
            );
        }
    };
    let verified_assertion =
        match ClientAssertionVerifier::new(assertion_config, client_jwks).verify(assertion) {
            Ok(verified) => verified,
            Err(err) => {
                tracing::warn!(client = %client_id, "rejected OAuth client assertion: {err}");
                if let Err(err) = record_token_auth_audit(
                    &state.gateway_state,
                    profile,
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: None,
                        jwt_id: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::InvalidClientAssertion,
                        started_at,
                    },
                ) {
                    return auth_audit_error_response(err);
                }
                return oauth_error_response(
                    StatusCode::UNAUTHORIZED,
                    "invalid_client",
                    "client authentication failed",
                );
            }
        };
    match state.gateway_state.record_client_assertion_jti(
        &authorization_server.id,
        &client_id,
        &verified_assertion.jwt_id,
        verified_assertion.expires_at,
        Utc::now(),
    ) {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(
                client = %client_id,
                jwt_id = %verified_assertion.jwt_id,
                "rejected replayed OAuth client assertion"
            );
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: Some(&verified_assertion.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::ClientAssertionReplay,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication failed",
            );
        }
        Err(err) => {
            tracing::error!("failed to record OAuth client assertion replay state: {err}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let scopes = match requested_token_scopes(catalog, profile, client, request.scope.as_deref()) {
        Ok(scopes) => scopes,
        Err(err) => {
            tracing::warn!(client = %client_id, "rejected token scope request: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: Some(&verified_assertion.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidScope,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_scope",
                "requested scope is not allowed",
            );
        }
    };

    let token = match issue_client_credentials_access_token(
        catalog,
        authorization_server,
        profile,
        &client_id,
        &scopes,
    )
    .await
    {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("failed to issue client credentials access token: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: Some(&verified_assertion.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::TokenSigningKeyUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "token signing key is unavailable",
            );
        }
    };
    if let Err(err) = record_token_auth_audit(
        &state.gateway_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: None,
            jwt_id: Some(&token.jwt_id),
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
    ) {
        return auth_audit_error_response(err);
    }

    token_response(TokenResponse {
        access_token: token.access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECONDS as u64,
        scope: scopes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" "),
    })
}
