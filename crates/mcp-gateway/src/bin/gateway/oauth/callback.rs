use std::time::Instant;

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{TimeDelta, Utc};
use serde::Deserialize;
use veoveo_mcp_contract::{
    AuthOutcome, AuthReasonCode, GatewayAuthorizationCodeRecord, GatewayProfileId, OAuthStateValue,
    SecretPurpose,
};
use veoveo_mcp_gateway::{GatewaySecretResolver, OidcIdTokenConfig, OidcIdTokenVerifier};

use crate::{
    audit::{
        AuthAuditRecord, auth_audit_error_response, internal_error_response, record_oidc_auth_audit,
    },
    http_util::{
        OidcTokenExchangeRequest, allowed_gateway_jwt_algorithms, exchange_oidc_authorization_code,
        load_jwks, oauth_error_response, random_authorization_code,
        redirect_with_authorization_code, redirect_with_oauth_error,
    },
    runtime::{AppState, current_catalog, current_http_client},
};

const AUTHORIZATION_CODE_TTL_SECONDS: i64 = 5 * 60;

#[derive(Debug, Deserialize)]
pub(crate) struct AuthorizationCallback {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

pub(crate) async fn authorization_callback(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
    Query(callback): Query<AuthorizationCallback>,
) -> axum::response::Response {
    let started_at = Instant::now();
    let Ok(profile_id) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(&profile_id).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(authorization_server) = catalog
        .authorization_server(&profile.authorization_server)
        .cloned()
    else {
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "authorization server is unavailable",
        );
    };
    let Some(raw_state) = callback.state.as_deref() else {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "state is required",
        );
    };
    let idp_state = match OAuthStateValue::new(raw_state.trim()) {
        Ok(value) => value,
        Err(_) => {
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "state is invalid",
            );
        }
    };
    let authorization_request = match state
        .gateway_state
        .consume_authorization_request(&idp_state, Utc::now())
    {
        Ok(Some(request)) if request.profile == profile.id => request,
        Ok(_) => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                &profile,
                AuthAuditRecord {
                    authorization_server: Some(&authorization_server),
                    client_id: None,
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
                "authorization state is invalid or expired",
            );
        }
        Err(err) => {
            tracing::error!("failed to consume gateway authorization state: {err}");
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                &profile,
                AuthAuditRecord {
                    authorization_server: Some(&authorization_server),
                    client_id: None,
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
    if let Some(error) = callback.error.as_deref() {
        let description = callback.error_description.as_deref();
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            &profile,
            AuthAuditRecord {
                authorization_server: Some(&authorization_server),
                client_id: Some(&authorization_request.oauth_client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidAuthorizationRequest,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return redirect_with_oauth_error(
            &authorization_request.redirect_uri,
            error,
            description,
            authorization_request.client_state.as_ref(),
        );
    }
    let Some(idp_code) = callback
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return redirect_with_oauth_error(
            &authorization_request.redirect_uri,
            "invalid_request",
            Some("authorization code is required"),
            authorization_request.client_state.as_ref(),
        );
    };
    let idp_code = idp_code.to_string();
    let Some(oidc_client) = catalog
        .oidc_client(&authorization_request.oidc_client)
        .cloned()
    else {
        tracing::error!(oidc_client = %authorization_request.oidc_client, "unknown OIDC client registration");
        return redirect_with_oauth_error(
            &authorization_request.redirect_uri,
            "server_error",
            Some("OIDC client is unavailable"),
            authorization_request.client_state.as_ref(),
        );
    };
    let Some(identity_provider) = catalog
        .identity_provider(&oidc_client.identity_provider)
        .cloned()
    else {
        tracing::error!(identity_provider = %oidc_client.identity_provider, "unknown identity provider");
        return redirect_with_oauth_error(
            &authorization_request.redirect_uri,
            "server_error",
            Some("identity provider is unavailable"),
            authorization_request.client_state.as_ref(),
        );
    };
    let Some(token_endpoint) = identity_provider
        .token_endpoint
        .as_ref()
        .map(ToString::to_string)
    else {
        tracing::error!(identity_provider = %identity_provider.id, "identity provider has no token endpoint");
        return redirect_with_oauth_error(
            &authorization_request.redirect_uri,
            "server_error",
            Some("identity provider is not configured"),
            authorization_request.client_state.as_ref(),
        );
    };
    let client_secret = match GatewaySecretResolver::new()
        .resolve_string(
            &catalog,
            &oidc_client.credential_secret,
            SecretPurpose::OAuthClientSecret,
        )
        .await
    {
        Ok(secret) => secret,
        Err(err) => {
            tracing::error!("failed to resolve OIDC client secret: {err}");
            return redirect_with_oauth_error(
                &authorization_request.redirect_uri,
                "server_error",
                Some("OIDC client secret is unavailable"),
                authorization_request.client_state.as_ref(),
            );
        }
    };
    let idp_jwks_source = identity_provider.jwks.clone();
    let idp_issuer = identity_provider.issuer.clone();
    let oidc_client_id = oidc_client.client_id.clone();
    let oidc_client_record_id = oidc_client.id.clone();
    let token_exchange = OidcTokenExchangeRequest {
        token_endpoint,
        client_id: oidc_client.client_id.to_string(),
        client_secret,
        auth_method: oidc_client.auth_method,
        redirect_uri: oidc_client.redirect_uri.to_string(),
        code_verifier: authorization_request.idp_code_verifier.to_string(),
    };
    drop(catalog);
    drop(identity_provider);
    drop(oidc_client);
    let http = current_http_client(&state.http);
    let token_response =
        match exchange_oidc_authorization_code(&http, token_exchange, idp_code).await {
            Ok(response) => response,
            Err(err) => {
                tracing::warn!("OIDC token exchange failed: {err}");
                if let Err(err) = record_oidc_auth_audit(
                    &state.gateway_state,
                    &profile,
                    AuthAuditRecord {
                        authorization_server: Some(&authorization_server),
                        client_id: Some(&authorization_request.oauth_client_id),
                        principal: None,
                        jwt_id: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::IdentityProviderUnavailable,
                        started_at,
                    },
                ) {
                    return auth_audit_error_response(err);
                }
                return redirect_with_oauth_error(
                    &authorization_request.redirect_uri,
                    "server_error",
                    Some("identity provider token exchange failed"),
                    authorization_request.client_state.as_ref(),
                );
            }
        };
    let idp_jwks = match load_jwks(&http, &idp_jwks_source).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!("failed to load identity provider JWKS for OIDC: {err}");
            return redirect_with_oauth_error(
                &authorization_request.redirect_uri,
                "server_error",
                Some("identity provider keys are unavailable"),
                authorization_request.client_state.as_ref(),
            );
        }
    };
    let verifier = match OidcIdTokenConfig::new(
        idp_issuer,
        oidc_client_id,
        authorization_request.nonce.clone(),
        allowed_gateway_jwt_algorithms(),
    ) {
        Ok(config) => OidcIdTokenVerifier::new(config, idp_jwks),
        Err(err) => return internal_error_response(err),
    };
    let verified_identity = match verifier.verify(&token_response.id_token) {
        Ok(identity) => identity,
        Err(err) => {
            tracing::warn!("rejected OIDC ID token: {err}");
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                &profile,
                AuthAuditRecord {
                    authorization_server: Some(&authorization_server),
                    client_id: Some(&authorization_request.oauth_client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidOidcIdToken,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return redirect_with_oauth_error(
                &authorization_request.redirect_uri,
                "invalid_grant",
                Some("identity token could not be validated"),
                authorization_request.client_state.as_ref(),
            );
        }
    };
    let gateway_code = match random_authorization_code() {
        Ok(code) => code,
        Err(err) => return internal_error_response(err),
    };
    let now = Utc::now();
    let expires_at =
        match now.checked_add_signed(TimeDelta::seconds(AUTHORIZATION_CODE_TTL_SECONDS)) {
            Some(value) => value,
            None => return internal_error_response("authorization code expiration overflow"),
        };
    let code_record = GatewayAuthorizationCodeRecord {
        code: gateway_code.clone(),
        profile: profile.id.clone(),
        oauth_client_id: authorization_request.oauth_client_id.clone(),
        oidc_client: oidc_client_record_id,
        redirect_uri: authorization_request.redirect_uri.clone(),
        client_state: authorization_request.client_state.clone(),
        scopes: authorization_request.requested_scopes.clone(),
        code_challenge: authorization_request.code_challenge.clone(),
        code_challenge_method: authorization_request.code_challenge_method,
        principal: verified_identity.principal.clone(),
        issued_at: now,
        expires_at,
        consumed_at: None,
    };
    if let Err(err) = state.gateway_state.record_authorization_code(&code_record) {
        tracing::error!("failed to record gateway authorization code: {err}");
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            &profile,
            AuthAuditRecord {
                authorization_server: Some(&authorization_server),
                client_id: Some(&authorization_request.oauth_client_id),
                principal: Some(&verified_identity.principal),
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
    if let Err(err) = record_oidc_auth_audit(
        &state.gateway_state,
        &profile,
        AuthAuditRecord {
            authorization_server: Some(&authorization_server),
            client_id: Some(&authorization_request.oauth_client_id),
            principal: Some(&verified_identity.principal),
            jwt_id: None,
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
    ) {
        return auth_audit_error_response(err);
    }
    redirect_with_authorization_code(
        &authorization_request.redirect_uri,
        &gateway_code,
        authorization_request.client_state.as_ref(),
    )
}
