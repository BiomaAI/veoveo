use std::collections::BTreeSet;

use anyhow::{Context, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{TimeDelta, Utc};
use jsonwebtoken::{
    Algorithm, EncodingKey, Header, encode,
    jwk::{Jwk, JwkSet},
};
use serde::Serialize;
use veoveo_mcp_contract::{
    InvocationProvenance, JwtId, OAuthClientId, Principal, PrincipalId, PrincipalKind,
    ProtectedResourceId, ResourceAuthorizationServer, ScopeName, SecretPurpose, SecretReferenceId,
    TenantId, TokenSubject, WorkContextId,
};
use veoveo_mcp_gateway::{GatewayCatalog, GatewaySecretResolver};

pub(super) const ACCESS_TOKEN_TTL_SECONDS: i64 = 15 * 60;

#[derive(Serialize)]
struct AccessTokenClaims {
    iss: String,
    sub: String,
    principal_id: String,
    client_id: String,
    work_context: String,
    invocation_mode: veoveo_mcp_contract::InvocationMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    initiator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delegation_id: Option<String>,
    aud: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
    principal_kind: PrincipalKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    data_labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    principal_assurances: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct AccessTokenInvocation {
    pub(super) work_context: WorkContextId,
    pub(super) provenance: InvocationProvenance,
}

pub(super) struct IssuedAccessToken {
    pub(super) access_token: String,
    pub(super) jwt_id: JwtId,
}

impl std::fmt::Debug for IssuedAccessToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IssuedAccessToken")
            .field("access_token", &"[REDACTED]")
            .field("jwt_id", &self.jwt_id)
            .finish()
    }
}

pub(super) async fn issue_client_credentials_access_token(
    catalog: &GatewayCatalog,
    authorization_server: &ResourceAuthorizationServer,
    protected_resource: &ProtectedResourceId,
    client_id: &OAuthClientId,
    service_principal: &Principal,
    work_context: WorkContextId,
    scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<IssuedAccessToken> {
    issue_access_token(
        catalog,
        authorization_server,
        protected_resource,
        &service_principal.subject,
        client_id,
        PrincipalKind::Service,
        Some(service_principal),
        None,
        AccessTokenInvocation {
            work_context,
            provenance: InvocationProvenance::Automated,
        },
        service_principal.id.clone(),
        scopes,
    )
    .await
}

pub(super) fn client_credentials_principal(
    authorization_server: &ResourceAuthorizationServer,
    client_id: &OAuthClientId,
    tenant: &TenantId,
    scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<Principal> {
    let subject = TokenSubject::new(client_id.as_str())?;
    Ok(Principal {
        id: PrincipalId::new(format!("{}#{subject}", authorization_server.issuer))?,
        kind: PrincipalKind::Service,
        issuer: authorization_server.issuer.clone(),
        subject,
        tenant: Some(tenant.clone()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: scopes.clone(),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(Utc::now()),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn issue_access_token(
    catalog: &GatewayCatalog,
    authorization_server: &ResourceAuthorizationServer,
    protected_resource: &ProtectedResourceId,
    subject: &TokenSubject,
    client_id: &OAuthClientId,
    principal_kind: PrincipalKind,
    principal: Option<&Principal>,
    service_tenant: Option<&TenantId>,
    invocation: AccessTokenInvocation,
    principal_id: PrincipalId,
    scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<IssuedAccessToken> {
    let signing_key = access_token_signing_key(
        catalog,
        &authorization_server.access_token_signing_key,
        SecretPurpose::JwksPrivateKey,
    )
    .await?;
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::seconds(ACCESS_TOKEN_TTL_SECONDS))
        .ok_or_else(|| anyhow!("access token expiration overflow"))?;
    let jwt_id = JwtId::new(uuid::Uuid::new_v4().to_string())?;
    let scope = (!scopes.is_empty()).then(|| {
        scopes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    });
    let claims = AccessTokenClaims {
        iss: authorization_server.issuer.to_string(),
        sub: subject.to_string(),
        principal_id: principal_id.to_string(),
        client_id: client_id.to_string(),
        work_context: invocation.work_context.to_string(),
        invocation_mode: invocation.provenance.mode(),
        initiator: invocation.provenance.initiator().map(ToString::to_string),
        delegation_id: match &invocation.provenance {
            InvocationProvenance::Delegated { delegation_id, .. } => {
                Some(delegation_id.to_string())
            }
            InvocationProvenance::Direct { .. } | InvocationProvenance::Automated => None,
        },
        aud: protected_resource.to_string(),
        exp: unix_seconds(expires_at.timestamp())?,
        nbf: unix_seconds(now.timestamp())?,
        iat: unix_seconds(now.timestamp())?,
        jti: jwt_id.to_string(),
        principal_kind,
        scope,
        groups: principal
            .map(|principal| {
                principal
                    .groups
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        roles: principal
            .map(|principal| {
                principal
                    .roles
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        tenant: principal
            .and_then(|principal| principal.tenant.as_ref())
            .or(service_tenant)
            .map(ToString::to_string),
        data_labels: principal
            .map(|principal| {
                principal
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        principal_assurances: principal
            .map(|principal| {
                principal
                    .assurances
                    .iter()
                    .map(|assurance| match assurance {
                        veoveo_mcp_contract::PrincipalAssurance::UsPerson => {
                            "us_person".to_string()
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(authorization_server.access_token_key_id.to_string());
    let access_token = encode(&header, &claims, &signing_key)?;
    Ok(IssuedAccessToken {
        access_token,
        jwt_id,
    })
}

pub(super) async fn authorization_server_jwks_from_signing_key(
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

async fn access_token_signing_key(
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

fn unix_seconds(value: i64) -> anyhow::Result<u64> {
    u64::try_from(value).map_err(|_| anyhow!("timestamp before Unix epoch"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_access_token_debug_redacts_bearer_value() {
        let issued = IssuedAccessToken {
            access_token: "sensitive-access-token".to_owned(),
            jwt_id: JwtId::new("test-jwt-id").unwrap(),
        };
        let debug = format!("{issued:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("sensitive-access-token"));
    }
}
