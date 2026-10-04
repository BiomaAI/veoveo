use super::auth_support::{
    allowed_gateway_jwt_algorithms, auth_audit_error_response,
    authorization_server_jwks_from_signing_key, load_jwks, record_auth_audit, unauthorized,
};
use super::{ProfileAuthState, current_catalog, current_http_client, public_authorization_server};
use crate::{
    AuthError, AuthenticatedSubject, BearerToken, GatewayCatalog, JwtAuthConfig, JwtVerifier,
};
use axum::{
    extract::{MatchedPath, OriginalUri, Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use chrono::Utc;
use jsonwebtoken::jwk::JwkSet;
use std::time::Instant;
use veoveo_mcp_contract::{
    AuthOutcome, AuthReasonCode, GatewayProfileId, PrincipalKind, ResourceAuthorizationServer,
};
use veoveo_platform_store::PrincipalKind as StorePrincipalKind;

/// Capture the raw spelling at the registered named profile position. Decoded
/// path extractors cannot establish whether the caller submitted an encoded alias.
pub fn profile_from_route(matched: &str, raw: &str) -> Option<GatewayProfileId> {
    let template: Vec<_> = matched.split('/').collect();
    let input: Vec<_> = raw.split('/').collect();
    let position = template.iter().position(|part| *part == "{profile}")?;
    for (expected, actual) in template.iter().zip(&input).take(position) {
        if !expected.starts_with('{') && expected != actual {
            return None;
        }
    }
    let raw = *input.get(position)?;
    if raw.contains('%') {
        return None;
    }
    GatewayProfileId::new(raw).ok()
}
pub fn profile_from_request(request: &Request) -> Option<GatewayProfileId> {
    let matched = request.extensions().get::<MatchedPath>()?;
    let raw = request
        .extensions()
        .get::<OriginalUri>()
        .map_or(request.uri(), |uri| &uri.0);
    profile_from_route(matched.as_str(), raw.path())
}
pub async fn authenticate_profile(
    State(state): State<ProfileAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let started_at = Instant::now();
    let Some(profile_id) = profile_from_request(&request) else {
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
        )
        .await
        {
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
        )
        .await
        {
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
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return unauthorized(&state, profile, "invalid authorization header");
        }
    };

    let http = current_http_client(&state.auth_http);
    let jwks = match load_resource_authorization_jwks(
        &catalog,
        authorization_server,
        state.deployment.base_url(),
        &http,
    )
    .await
    {
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
            )
            .await
            {
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
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let verified = match state
        .gateway_state
        .token_extension_registry()
        .map_err(|_| {
            AuthError::Extension(veoveo_types::ExtensionError::new(
                "JWT extension profile is unbound",
            ))
        })
        .and_then(|registry| JwtVerifier::new(auth_config, jwks).with_extensions(registry))
        .and_then(|verifier| verifier.verify(&token))
    {
        Ok(verified) => verified,
        Err(err) => {
            tracing::warn!("rejected gateway token: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidBearerToken,
                None,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return unauthorized(&state, profile, "invalid bearer token");
        }
    };
    let subject = match state
        .gateway_state
        .resolve_authenticated_subject(&catalog, verified)
        .await
    {
        Ok(subject) => subject,
        Err(err) => {
            tracing::warn!("rejected gateway authority: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidBearerToken,
                None,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return unauthorized(&state, profile, "invalid invocation authority");
        }
    };
    match state
        .gateway_state
        .access_token_session_valid(
            &profile.id,
            &authorization_server.id,
            &subject.access_token,
            &subject.principal,
        )
        .await
    {
        Ok(true) => {}
        outcome => {
            let (reason, response) = match outcome {
                Ok(false) => (
                    AuthReasonCode::TokenRevoked,
                    unauthorized(&state, profile, "session revoked or expired"),
                ),
                Err(error) => {
                    tracing::error!(%error, "failed to read current access-token session");
                    (
                        AuthReasonCode::AuthStateUnavailable,
                        StatusCode::SERVICE_UNAVAILABLE.into_response(),
                    )
                }
                Ok(true) => unreachable!(),
            };
            if let Err(error) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                reason,
                Some(&subject),
                started_at,
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            return response;
        }
    }
    if let Some(jwt_id) = &subject.access_token.jwt_id {
        match state
            .gateway_state
            .jwt_revocation(
                &profile.id,
                &subject.access_token.issuer,
                jwt_id,
                Utc::now(),
            )
            .await
        {
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
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return unauthorized(&state, profile, "token revoked");
            }
            Ok(None) => {}
            Err(err) => {
                tracing::error!("failed to check gateway token revocation state: {err}");
                if let Err(err) = record_auth_audit(
                    &state,
                    profile,
                    AuthOutcome::Deny,
                    AuthReasonCode::AuthStateUnavailable,
                    Some(&subject),
                    started_at,
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }
    if let Err(err) = sync_principal_directory(&state, &subject).await {
        tracing::error!(%err, principal = %subject.principal.id, "failed to synchronize authenticated principal display metadata");
        if let Err(audit_error) = record_auth_audit(
            &state,
            profile,
            AuthOutcome::Deny,
            AuthReasonCode::AuthStateUnavailable,
            Some(&subject),
            started_at,
        )
        .await
        {
            return auth_audit_error_response(audit_error);
        }
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if let Err(err) = record_auth_audit(
        &state,
        profile,
        AuthOutcome::Allow,
        AuthReasonCode::AuthAllow,
        Some(&subject),
        started_at,
    )
    .await
    {
        return auth_audit_error_response(err);
    }

    request
        .extensions_mut()
        .insert::<AuthenticatedSubject>(subject);
    next.run(request).await
}

async fn sync_principal_directory(
    state: &ProfileAuthState,
    subject: &AuthenticatedSubject,
) -> Result<(), veoveo_platform_store::StoreError> {
    let store = state.gateway_state.platform_store();
    let tenant = subject.authority.tenant.as_str();
    let principal = &subject.principal;
    let kind = match principal.kind {
        PrincipalKind::User => StorePrincipalKind::User,
        PrincipalKind::Service => StorePrincipalKind::Service,
    };
    match &subject.principal_display_name {
        Some(display_name) => {
            store
                .ensure_named_identity(
                    tenant,
                    principal.id.as_str(),
                    principal.issuer.as_str(),
                    principal.subject.as_str(),
                    kind,
                    display_name.as_str(),
                )
                .await?;
        }
        None => {
            store
                .ensure_identity(
                    tenant,
                    principal.id.as_str(),
                    principal.issuer.as_str(),
                    principal.subject.as_str(),
                    kind,
                )
                .await?;
        }
    }
    if subject.actor.id != principal.id {
        let actor = &subject.actor;
        store
            .ensure_identity(
                tenant,
                actor.id.as_str(),
                actor.issuer.as_str(),
                actor.subject.as_str(),
                match actor.kind {
                    PrincipalKind::User => StorePrincipalKind::User,
                    PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
    }
    Ok(())
}

pub async fn load_resource_authorization_jwks(
    catalog: &GatewayCatalog,
    authorization_server: &ResourceAuthorizationServer,
    public_base_url: &str,
    http: &reqwest::Client,
) -> anyhow::Result<JwkSet> {
    if public_authorization_server(catalog, public_base_url)
        .is_some_and(|hosted| hosted.id == authorization_server.id)
    {
        authorization_server_jwks_from_signing_key(catalog, authorization_server).await
    } else {
        load_jwks(http, &authorization_server.jwks).await
    }
}
