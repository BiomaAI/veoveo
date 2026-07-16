use std::time::Instant;

use axum::{http::StatusCode, response::IntoResponse};
use chrono::Utc;
use veoveo_mcp_contract::{
    AuthOutcome, AuthReasonCode, GatewayProfile, OAuthClientAuthMethod, OAuthClientId,
    OAuthGrantType, OAuthRefreshToken, PrincipalKind, ResourceAuthorizationServer,
};
use veoveo_mcp_gateway::{GatewayCatalog, GatewayRefreshExchange, GatewayRefreshRotationRequest};

use crate::{
    audit::{
        AuthAuditRecord, auth_audit_error_response, record_refresh_auth_audit,
        refresh_auth_audit_event,
    },
    http_util::{TokenResponse, oauth_error_response, scope_string, token_response},
    oauth_grants::TokenRequest,
    runtime::AppState,
    tokens::{ACCESS_TOKEN_TTL_SECONDS, issue_access_token},
};

pub(crate) async fn token_endpoint_refresh_token(
    state: &AppState,
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    request: TokenRequest,
    started_at: Instant,
) -> axum::response::Response {
    let client_id = match OAuthClientId::new(request.client_id.trim()) {
        Ok(client_id) => client_id,
        Err(_) => {
            return invalid_refresh_response(
                state,
                profile,
                authorization_server,
                None,
                None,
                AuthReasonCode::InvalidClient,
                started_at,
            )
            .await;
        }
    };
    let Some(client) = catalog.oauth_client(&client_id) else {
        return invalid_refresh_response(
            state,
            profile,
            authorization_server,
            Some(&client_id),
            None,
            AuthReasonCode::InvalidClient,
            started_at,
        )
        .await;
    };
    if client.authorization_server != profile.authorization_server
        || !client
            .allowed_resources
            .contains(&profile.protected_resource)
        || !client.grant_types.contains(&OAuthGrantType::RefreshToken)
        || !client.auth_methods.contains(&OAuthClientAuthMethod::None)
    {
        return invalid_refresh_response(
            state,
            profile,
            authorization_server,
            Some(&client_id),
            None,
            AuthReasonCode::UnsupportedGrantType,
            started_at,
        )
        .await;
    }
    if request.scope.is_some() {
        if let Err(error) = record_refresh_auth_audit(
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
            return auth_audit_error_response(error);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_scope",
            "refresh-token exchange cannot request new scopes",
        );
    }
    let refresh_token = match request
        .refresh_token
        .as_deref()
        .map(str::trim)
        .map(OAuthRefreshToken::new)
        .transpose()
    {
        Ok(Some(token)) => token,
        _ => {
            return invalid_refresh_response(
                state,
                profile,
                authorization_server,
                Some(&client_id),
                None,
                AuthReasonCode::InvalidRefreshToken,
                started_at,
            )
            .await;
        }
    };
    let now = Utc::now();
    let grant = match state
        .gateway_state
        .refresh_token_grant(
            &refresh_token,
            &authorization_server.id,
            &profile.id,
            &client_id,
            now,
        )
        .await
    {
        Ok(Some(grant)) => grant,
        Ok(None) => {
            return invalid_refresh_response(
                state,
                profile,
                authorization_server,
                Some(&client_id),
                None,
                AuthReasonCode::InvalidRefreshToken,
                started_at,
            )
            .await;
        }
        Err(error) => {
            tracing::error!("failed to inspect OAuth refresh token: {error:#}");
            if let Err(error) = record_refresh_auth_audit(
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
                return auth_audit_error_response(error);
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if !grant.scopes.is_subset(&client.allowed_scopes)
        || !grant
            .scopes
            .is_subset(&catalog.profile_supported_scopes(profile))
    {
        return invalid_refresh_response(
            state,
            profile,
            authorization_server,
            Some(&client_id),
            Some(&grant.principal),
            AuthReasonCode::InvalidScope,
            started_at,
        )
        .await;
    }
    let token = match issue_access_token(
        catalog,
        authorization_server,
        &profile.protected_resource,
        &grant.principal.subject,
        &client_id,
        PrincipalKind::User,
        Some(&grant.principal),
        None,
        &grant.scopes,
    )
    .await
    {
        Ok(token) => token,
        Err(error) => {
            tracing::error!("failed to issue refreshed access token: {error}");
            if let Err(error) = record_refresh_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&grant.principal),
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::TokenSigningKeyUnavailable,
                    started_at,
                },
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "token signing key is unavailable",
            );
        }
    };
    let success_audit = match refresh_auth_audit_event(
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: Some(&grant.principal),
            jwt_id: Some(&token.jwt_id),
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
    ) {
        Ok(event) => event,
        Err(error) => return auth_audit_error_response(error),
    };
    let duplicate_delivery_audit = match refresh_auth_audit_event(
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: Some(&grant.principal),
            jwt_id: Some(&token.jwt_id),
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::RefreshTokenDuplicateDelivery,
            started_at,
        },
    ) {
        Ok(event) => event,
        Err(error) => return auth_audit_error_response(error),
    };
    let rotated = match state
        .gateway_state
        .rotate_refresh_token(
            &refresh_token,
            GatewayRefreshRotationRequest {
                authorization_server: &authorization_server.id,
                profile: &profile.id,
                oauth_client_id: &client_id,
                now,
                delivery_window: state.refresh_delivery_window,
                delivery_cipher: &state.refresh_delivery_cipher,
                success_audit: &success_audit,
                duplicate_delivery_audit: &duplicate_delivery_audit,
            },
        )
        .await
    {
        Ok(
            GatewayRefreshExchange::Rotated(rotated)
            | GatewayRefreshExchange::DuplicateDelivery(rotated),
        ) => rotated,
        Ok(GatewayRefreshExchange::ReplayDetected { grant }) => {
            return invalid_refresh_response(
                state,
                profile,
                authorization_server,
                Some(&client_id),
                Some(&grant.principal),
                AuthReasonCode::RefreshTokenReplay,
                started_at,
            )
            .await;
        }
        Ok(GatewayRefreshExchange::Invalid) => {
            return invalid_refresh_response(
                state,
                profile,
                authorization_server,
                Some(&client_id),
                None,
                AuthReasonCode::InvalidRefreshToken,
                started_at,
            )
            .await;
        }
        Err(error) => {
            tracing::error!("failed to rotate OAuth refresh token: {error:#}");
            if let Err(error) = record_refresh_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&grant.principal),
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::AuthStateUnavailable,
                    started_at,
                },
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let refresh_expires_in = rotated
        .grant
        .expires_at
        .signed_duration_since(Utc::now())
        .num_seconds()
        .max(0) as u64;
    token_response(TokenResponse {
        access_token: token.access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECONDS as u64,
        scope: scope_string(&rotated.grant.scopes),
        refresh_token: Some(rotated.token.as_str().to_owned()),
        refresh_token_expires_in: Some(refresh_expires_in),
    })
}

async fn invalid_refresh_response(
    state: &AppState,
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    client_id: Option<&OAuthClientId>,
    principal: Option<&veoveo_mcp_contract::Principal>,
    reason: AuthReasonCode,
    started_at: Instant,
) -> axum::response::Response {
    if let Err(error) = record_refresh_auth_audit(
        &state.gateway_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id,
            principal,
            jwt_id: None,
            outcome: AuthOutcome::Deny,
            reason,
            started_at,
        },
    )
    .await
    {
        return auth_audit_error_response(error);
    }
    oauth_error_response(
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        "refresh token is invalid, expired, or replayed",
    )
}
