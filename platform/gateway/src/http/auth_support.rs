use super::ProfileAuthState;
use crate::{AuthenticatedSubject, GatewayState, www_authenticate_challenge};
use crate::{GatewayCatalog, GatewaySecretResolver};
use anyhow::Context;
use axum::{
    http::{HeaderMap, HeaderValue, StatusCode, header::WWW_AUTHENTICATE},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use jsonwebtoken::{
    Algorithm, EncodingKey,
    jwk::{Jwk, JwkSet},
};
use std::time::Instant;
use veoveo_mcp_contract::{
    AuthMethod, AuthOutcome, AuthReasonCode, GatewayProfile, GatewayProfileId, ProtectedResourceId,
    audit::{
        AuditDetail, AuditDraft, AuditOutcome, AuditReason, AuditRequest, AuditTarget,
        AuthenticationActivity,
    },
};
use veoveo_mcp_contract::{
    JwksSource, ResourceAuthorizationServer, SecretPurpose, SecretReferenceId,
};
pub async fn record_auth_audit(
    state: &ProfileAuthState,
    profile: &GatewayProfile,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    subject: Option<&AuthenticatedSubject>,
    started_at: Instant,
) -> anyhow::Result<()> {
    if outcome == AuthOutcome::Allow {
        return Ok(());
    }
    record_resource_auth_audit(
        &state.gateway_state,
        AuthAuditTarget::from(profile),
        outcome,
        reason,
        subject,
        started_at,
    )
    .await
}

pub async fn record_resource_auth_audit(
    gateway_state: &GatewayState,
    target: AuthAuditTarget<'_>,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    subject: Option<&AuthenticatedSubject>,
    started_at: Instant,
) -> anyhow::Result<()> {
    // Successful bearer verification belongs to the action/window record.
    if outcome == AuthOutcome::Allow {
        return Ok(());
    }
    let request = subject
        .map(|subject| subject.audit.clone())
        .unwrap_or_else(audit_request);
    let mut draft = AuditDraft::builder(
        request,
        target.audit_target()?,
        AuditDetail::Authentication {
            activity: AuthenticationActivity::CredentialDenial,
            method: AuthMethod::BearerJwt,
            reason,
        },
        AuditOutcome::Denied,
        authentication_reason(outcome, reason),
    )
    .latency_ms(u64::try_from(started_at.elapsed().as_millis())?);
    if let Some(subject) = subject {
        draft = draft.actor(subject.audit_actor()?);
        if let Some(profile) = target.profile {
            draft = draft.authority(subject.audit_authority(profile));
        }
    }
    gateway_state.record_audit(draft.build()?).await
}

#[derive(Clone, Copy)]
pub struct AuthAuditTarget<'a> {
    pub profile: Option<&'a GatewayProfileId>,
    pub protected_resource: &'a ProtectedResourceId,
}

impl AuthAuditTarget<'_> {
    pub fn audit_target(self) -> anyhow::Result<AuditTarget> {
        Ok(match self.profile {
            Some(profile) => AuditTarget::Profile {
                profile: profile.clone(),
            },
            None => AuditTarget::PlatformResource {
                uri: veoveo_types::ResourceUri::new(self.protected_resource.to_string())?,
            },
        })
    }
}

impl<'a> From<&'a GatewayProfile> for AuthAuditTarget<'a> {
    fn from(profile: &'a GatewayProfile) -> Self {
        Self {
            profile: Some(&profile.id),
            protected_resource: &profile.protected_resource,
        }
    }
}

