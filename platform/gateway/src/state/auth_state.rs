use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::{
    AuthorizationServerId, GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest,
    GatewayJwtRevocation, GatewayProfileId, JwtId, OAuthAuthorizationCode, OAuthClientId,
    OAuthStateValue, TokenIssuer,
};
use veoveo_platform_store::{
    GatewayAuthorizationCodeStateRecord, GatewayAuthorizationRequestRecord,
    GatewayJwtRevocationRecord, GatewayReplayKind, GatewayReplayRecord, OpenObject,
    gateway_authorization_code_record_id, gateway_authorization_request_record_id,
    gateway_jwt_revocation_record_id, gateway_replay_record_id,
};

use super::GatewayState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayReplayRetentionSummary {
    pub client_assertion_jtis_deleted: u64,
    pub id_jag_jtis_deleted: u64,
}

impl GatewayState {
    pub async fn record_jwt_revocation(&self, revocation: &GatewayJwtRevocation) -> Result<()> {
        let id = gateway_jwt_revocation_record_id(
            revocation.profile.as_str(),
            revocation.issuer.as_str(),
            revocation.jwt_id.as_str(),
        );
        self.platform
            .upsert_gateway_jwt_revocation(GatewayJwtRevocationRecord {
                id,
                profile: revocation.profile.to_string(),
                issuer: revocation.issuer.to_string(),
                jwt_id: revocation.jwt_id.to_string(),
                revoked_at: revocation.revoked_at,
                expires_at: revocation.expires_at,
                reason: revocation.reason.clone(),
                payload: serialize_object(revocation)?,
            })
            .await
            .context("failed to persist gateway JWT revocation")
    }

