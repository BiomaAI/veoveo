use std::{collections::BTreeSet, time::Instant};

use anyhow::anyhow;
use axum::{http::StatusCode, response::IntoResponse};
use chrono::Utc;
use serde::Deserialize;
use veoveo_mcp_contract::{
    AuthMode, AuthOutcome, AuthReasonCode, GatewayProfile, OAuthAuthorizationCode,
    OAuthClientAuthMethod, OAuthClientId, OAuthClientRegistration, OAuthGrantType,
    OAuthRedirectUri, PkceCodeChallengeMethod, PkceCodeVerifier, PrincipalKind,
    ResourceAuthorizationServer, ScopeName,
};
use veoveo_mcp_gateway::{GatewayCatalog, IdJagConfig, IdJagVerifier};

use crate::{
    audit::{
        AuthAuditRecord, auth_audit_error_response, internal_error_response,
        record_id_jag_auth_audit, record_oidc_auth_audit,
    },
    http_util::{
        TokenResponse, allowed_gateway_jwt_algorithms, load_jwks, oauth_error_response,
        pkce_s256_challenge, scope_string, token_response,
    },
    runtime::{AppState, current_http_client},
    tokens::{ACCESS_TOKEN_TTL_SECONDS, issue_access_token},
};

#[derive(Debug, Deserialize)]
pub(super) struct TokenRequest {
    pub(super) grant_type: String,
    pub(super) client_id: String,
    #[serde(default)]
    pub(super) scope: Option<String>,
    #[serde(default)]
    pub(super) code: Option<String>,
    #[serde(default)]
    pub(super) redirect_uri: Option<String>,
    #[serde(default)]
    pub(super) code_verifier: Option<String>,
    #[serde(default)]
    pub(super) client_assertion_type: Option<String>,
    #[serde(default)]
    pub(super) client_assertion: Option<String>,
    #[serde(default)]
    pub(super) assertion: Option<String>,
}

pub(super) async fn token_endpoint_authorization_code(
    state: &AppState,
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    request: TokenRequest,
    started_at: Instant,
) -> axum::response::Response {
    if !profile
        .auth_modes
        .contains(&AuthMode::OidcAuthorizationCodePkce)
    {
        if let Err(err) = record_oidc_auth_audit(
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
            "authorization code flow is not enabled for this gateway profile",
        );
    }
    let client_id = match OAuthClientId::new(request.client_id.trim()) {
        Ok(client_id) => client_id,
        Err(_) => {
            if let Err(err) = record_oidc_auth_audit(
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
        if let Err(err) = record_oidc_auth_audit(
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
    if !authorization_code_client_allowed(profile, client) {
        if let Err(err) = record_oidc_auth_audit(
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
    if request.scope.is_some() {
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
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
            "authorization-code token exchange cannot request new scopes",
        );
    }
    let code = match request
        .code
        .as_deref()
        .map(str::trim)
        .map(OAuthAuthorizationCode::new)
        .transpose()
    {
        Ok(Some(code)) => code,
        _ => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthorizationCode,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "authorization code is invalid",
            );
        }
    };
    let redirect_uri = match request
        .redirect_uri
        .as_deref()
        .map(str::trim)
        .map(OAuthRedirectUri::new)
        .transpose()
    {
        Ok(Some(redirect_uri)) => redirect_uri,
        _ => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthorizationRequest,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "redirect_uri is required",
            );
        }
    };
    let code_verifier = match request
        .code_verifier
        .as_deref()
        .map(str::trim)
        .map(PkceCodeVerifier::new)
        .transpose()
    {
        Ok(Some(code_verifier)) => code_verifier,
        _ => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidPkce,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "PKCE code_verifier is required",
            );
        }
    };
    let code_record = match state
        .gateway_state
        .consume_authorization_code(&code, Utc::now())
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthorizationCode,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "authorization code is invalid or expired",
            );
        }
        Err(err) => {
            tracing::error!("failed to consume gateway authorization code: {err}");
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::AuthStateUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let expected_challenge = match pkce_s256_challenge(&code_verifier) {
        Ok(challenge) => challenge,
        Err(err) => return internal_error_response(err),
    };
    if code_record.profile != profile.id
        || code_record.oauth_client_id != client_id
        || code_record.redirect_uri != redirect_uri
        || code_record.code_challenge_method != PkceCodeChallengeMethod::S256
        || code_record.code_challenge != expected_challenge
    {
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: Some(&code_record.principal),
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidPkce,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "authorization code binding is invalid",
        );
    }
    let token = match issue_access_token(
        catalog,
        authorization_server,
        profile,
        &code_record.principal.subject,
        PrincipalKind::User,
        Some(&code_record.principal),
        &code_record.scopes,
    )
    .await
    {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("failed to issue browser authorization-code access token: {err}");
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&code_record.principal),
                    jwt_id: None,
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
    if let Err(err) = record_oidc_auth_audit(
        &state.gateway_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: Some(&code_record.principal),
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
        scope: scope_string(&code_record.scopes),
    })
}