pub fn audit_request() -> AuditRequest {
    crate::request_observation::RequestObservation::current()
        .map(|request| request.audit)
        .unwrap_or_else(AuditRequest::background)
}
pub fn authentication_reason(outcome: AuthOutcome, reason: AuthReasonCode) -> AuditReason {
    if outcome == AuthOutcome::Allow && reason == AuthReasonCode::RefreshTokenRevoked {
        return AuditReason::Accepted;
    }
    match reason {
        AuthReasonCode::AuthAllow | AuthReasonCode::RefreshTokenDuplicateDelivery => {
            AuditReason::Accepted
        }
        AuthReasonCode::RefreshTokenReplay
        | AuthReasonCode::ClientAssertionReplay
        | AuthReasonCode::IdentityAssertionReplay => AuditReason::Replay,
        AuthReasonCode::RefreshTokenRevoked | AuthReasonCode::TokenRevoked => AuditReason::Revoked,
        AuthReasonCode::MissingAuthorizationHeader => AuditReason::Unauthenticated,
        AuthReasonCode::PolicyDenied | AuthReasonCode::InvalidScope => AuditReason::PolicyDenied,
        AuthReasonCode::IdentityProviderUnavailable
        | AuthReasonCode::AuthorizationServerUnavailable
        | AuthReasonCode::TokenSigningKeyUnavailable
        | AuthReasonCode::AuthStateUnavailable => AuditReason::Unavailable,
        _ => AuditReason::InvalidCredential,
    }
}
pub fn auth_audit_error_response(err: anyhow::Error) -> Response {
    tracing::error!("failed to record gateway auth audit event: {err:#}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

pub fn internal_error_response(err: impl std::fmt::Display) -> Response {
    tracing::error!("gateway internal error: {err}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

pub fn unauthorized(
    state: &ProfileAuthState,
    profile: &GatewayProfile,
    reason: &'static str,
) -> Response {
    let metadata_url = format!(
        "{}/.well-known/oauth-protected-resource/mcp/{}",
        state.deployment.base_url(),
        profile.id
    );
    let challenge = www_authenticate_challenge(&metadata_url, &profile.required_scopes);
    let Ok(challenge) = HeaderValue::from_str(&challenge) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut headers = HeaderMap::new();
    headers.insert(WWW_AUTHENTICATE, challenge);
    tracing::debug!(profile = %profile.id, reason, "gateway authorization challenge");
    (
        StatusCode::UNAUTHORIZED,
        headers,
        "authorization required for gateway profile",
    )
        .into_response()
}
pub async fn load_jwks(http: &reqwest::Client, jwks: &JwksSource) -> anyhow::Result<JwkSet> {
    match jwks {
        JwksSource::Remote { jwks_uri } => fetch_jwks(http, jwks_uri.as_str()).await,
        JwksSource::File { path } => {
            let bytes = std::fs::read(path.as_str())?;
            Ok(serde_json::from_slice::<JwkSet>(&bytes)?)
        }
    }
}

async fn fetch_jwks(http: &reqwest::Client, url: &str) -> anyhow::Result<JwkSet> {
    let response = http.get(url).send().await?.error_for_status()?;
    Ok(response.json::<JwkSet>().await?)
}

pub fn allowed_gateway_jwt_algorithms() -> Vec<Algorithm> {
    vec![
        Algorithm::RS256,
        Algorithm::RS384,
        Algorithm::RS512,
        Algorithm::PS256,
        Algorithm::PS384,
        Algorithm::PS512,
        Algorithm::ES256,
        Algorithm::ES384,
        Algorithm::EdDSA,
    ]
}

pub async fn authorization_server_jwks_from_signing_key(
    catalog: &GatewayCatalog,
    authorization_server: &ResourceAuthorizationServer,
) -> anyhow::Result<JwkSet> {
    let signing_key = access_token_signing_key(
        catalog,
        &authorization_server.access_token_signing_key,
        SecretPurpose::JwksPrivateKey,
    )
    .await?;
    let mut jwk = Jwk::from_encoding_key(&signing_key, Algorithm::RS256)?;
    jwk.common.key_id = Some(authorization_server.access_token_key_id.to_string());
    Ok(JwkSet { keys: vec![jwk] })
}

pub async fn access_token_signing_key(
    catalog: &GatewayCatalog,
    secret_id: &SecretReferenceId,
    expected_purpose: SecretPurpose,
) -> anyhow::Result<EncodingKey> {
    let value = GatewaySecretResolver::new()
        .resolve_string(catalog, secret_id, expected_purpose)
        .await?;
    let der = BASE64_STANDARD
        .decode(value.expose_secret().trim())
        .context("access-token signing key must be base64-encoded RSA DER")?;
    Ok(EncodingKey::from_rsa_der(&der))
}
