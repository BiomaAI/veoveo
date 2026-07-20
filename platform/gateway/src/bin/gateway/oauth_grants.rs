use std::time::Instant;

use axum::{http::StatusCode, response::IntoResponse};
use chrono::Utc;
use veoveo_mcp_contract::{
    AuthMode, AuthOutcome, AuthReasonCode, GatewayProfile, InvocationProvenance,
    OAuthAuthorizationCode, OAuthClientId, OAuthGrantType, OAuthRedirectUri,
    PkceCodeChallengeMethod, PkceCodeVerifier, PrincipalKind, ResourceAuthorizationServer,
};
use veoveo_mcp_gateway::{GatewayCatalog, REFRESH_TOKEN_TTL_SECONDS};

use crate::{
    audit::{
        AuthAuditRecord, auth_audit_error_response, internal_error_response, record_oidc_auth_audit,
    },
    http_util::{
        TokenResponse, oauth_error_response, pkce_s256_challenge, scope_string, token_response,
    },
    runtime::AppState,
    tokens::{ACCESS_TOKEN_TTL_SECONDS, AccessTokenInvocation, issue_access_token},
};

#[path = "oauth_grants/id_jag.rs"]
mod id_jag;
#[path = "oauth_grants/refresh.rs"]
mod refresh;
#[path = "oauth_grants/request.rs"]
mod request;
#[path = "oauth_grants/scopes.rs"]
mod scopes;

pub(super) use id_jag::token_endpoint_id_jag;
pub(super) use refresh::token_endpoint_refresh_token;
pub(super) use request::TokenRequest;
pub(super) use scopes::{
    authorization_code_client_allowed, requested_client_credentials_scopes, requested_token_scopes,
};

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
        )
        .await
        {
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
            )
            .await
            {
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
        )
        .await
        {
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
        )
        .await
        {
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
        )
        .await
        {
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
            )
            .await
            {
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
            )
            .await
            {
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
            )
            .await
            {
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
        .await
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
            )
            .await
            {
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
            )
            .await
            {
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
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "authorization code binding is invalid",
        );
    }
    if let Err(err) = catalog.work_context_membership(
        &client_id,
        &code_record.work_context,
        &code_record.principal,
    ) {
        tracing::warn!("rejected authorization-code Work Context: {err}");
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: Some(&code_record.principal),
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidAuthorizationRequest,
                started_at,
            },
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::FORBIDDEN,
            "access_denied",
            "Work Context membership is required",
        );
    }
    let token = match issue_access_token(
        catalog,
        authorization_server,
        &profile.protected_resource,
        &code_record.principal.subject,
        &client_id,
        PrincipalKind::User,
        Some(&code_record.principal),
        None,
        AccessTokenInvocation {
            work_context: code_record.work_context.clone(),
            provenance: InvocationProvenance::Direct {
                initiator: code_record.principal.id.clone(),
            },
        },
        code_record.principal.id.clone(),
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
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "token signing key is unavailable",
            );
        }
    };
    let refresh = if client.grant_types.contains(&OAuthGrantType::RefreshToken) {
        match state
            .gateway_state
            .issue_refresh_token(
                &authorization_server.id,
                &profile.id,
                &client_id,
                &code_record.work_context,
                &code_record.principal,
                &code_record.scopes,
                Utc::now(),
            )
            .await
        {
            Ok(refresh) => Some(refresh),
            Err(err) => {
                tracing::error!("failed to issue browser refresh token: {err:#}");
                if let Err(err) = record_oidc_auth_audit(
                    &state.gateway_state,
                    profile,
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: Some(&code_record.principal),
                        jwt_id: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::AuthStateUnavailable,
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
                    "refresh-token state is unavailable",
                );
            }
        }
    } else {
        None
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
    )
    .await
    {
        return auth_audit_error_response(err);
    }
    token_response(TokenResponse {
        access_token: token.access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECONDS as u64,
        scope: scope_string(&code_record.scopes),
        refresh_token: refresh
            .as_ref()
            .map(|refresh| refresh.token.as_str().to_owned()),
        refresh_token_expires_in: refresh.map(|_| REFRESH_TOKEN_TTL_SECONDS as u64),
    })
}