    pub async fn jwt_revocation(
        &self,
        profile: &GatewayProfileId,
        issuer: &TokenIssuer,
        jwt_id: &JwtId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayJwtRevocation>> {
        let id =
            gateway_jwt_revocation_record_id(profile.as_str(), issuer.as_str(), jwt_id.as_str());
        self.platform
            .gateway_jwt_revocation(id, now)
            .await
            .context("failed to read gateway JWT revocation")?
            .map(|record| deserialize_object(record.payload))
            .transpose()
    }

    pub async fn prune_expired_jwt_revocations(&self, now: DateTime<Utc>) -> Result<u64> {
        self.platform
            .prune_expired_gateway_jwt_revocations(now)
            .await
            .context("failed to prune expired gateway JWT revocations")
    }

    pub async fn record_client_assertion_jti(
        &self,
        authorization_server: &AuthorizationServerId,
        client_id: &OAuthClientId,
        jwt_id: &JwtId,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        self.register_replay_id(
            GatewayReplayKind::ClientAssertion,
            authorization_server,
            client_id,
            jwt_id,
            expires_at,
            now,
        )
        .await
    }

    pub async fn record_id_jag_jti(
        &self,
        authorization_server: &AuthorizationServerId,
        client_id: &OAuthClientId,
        jwt_id: &JwtId,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        self.register_replay_id(
            GatewayReplayKind::IdJag,
            authorization_server,
            client_id,
            jwt_id,
            expires_at,
            now,
        )
        .await
    }

    pub async fn prune_expired_replay_ids(
        &self,
        now: DateTime<Utc>,
    ) -> Result<GatewayReplayRetentionSummary> {
        let client_assertion_jtis_deleted = self
            .platform
            .prune_expired_gateway_replay_ids(GatewayReplayKind::ClientAssertion, now)
            .await
            .context("failed to prune client assertion replay identifiers")?;
        let id_jag_jtis_deleted = self
            .platform
            .prune_expired_gateway_replay_ids(GatewayReplayKind::IdJag, now)
            .await
            .context("failed to prune ID-JAG replay identifiers")?;
        Ok(GatewayReplayRetentionSummary {
            client_assertion_jtis_deleted,
            id_jag_jtis_deleted,
        })
    }

    pub async fn record_authorization_request(
        &self,
        request: &GatewayAuthorizationRequest,
    ) -> Result<()> {
        self.platform
            .create_gateway_authorization_request(GatewayAuthorizationRequestRecord {
                id: gateway_authorization_request_record_id(request.idp_state.as_str()),
                idp_state: request.idp_state.to_string(),
                profile: request.profile.to_string(),
                oauth_client_id: request.oauth_client_id.to_string(),
                work_context: request.work_context.to_string(),
                oidc_client: request.oidc_client.to_string(),
                redirect_uri: request.redirect_uri.to_string(),
                created_at: request.created_at,
                expires_at: request.expires_at,
                payload: serialize_object(request)?,
            })
            .await
            .context("failed to persist OAuth authorization request")
    }

    pub async fn consume_authorization_request(
        &self,
        idp_state: &OAuthStateValue,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationRequest>> {
        self.platform
            .consume_gateway_authorization_request(
                gateway_authorization_request_record_id(idp_state.as_str()),
                now,
            )
            .await
            .context("failed to consume OAuth authorization request")?
            .map(|record| deserialize_object(record.payload))
            .transpose()
    }

    pub async fn record_authorization_code(
        &self,
        code: &GatewayAuthorizationCodeRecord,
    ) -> Result<()> {
        self.platform
            .create_gateway_authorization_code(GatewayAuthorizationCodeStateRecord {
                id: gateway_authorization_code_record_id(code.code.as_str()),
                code: code.code.to_string(),
                profile: code.profile.to_string(),
                oauth_client_id: code.oauth_client_id.to_string(),
                work_context: code.work_context.to_string(),
                oidc_client: code.oidc_client.to_string(),
                principal: code.principal.id.to_string(),
                redirect_uri: code.redirect_uri.to_string(),
                issued_at: code.issued_at,
                expires_at: code.expires_at,
                consumed_at: code.consumed_at,
                payload: serialize_object(code)?,
            })
            .await
            .context("failed to persist OAuth authorization code")
    }

    pub async fn consume_authorization_code(
        &self,
        code: &OAuthAuthorizationCode,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationCodeRecord>> {
        let record = self
            .platform
            .consume_gateway_authorization_code(
                gateway_authorization_code_record_id(code.as_str()),
                now,
            )
            .await
            .context("failed to consume OAuth authorization code")?;
        record
            .map(|record| {
                let mut code: GatewayAuthorizationCodeRecord = deserialize_object(record.payload)?;
                code.consumed_at = Some(now);
                Ok(code)
            })
            .transpose()
    }

    pub async fn prune_expired_authorization_records(&self, now: DateTime<Utc>) -> Result<u64> {
        self.platform
            .prune_expired_gateway_authorization_records(now)
            .await
            .context("failed to prune expired OAuth authorization records")
    }

    async fn register_replay_id(
        &self,
        kind: GatewayReplayKind,
        authorization_server: &AuthorizationServerId,
        client_id: &OAuthClientId,
        jwt_id: &JwtId,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        self.platform
            .register_gateway_replay_id(
                GatewayReplayRecord {
                    id: gateway_replay_record_id(
                        kind,
                        authorization_server.as_str(),
                        client_id.as_str(),
                        jwt_id.as_str(),
                    ),
                    kind,
                    authorization_server: authorization_server.to_string(),
                    client_id: client_id.to_string(),
                    jwt_id: jwt_id.to_string(),
                    seen_at: now,
                    expires_at,
                },
                now,
            )
            .await
            .context("failed to atomically register gateway replay identifier")
    }
}

fn serialize_object(value: impl Serialize) -> Result<OpenObject> {
    let value = serde_json::to_value(value)?;
    let serde_json::Value::Object(object) = value else {
        anyhow::bail!("gateway runtime value did not serialize as an object");
    };
    Ok(OpenObject::new(object.into_iter().collect()))
}

fn deserialize_object<T: serde::de::DeserializeOwned>(value: OpenObject) -> Result<T> {
    serde_json::from_value(serde_json::to_value(value)?).map_err(Into::into)
}
