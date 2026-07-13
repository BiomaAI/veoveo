use std::time::Instant;

use axum::{
    Form,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Deserialize;
use veoveo_mcp_contract::{
    AuthOutcome, AuthReasonCode, GatewayProfile, GatewayRefreshRevocationRequest,
    OAuthClientAuthMethod, OAuthClientId, OAuthGrantType, OAuthRefreshToken, OAuthTokenTypeHint,
    ProtectedResourceId, ResourceAuthorizationServer,
};

use crate::{
    audit::{AuthAuditRecord, auth_audit_error_response, record_refresh_auth_audit},
    http_util::oauth_error_response,
    oauth::resolve_oauth_profile,
    runtime::{AppState, current_catalog},
};

#[derive(Deserialize)]
pub(crate) struct RefreshRevocationForm {
    token: String,
    #[serde(default)]
    token_type_hint: Option<String>,
    client_id: String,
    #[serde(default)]
    resource: Option<String>,
}

pub(crate) async fn revoke_refresh_token(
    State(state): State<AppState>,
    Form(form): Form<RefreshRevocationForm>,
) -> Response {
    let started_at = Instant::now();
    let catalog = current_catalog(&state.catalog);
    let profile =
        match resolve_oauth_profile(&catalog, form.client_id.as_str(), form.resource.as_deref()) {
            Ok(profile) => profile,
            Err(response) => return *response,
        };
    let Some(authorization_server) = catalog.authorization_server(&profile.authorization_server)
    else {
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "authorization server is unavailable",
        );
    };
    let client_id = match OAuthClientId::new(form.client_id.trim()) {
        Ok(client_id) => client_id,
        Err(_) => {
            return invalid_client_response(
                &state,
                profile,
                authorization_server,
                None,
                started_at,
            )
            .await;
        }
    };
    let Some(client) = catalog.oauth_client(&client_id) else {
        return invalid_client_response(
            &state,
            profile,
            authorization_server,
            Some(&client_id),
            started_at,
        )
        .await;
    };
    if client.authorization_server != profile.authorization_server
        || !client.allowed_profiles.contains(&profile.id)
        || !client.grant_types.contains(&OAuthGrantType::RefreshToken)
        || !client.auth_methods.contains(&OAuthClientAuthMethod::None)
    {
        return invalid_client_response(
            &state,
            profile,
            authorization_server,
            Some(&client_id),
            started_at,
        )
        .await;
    }
    let request = match parse_request(form, client_id.clone()) {
        Ok(request) => request,
        Err(RevocationRequestError::UnsupportedTokenType) => {
            if let Err(error) = record_revocation_audit(
                &state,
                profile,
                authorization_server,
                Some(&client_id),
                None,
                AuthOutcome::Deny,
                AuthReasonCode::UnsupportedGrantType,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "unsupported_token_type",
                "only refresh tokens can be revoked",
            );
        }
        Err(RevocationRequestError::InvalidToken) => {
            if let Err(error) = record_revocation_audit(
                &state,
                profile,
                authorization_server,
                Some(&client_id),
                None,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidRefreshToken,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            return revocation_success_response();
        }
    };
    match state
        .gateway_state
        .revoke_refresh_token_family(
            &request.token,
            &authorization_server.id,
            &profile.id,
            &request.client_id,
            Utc::now(),
        )
        .await
    {
        Ok(Some(grant)) => {
            if let Err(error) = record_revocation_audit(
                &state,
                profile,
                authorization_server,
                Some(&request.client_id),
                Some(&grant.principal),
                AuthOutcome::Allow,
                AuthReasonCode::RefreshTokenRevoked,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            revocation_success_response()
        }
        Ok(None) => {
            if let Err(error) = record_revocation_audit(
                &state,
                profile,
                authorization_server,
                Some(&request.client_id),
                None,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidRefreshToken,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            revocation_success_response()
        }
        Err(error) => {
            tracing::error!("failed to revoke OAuth refresh-token family: {error:#}");
            if let Err(audit_error) = record_revocation_audit(
                &state,
                profile,
                authorization_server,
                Some(&request.client_id),
                None,
                AuthOutcome::Deny,
                AuthReasonCode::AuthStateUnavailable,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(audit_error);
            }
            oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "refresh-token revocation is unavailable",
            )
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevocationRequestError {
    InvalidToken,
    UnsupportedTokenType,
}

fn parse_request(
    form: RefreshRevocationForm,
    client_id: OAuthClientId,
) -> Result<GatewayRefreshRevocationRequest, RevocationRequestError> {
    let token = OAuthRefreshToken::new(form.token.trim())
        .map_err(|_| RevocationRequestError::InvalidToken)?;
    let token_type_hint = match form.token_type_hint.as_deref().map(str::trim) {
        None | Some("") => None,
        Some("refresh_token") => Some(OAuthTokenTypeHint::RefreshToken),
        Some(_) => return Err(RevocationRequestError::UnsupportedTokenType),
    };
    let resource = form
        .resource
        .map(|resource| ProtectedResourceId::new(resource.trim()))
        .transpose()
        .map_err(|_| RevocationRequestError::InvalidToken)?;
    Ok(GatewayRefreshRevocationRequest {
        token,
        token_type_hint,
        client_id,
        resource,
    })
}

async fn invalid_client_response(
    state: &AppState,
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    client_id: Option<&OAuthClientId>,
    started_at: Instant,
) -> Response {
    if let Err(error) = record_revocation_audit(
        state,
        profile,
        authorization_server,
        client_id,
        None,
        AuthOutcome::Deny,
        AuthReasonCode::InvalidClient,
        started_at,
    )
    .await
    {
        return auth_audit_error_response(error);
    }
    oauth_error_response(
        StatusCode::UNAUTHORIZED,
        "invalid_client",
        "client is not permitted to revoke this refresh token",
    )
}

#[allow(clippy::too_many_arguments)]
async fn record_revocation_audit(
    state: &AppState,
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    client_id: Option<&OAuthClientId>,
    principal: Option<&veoveo_mcp_contract::Principal>,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    started_at: Instant,
) -> anyhow::Result<()> {
    record_refresh_auth_audit(
        &state.gateway_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id,
            principal,
            jwt_id: None,
            outcome,
            reason,
            started_at,
        },
    )
    .await
}

fn revocation_success_response() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    (StatusCode::OK, headers).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_form_builds_the_typed_secret_request() {
        let client_id = OAuthClientId::new("console").unwrap();
        let request = parse_request(
            RefreshRevocationForm {
                token: "A".repeat(43),
                token_type_hint: Some("refresh_token".to_owned()),
                client_id: client_id.to_string(),
                resource: Some("https://veoveo.example/mcp/admin".to_owned()),
            },
            client_id.clone(),
        )
        .unwrap();

        assert_eq!(request.client_id, client_id);
        assert_eq!(
            request.token_type_hint,
            Some(OAuthTokenTypeHint::RefreshToken)
        );
        assert_eq!(
            format!("{:?}", request.token),
            "OAuthRefreshToken([REDACTED])"
        );
    }

    #[test]
    fn revocation_form_rejects_non_refresh_token_hints() {
        let error = parse_request(
            RefreshRevocationForm {
                token: "A".repeat(43),
                token_type_hint: Some("access_token".to_owned()),
                client_id: "console".to_owned(),
                resource: None,
            },
            OAuthClientId::new("console").unwrap(),
        )
        .unwrap_err();

        assert_eq!(error, RevocationRequestError::UnsupportedTokenType);
    }
}