pub(super) async fn token_endpoint_id_jag(
    state: &AppState,
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    request: TokenRequest,
    started_at: Instant,
) -> axum::response::Response {
    if !profile
        .auth_modes
        .contains(&AuthMode::EnterpriseManagedAuthorization)
    {
        if let Err(err) = record_id_jag_auth_audit(
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
            if let Err(err) = record_id_jag_auth_audit(
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
        if let Err(err) = record_id_jag_auth_audit(
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
            .contains(&OAuthGrantType::EnterpriseManagedAuthorization)
        || !client.auth_methods.contains(&OAuthClientAuthMethod::None)
    {
        if let Err(err) = record_id_jag_auth_audit(
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

    let Some(assertion) = request.assertion.as_deref() else {
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidIdentityAssertion,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "identity assertion is required",
        );
    };
    let Some(identity_provider_id) = authorization_server.identity_provider.as_ref() else {
        tracing::error!("enterprise-managed authorization requires an identity provider");
        if let Err(err) = record_id_jag_auth_audit(
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
            "identity provider is not configured",
        );
    };
    let Some(identity_provider) = catalog.identity_provider(identity_provider_id) else {
        tracing::error!(identity_provider = %identity_provider_id, "unknown identity provider");
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnknownIdentityProvider,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "identity provider is unavailable",
        );
    };
    let http = current_http_client(&state.http);
    let idp_jwks = match load_jwks(&http, &identity_provider.jwks).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!("failed to load identity provider JWKS for ID-JAG: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::IdentityProviderUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_grant",
                "identity assertion could not be validated",
            );
        }
    };
    let id_jag_config = match IdJagConfig::new(
        identity_provider.issuer.clone(),
        authorization_server.issuer.clone(),
        profile.protected_resource.clone(),
        allowed_gateway_jwt_algorithms(),
    ) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("invalid ID-JAG verifier config: {err}");
            if let Err(err) = record_id_jag_auth_audit(
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
                "identity assertion validation is not configured",
            );
        }
    };
    let verified_id_jag = match IdJagVerifier::new(id_jag_config, idp_jwks).verify(assertion) {
        Ok(verified) => verified,
        Err(err) => {
            tracing::warn!(client = %client_id, "rejected ID-JAG: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidIdentityAssertion,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_grant",
                "identity assertion could not be validated",
            );
        }
    };
    if verified_id_jag.client_id != client_id {
        tracing::warn!(
            request_client = %client_id,
            assertion_client = %verified_id_jag.client_id,
            "rejected ID-JAG with mismatched client_id"
        );
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: Some(&verified_id_jag.principal),
                jwt_id: Some(&verified_id_jag.jwt_id),
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidIdentityAssertion,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_grant",
            "identity assertion could not be validated",
        );
    }
    match state.gateway_state.record_id_jag_jti(
        &authorization_server.id,
        &client_id,
        &verified_id_jag.jwt_id,
        verified_id_jag.expires_at,
        Utc::now(),
    ) {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(
                client = %client_id,
                jwt_id = %verified_id_jag.jwt_id,
                "rejected replayed ID-JAG"
            );
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&verified_id_jag.principal),
                    jwt_id: Some(&verified_id_jag.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::IdentityAssertionReplay,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_grant",
                "identity assertion has already been used",
            );
        }
        Err(err) => {
            tracing::error!("failed to record ID-JAG replay state: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&verified_id_jag.principal),
                    jwt_id: Some(&verified_id_jag.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::AuthStateUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let scopes = match id_jag_token_scopes(
        catalog,
        profile,
        client,
        request.scope.as_deref(),
        &verified_id_jag.scopes,
    ) {
        Ok(scopes) => scopes,
        Err(err) => {
            tracing::warn!(client = %client_id, "rejected ID-JAG scope request: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&verified_id_jag.principal),
                    jwt_id: Some(&verified_id_jag.jwt_id),
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
    let token = match issue_access_token(
        catalog,
        authorization_server,
        profile,
        &verified_id_jag.principal.subject,
        PrincipalKind::User,
        Some(&verified_id_jag.principal),
        &scopes,
    )
    .await
    {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("failed to issue ID-JAG access token: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&verified_id_jag.principal),
                    jwt_id: Some(&verified_id_jag.jwt_id),
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
    if let Err(err) = record_id_jag_auth_audit(
        &state.gateway_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: Some(&verified_id_jag.principal),
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

pub(super) fn requested_token_scopes(
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    client: &OAuthClientRegistration,
    raw_scope: Option<&str>,
) -> anyhow::Result<BTreeSet<ScopeName>> {
    let raw_scope = raw_scope.ok_or_else(|| anyhow!("scope is required"))?;
    let scopes = raw_scope
        .split_whitespace()
        .map(ScopeName::new)
        .collect::<Result<BTreeSet<_>, _>>()?;
    if scopes.is_empty() {
        return Err(anyhow!("scope is required"));
    }
    let profile_supported_scopes = catalog.profile_supported_scopes(profile);
    if !scopes.is_subset(&client.allowed_scopes) {
        return Err(anyhow!("requested scope is not allowed for OAuth client"));
    }
    if !scopes.is_subset(&profile_supported_scopes) {
        return Err(anyhow!(
            "requested scope is not supported by gateway profile"
        ));
    }
    Ok(scopes)
}

fn id_jag_token_scopes(
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    client: &OAuthClientRegistration,
    raw_scope: Option<&str>,
    id_jag_scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<BTreeSet<ScopeName>> {
    if id_jag_scopes.is_empty() {
        return Err(anyhow!("ID-JAG scope is required"));
    }
    let scopes = match raw_scope {
        Some(raw_scope) => {
            let scopes = raw_scope
                .split_whitespace()
                .map(ScopeName::new)
                .collect::<Result<BTreeSet<_>, _>>()?;
            if scopes.is_empty() {
                return Err(anyhow!("scope is required"));
            }
            if !scopes.is_subset(id_jag_scopes) {
                return Err(anyhow!("requested scope exceeds ID-JAG scope"));
            }
            scopes
        }
        None => id_jag_scopes.clone(),
    };
    let profile_supported_scopes = catalog.profile_supported_scopes(profile);
    if !scopes.is_subset(&client.allowed_scopes) {
        return Err(anyhow!("requested scope is not allowed for OAuth client"));
    }
    if !scopes.is_subset(&profile_supported_scopes) {
        return Err(anyhow!(
            "requested scope is not supported by gateway profile"
        ));
    }
    Ok(scopes)
}

pub(super) fn authorization_code_client_allowed(
    profile: &GatewayProfile,
    client: &OAuthClientRegistration,
) -> bool {
    client.authorization_server == profile.authorization_server
        && client.allowed_profiles.contains(&profile.id)
        && client
            .grant_types
            .contains(&OAuthGrantType::AuthorizationCodePkce)
        && client.auth_methods.contains(&OAuthClientAuthMethod::None)
}
