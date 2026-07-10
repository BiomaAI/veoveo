use std::time::Instant;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{TimeDelta, Utc};
use serde::Deserialize;
use url::Url;
use veoveo_mcp_contract::{
    AuthMode, AuthOutcome, AuthReasonCode, GatewayAuthorizationRequest, OAuthClientId,
    OAuthRedirectUri, OAuthStateValue, PkceCodeChallenge, PkceCodeChallengeMethod,
};

use crate::{
    audit::{
        AuthAuditRecord, auth_audit_error_response, internal_error_response, record_oidc_auth_audit,
    },
    http_util::{
        oauth_error_response, pkce_s256_challenge, random_oauth_state, random_oidc_nonce,
        random_pkce_verifier, redirect_response, scope_string,
    },
    oauth_grants::{authorization_code_client_allowed, requested_token_scopes},
    runtime::{AppState, current_catalog},
};

use super::resolve_oauth_profile;

const AUTHORIZATION_REQUEST_TTL_SECONDS: i64 = 10 * 60;

#[derive(Deserialize)]
pub(crate) struct AuthorizationRequest {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    code_challenge_method: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    resource: Option<String>,
}

pub(crate) async fn authorize_endpoint(
    State(state): State<AppState>,
    Query(request): Query<AuthorizationRequest>,
) -> axum::response::Response {
    let started_at = Instant::now();
    let catalog = current_catalog(&state.catalog);
    let profile = match resolve_oauth_profile(
        &catalog,
        request.client_id.as_str(),
        request.resource.as_deref(),
    ) {
        Ok(profile) => profile,
        Err(response) => return *response,
    };
    let Some(authorization_server) = catalog.authorization_server(&profile.authorization_server)
    else {
        if let Err(err) = record_oidc_auth_audit(
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
            "unsupported_response_type",
            "authorization code flow is not enabled for this gateway profile",
        );
    }
    if request.response_type != "code" {
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: None,
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
            "authorization request is invalid",
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
                "client is not allowed for this gateway profile",
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
            "client is not allowed for this gateway profile",
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
            "client is not allowed for this gateway profile",
        );
    }
    let redirect_uri = match OAuthRedirectUri::new(request.redirect_uri.trim()) {
        Ok(redirect_uri) if client.redirect_uris.contains(&redirect_uri) => redirect_uri,
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
                "redirect_uri is not registered for this client",
            );
        }
    };
    let scopes = match requested_token_scopes(&catalog, profile, client, request.scope.as_deref()) {
        Ok(scopes) => scopes,
        Err(err) => {
            tracing::warn!(client = %client_id, "rejected authorization scope request: {err}");
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
                "requested scope is not allowed",
            );
        }
    };
    let code_challenge = match PkceCodeChallenge::new(request.code_challenge.trim()) {
        Ok(challenge) if request.code_challenge_method == "S256" => challenge,
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
                "PKCE S256 code challenge is required",
            );
        }
    };
    let client_state = match request
        .state
        .as_deref()
        .map(OAuthStateValue::new)
        .transpose()
    {
        Ok(state) => state,
        Err(_) => {
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
                "state value is invalid",
            );
        }
    };
    let Some(oidc_client) = catalog.profile_oidc_client(profile) else {
        tracing::error!(profile = %profile.id, "gateway profile is missing OIDC client registration");
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "OIDC client is not configured",
        );
    };
    let Some(identity_provider) = catalog.identity_provider(&oidc_client.identity_provider) else {
        tracing::error!(identity_provider = %oidc_client.identity_provider, "unknown identity provider");
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "identity provider is unavailable",
        );
    };
    let Some(idp_authorization_endpoint) = identity_provider.authorization_endpoint.as_ref() else {
        tracing::error!(identity_provider = %identity_provider.id, "identity provider has no authorization endpoint");
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "identity provider is not configured",
        );
    };

    let idp_state = match random_oauth_state() {
        Ok(value) => value,
        Err(err) => return internal_error_response(err),
    };
    let idp_code_verifier = match random_pkce_verifier() {
        Ok(value) => value,
        Err(err) => return internal_error_response(err),
    };
    let idp_code_challenge = match pkce_s256_challenge(&idp_code_verifier) {
        Ok(value) => value,
        Err(err) => return internal_error_response(err),
    };
    let nonce = match random_oidc_nonce() {
        Ok(value) => value,
        Err(err) => return internal_error_response(err),
    };
    let now = Utc::now();
    let expires_at =
        match now.checked_add_signed(TimeDelta::seconds(AUTHORIZATION_REQUEST_TTL_SECONDS)) {
            Some(value) => value,
            None => return internal_error_response("authorization request expiration overflow"),
        };
    let authorization_request = GatewayAuthorizationRequest {
        idp_state: idp_state.clone(),
        profile: profile.id.clone(),
        oauth_client_id: client_id.clone(),
        oidc_client: oidc_client.id.clone(),
        redirect_uri,
        client_state,
        requested_scopes: scopes,
        code_challenge,
        code_challenge_method: PkceCodeChallengeMethod::S256,
        idp_code_verifier,
        idp_code_challenge: idp_code_challenge.clone(),
        idp_code_challenge_method: PkceCodeChallengeMethod::S256,
        nonce: nonce.clone(),
        created_at: now,
        expires_at,
    };
    if let Err(err) = state
        .gateway_state
        .record_authorization_request(&authorization_request)
        .await
    {
        tracing::error!("failed to record gateway authorization request: {err}");
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
    if let Err(err) = record_oidc_auth_audit(
        &state.gateway_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: None,
            jwt_id: None,
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
    )
    .await
    {
        return auth_audit_error_response(err);
    }

    let mut redirect = match Url::parse(idp_authorization_endpoint.as_str()) {
        Ok(url) => url,
        Err(err) => return internal_error_response(err),
    };
    redirect
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", oidc_client.client_id.as_str())
        .append_pair("redirect_uri", oidc_client.redirect_uri.as_str())
        .append_pair("scope", &scope_string(&oidc_client.scopes))
        .append_pair("state", idp_state.as_str())
        .append_pair("code_challenge", idp_code_challenge.as_str())
        .append_pair("code_challenge_method", "S256")
        .append_pair("nonce", nonce.as_str());
    redirect_response(redirect.as_str())
}
