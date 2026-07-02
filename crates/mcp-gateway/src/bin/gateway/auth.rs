use std::time::Instant;

use axum::{
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use chrono::Utc;
use veoveo_mcp_contract::{AuthOutcome, AuthReasonCode};
use veoveo_mcp_gateway::{AuthenticatedSubject, BearerToken, JwtAuthConfig, JwtVerifier};

use crate::{
    audit::{auth_audit_error_response, record_auth_audit, unauthorized},
    http_util::{allowed_gateway_jwt_algorithms, load_jwks},
    runtime::{
        ProfileAuthState, current_catalog, current_http_client, profile_id_from_gateway_path,
    },
};

pub(super) async fn authenticate_mcp(
    State(state): State<ProfileAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let started_at = Instant::now();
    let Some(profile_id) = profile_id_from_gateway_path(request.uri().path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(&profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(authorization_server) = catalog.authorization_server(&profile.authorization_server)
    else {
        if let Err(err) = record_auth_audit(
            &state,
            profile,
            AuthOutcome::Deny,
            AuthReasonCode::UnknownAuthorizationServer,
            None,
            started_at,
        ) {
            return auth_audit_error_response(err);
        }
        return unauthorized(&state, profile, "unknown authorization server");
    };

    let Some(header) = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        if let Err(err) = record_auth_audit(
            &state,
            profile,
            AuthOutcome::Deny,
            AuthReasonCode::MissingAuthorizationHeader,
            None,
            started_at,
        ) {
            return auth_audit_error_response(err);
        }
        return unauthorized(&state, profile, "missing authorization header");
    };
    let token = match BearerToken::from_authorization_header(header) {
        Ok(token) => token,
        Err(err) => {
            tracing::warn!("rejected gateway request: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidAuthorizationHeader,
                None,
                started_at,
            ) {
                return auth_audit_error_response(err);
            }
            return unauthorized(&state, profile, "invalid authorization header");
        }
    };

    let http = current_http_client(&state.http);
    let jwks = match load_jwks(&http, &authorization_server.jwks).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!("failed to load resource authorization server JWKS: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::AuthorizationServerUnavailable,
                None,
                started_at,
            ) {
                return auth_audit_error_response(err);
            }
            return unauthorized(&state, profile, "authorization server unavailable");
        }
    };
    let auth_config = match JwtAuthConfig::new(
        authorization_server.issuer.clone(),
        profile.protected_resource.clone(),
        profile.required_scopes.iter().cloned().collect(),
        allowed_gateway_jwt_algorithms(),
    ) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("invalid gateway auth config: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidAuthConfig,
                None,
                started_at,
            ) {
                return auth_audit_error_response(err);
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let subject = match JwtVerifier::new(auth_config, jwks).verify(&token) {
        Ok(subject) => subject,
        Err(err) => {
            tracing::warn!("rejected gateway token: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidBearerToken,
                None,
                started_at,
            ) {
                return auth_audit_error_response(err);
            }
            return unauthorized(&state, profile, "invalid bearer token");
        }
    };
    if let Some(jwt_id) = &subject.access_token.jwt_id {
        match state.gateway_state.jwt_revocation(
            &profile.id,
            &subject.access_token.issuer,
            jwt_id,
            Utc::now(),
        ) {
            Ok(Some(_revocation)) => {
                tracing::warn!(
                    profile = %profile.id,
                    issuer = %subject.access_token.issuer,
                    jwt_id = %jwt_id,
                    "rejected revoked gateway token"
                );
                if let Err(err) = record_auth_audit(
                    &state,
                    profile,
                    AuthOutcome::Deny,
                    AuthReasonCode::TokenRevoked,
                    Some(&subject),
                    started_at,
                ) {
                    return auth_audit_error_response(err);
                }
                return unauthorized(&state, profile, "token revoked");
            }
            Ok(None) => {}
            Err(err) => {
                tracing::error!("failed to check gateway token revocation state: {err}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }
    if let Err(err) = record_auth_audit(
        &state,
        profile,
        AuthOutcome::Allow,
        AuthReasonCode::AuthAllow,
        Some(&subject),
        started_at,
    ) {
        return auth_audit_error_response(err);
    }

    request
        .extensions_mut()
        .insert::<AuthenticatedSubject>(subject);
    next.run(request).await
}
