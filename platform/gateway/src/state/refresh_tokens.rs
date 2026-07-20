use std::{collections::BTreeSet, num::NonZeroU32, sync::Arc};

use anyhow::{Context, Result, anyhow};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD},
};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use chrono::{DateTime, TimeDelta, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use veoveo_mcp_contract::{
    AuthAuditEvent, AuthorizationServerId, GatewayProfileId, GatewayRefreshFamilyId,
    GatewayRefreshGrant, OAuthClientId, OAuthRefreshToken, Principal, ScopeName, WorkContextId,
};
use veoveo_platform_store::{
    GatewayRefreshFamilyRecord, GatewayRefreshRotationOutcome, GatewayRefreshTokenRecord,
    OpenObject, RecordIdKey, RedactedSecret, gateway_refresh_family_record_id,
    gateway_refresh_token_record_id,
};

use super::GatewayState;

pub const REFRESH_TOKEN_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const REFRESH_DELIVERY_NONCE_BYTES: usize = 24;
const MAX_REFRESH_DELIVERY_WINDOW_SECONDS: u32 = 30;

#[derive(Clone)]
pub struct RefreshTokenDeliveryCipher(Arc<XChaCha20Poly1305>);

impl std::fmt::Debug for RefreshTokenDeliveryCipher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RefreshTokenDeliveryCipher")
            .finish_non_exhaustive()
    }
}

impl RefreshTokenDeliveryCipher {
    pub fn from_base64(encoded: &str) -> Result<Self> {
        let key = BASE64_STANDARD
            .decode(encoded.trim())
            .context("refresh delivery key must be standard base64")?;
        let key: [u8; 32] = key
            .try_into()
            .map_err(|_| anyhow!("refresh delivery key must decode to exactly 32 bytes"))?;
        Self::new(&key)
    }

    pub fn new(key: &[u8; 32]) -> Result<Self> {
        Ok(Self(Arc::new(
            XChaCha20Poly1305::new_from_slice(key)
                .map_err(|_| anyhow!("refresh delivery key must be exactly 32 bytes"))?,
        )))
    }

    fn seal(&self, token: &OAuthRefreshToken, aad: &[u8]) -> Result<RedactedSecret> {
        let mut nonce = [0_u8; REFRESH_DELIVERY_NONCE_BYTES];
        getrandom::fill(&mut nonce).context("failed to generate refresh delivery nonce")?;
        let ciphertext = self
            .0
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: token.as_str().as_bytes(),
                    aad,
                },
            )
            .map_err(|_| anyhow!("failed to encrypt refresh delivery envelope"))?;
        let mut envelope = Vec::with_capacity(nonce.len() + ciphertext.len());
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&ciphertext);
        Ok(RedactedSecret::new(URL_SAFE_NO_PAD.encode(envelope)))
    }

    fn open(&self, envelope: &RedactedSecret, aad: &[u8]) -> Result<OAuthRefreshToken> {
        let envelope = URL_SAFE_NO_PAD
            .decode(envelope.expose_secret())
            .context("refresh delivery envelope is not base64url")?;
        let (nonce, ciphertext) = envelope
            .split_at_checked(REFRESH_DELIVERY_NONCE_BYTES)
            .context("refresh delivery envelope is truncated")?;
        let nonce: [u8; REFRESH_DELIVERY_NONCE_BYTES] = nonce
            .try_into()
            .context("refresh delivery nonce is invalid")?;
        let plaintext = self
            .0
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| anyhow!("refresh delivery envelope authentication failed"))?;
        let token = String::from_utf8(plaintext).context("refresh delivery token is not UTF-8")?;
        OAuthRefreshToken::new(token).map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GatewayRefreshDeliveryWindow(TimeDelta);

impl GatewayRefreshDeliveryWindow {
    pub fn from_seconds(seconds: NonZeroU32) -> Result<Self> {
        if seconds.get() > MAX_REFRESH_DELIVERY_WINDOW_SECONDS {
            anyhow::bail!(
                "refresh delivery window must be at most {MAX_REFRESH_DELIVERY_WINDOW_SECONDS} seconds"
            );
        }
        Ok(Self(TimeDelta::seconds(i64::from(seconds.get()))))
    }

    fn expires_at(self, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
        now.checked_add_signed(self.0)
            .context("refresh delivery window overflow")
    }
}

pub struct GatewayRefreshRotationRequest<'a> {
    pub authorization_server: &'a AuthorizationServerId,
    pub profile: &'a GatewayProfileId,
    pub oauth_client_id: &'a OAuthClientId,
    pub now: DateTime<Utc>,
    pub delivery_window: GatewayRefreshDeliveryWindow,
    pub delivery_cipher: &'a RefreshTokenDeliveryCipher,
    pub success_audit: &'a AuthAuditEvent,
    pub duplicate_delivery_audit: &'a AuthAuditEvent,
}

#[derive(Clone)]
pub struct IssuedGatewayRefreshToken {
    pub token: OAuthRefreshToken,
    pub grant: GatewayRefreshGrant,
}

impl std::fmt::Debug for IssuedGatewayRefreshToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IssuedGatewayRefreshToken")
            .field("token", &"[REDACTED]")
            .field("grant", &self.grant)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum GatewayRefreshExchange {
    Rotated(IssuedGatewayRefreshToken),
    DuplicateDelivery(IssuedGatewayRefreshToken),
    ReplayDetected { grant: GatewayRefreshGrant },
    Invalid,
}

impl GatewayState {
    pub async fn refresh_token_grant(
        &self,
        token: &OAuthRefreshToken,
        authorization_server: &AuthorizationServerId,
        profile: &GatewayProfileId,
        oauth_client_id: &OAuthClientId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayRefreshGrant>> {
        let hash = refresh_token_hash(token);
        let Some((current, family)) = self
            .platform
            .gateway_refresh_grant_by_hash(&hash)
            .await
            .context("failed to read gateway refresh-token grant")?
        else {
            return Ok(None);
        };
        if family.authorization_server != authorization_server.as_str()
            || family.profile != profile.as_str()
            || family.oauth_client_id != oauth_client_id.as_str()
            || current.expires_at <= now
            || family.expires_at <= now
            || family.revoked_at.is_some()
        {
            return Ok(None);
        }
        grant_from_family(family).map(Some)
    }

    pub async fn revoke_refresh_token_family(
        &self,
        token: &OAuthRefreshToken,
        authorization_server: &AuthorizationServerId,
        profile: &GatewayProfileId,
        oauth_client_id: &OAuthClientId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayRefreshGrant>> {
        self.platform
            .revoke_gateway_refresh_family_by_hash(
                &refresh_token_hash(token),
                authorization_server.as_str(),
                profile.as_str(),
                oauth_client_id.as_str(),
                now,
            )
            .await
            .context("failed to revoke gateway refresh-token family")?
            .map(grant_from_family)
            .transpose()
    }

    pub async fn issue_refresh_token(
        &self,
        authorization_server: &AuthorizationServerId,
        profile: &GatewayProfileId,
        oauth_client_id: &OAuthClientId,
        work_context: &WorkContextId,
        principal: &Principal,
        scopes: &BTreeSet<ScopeName>,
        now: DateTime<Utc>,
    ) -> Result<IssuedGatewayRefreshToken> {
        let expires_at = now
            .checked_add_signed(TimeDelta::seconds(REFRESH_TOKEN_TTL_SECONDS))
            .context("refresh-token lifetime overflow")?;
        let family_uuid = Uuid::now_v7();
        let family_id = GatewayRefreshFamilyId::new(family_uuid.to_string())?;
        let family_record_id = gateway_refresh_family_record_id(family_uuid);
        let token = random_refresh_token()?;
        let token_record = GatewayRefreshTokenRecord {
            id: gateway_refresh_token_record_id(Uuid::now_v7()),
            family: family_record_id.clone(),
            token_hash: refresh_token_hash(&token),
            generation: 0,
            issued_at: now,
            expires_at,
            consumed_at: None,
            replacement: None,
            replay_detected_at: None,
            delivery_envelope: None,
            delivery_expires_at: None,
        };
        let family = GatewayRefreshFamilyRecord {
            id: family_record_id,
            authorization_server: authorization_server.to_string(),
            profile: profile.to_string(),
            oauth_client_id: oauth_client_id.to_string(),
            work_context: work_context.to_string(),
            principal_id: principal.id.to_string(),
            tenant: principal.tenant.as_ref().map(ToString::to_string),
            scopes: scopes.iter().map(ToString::to_string).collect(),
            principal: serialize_principal(principal)?,
            current_generation: 0,
            issued_at: now,
            expires_at,
            revoked_at: None,
            revocation_reason: None,
        };
        self.platform
            .create_gateway_refresh_family(family, token_record)
            .await
            .context("failed to create gateway refresh-token family")?;
        Ok(IssuedGatewayRefreshToken {
            token,
            grant: GatewayRefreshGrant {
                family_id,
                authorization_server: authorization_server.clone(),
                profile: profile.clone(),
                oauth_client_id: oauth_client_id.clone(),
                work_context: work_context.clone(),
                principal: principal.clone(),
                scopes: scopes.clone(),
                generation: 0,
                issued_at: now,
                expires_at,
            },
        })
    }

    pub async fn rotate_refresh_token(
        &self,
        token: &OAuthRefreshToken,
        request: GatewayRefreshRotationRequest<'_>,
    ) -> Result<GatewayRefreshExchange> {
        let hash = refresh_token_hash(token);
        let Some((current, family)) = self
            .platform
            .gateway_refresh_grant_by_hash(&hash)
            .await
            .context("failed to read gateway refresh-token grant")?
        else {
            return Ok(GatewayRefreshExchange::Invalid);
        };
        if family.authorization_server != request.authorization_server.as_str()
            || family.profile != request.profile.as_str()
            || family.oauth_client_id != request.oauth_client_id.as_str()
        {
            return Ok(GatewayRefreshExchange::Invalid);
        }
        if current.expires_at <= request.now || family.expires_at <= request.now {
            return Ok(GatewayRefreshExchange::Invalid);
        }
        let success_audit = super::audit::canonical_auth_record(request.success_audit)
            .context("failed to prepare refresh-token success audit")?;
        let duplicate_delivery_audit =
            super::audit::canonical_auth_record(request.duplicate_delivery_audit)
                .context("failed to prepare duplicate refresh delivery audit")?;
        let next_generation = current
            .generation
            .checked_add(1)
            .context("refresh-token generation overflow")?;
        let replacement_token = random_refresh_token()?;
        let delivery_expires_at = std::cmp::min(
            request.delivery_window.expires_at(request.now)?,
            family.expires_at,
        );
        let delivery_envelope = request.delivery_cipher.seal(
            &replacement_token,
            &refresh_delivery_aad(&family, next_generation)?,
        )?;
        let replacement = GatewayRefreshTokenRecord {
            id: gateway_refresh_token_record_id(Uuid::now_v7()),
            family: family.id.clone(),
            token_hash: refresh_token_hash(&replacement_token),
            generation: next_generation,
            issued_at: request.now,
            expires_at: family.expires_at,
            consumed_at: None,
            replacement: None,
            replay_detected_at: None,
            delivery_envelope: Some(delivery_envelope),
            delivery_expires_at: Some(delivery_expires_at),
        };
        match self
            .platform
            .rotate_gateway_refresh_token(
                &hash,
                replacement,
                request.now,
                success_audit,
                duplicate_delivery_audit,
            )
            .await
            .context("failed to rotate gateway refresh token")?
        {
            GatewayRefreshRotationOutcome::Rotated(rotation) => {
                Ok(GatewayRefreshExchange::Rotated(IssuedGatewayRefreshToken {
                    token: replacement_token,
                    grant: grant_from_family(rotation.family)?,
                }))
            }
            GatewayRefreshRotationOutcome::Redelivered(redelivery) => {
                let envelope = redelivery
                    .replacement
                    .delivery_envelope
                    .as_ref()
                    .context("duplicate refresh delivery is missing its encrypted envelope")?;
                let token = request.delivery_cipher.open(
                    envelope,
                    &refresh_delivery_aad(&redelivery.family, redelivery.replacement.generation)?,
                )?;
                if refresh_token_hash(&token) != redelivery.replacement.token_hash {
                    anyhow::bail!("duplicate refresh delivery token does not match its record");
                }
                Ok(GatewayRefreshExchange::DuplicateDelivery(
                    IssuedGatewayRefreshToken {
                        token,
                        grant: grant_from_family(redelivery.family)?,
                    },
                ))
            }
            GatewayRefreshRotationOutcome::ReplayDetected(family) => {
                Ok(GatewayRefreshExchange::ReplayDetected {
                    grant: grant_from_family(*family)?,
                })
            }
            GatewayRefreshRotationOutcome::Invalid => Ok(GatewayRefreshExchange::Invalid),
        }
    }

    pub async fn prune_expired_refresh_tokens(
        &self,
        now: DateTime<Utc>,
    ) -> Result<veoveo_platform_store::GatewayRefreshRetentionSummary> {
        self.platform
            .prune_expired_gateway_refresh_state(now)
            .await
            .context("failed to prune expired gateway refresh-token state")
    }

    pub async fn clear_expired_refresh_delivery_envelopes(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        self.platform
            .clear_expired_gateway_refresh_delivery_envelopes(now)
            .await
            .context("failed to clear expired gateway refresh delivery envelopes")
    }
}

fn grant_from_family(family: GatewayRefreshFamilyRecord) -> Result<GatewayRefreshGrant> {
    Ok(GatewayRefreshGrant {
        family_id: family_id(&family)?,
        authorization_server: AuthorizationServerId::new(family.authorization_server)?,
        profile: GatewayProfileId::new(family.profile)?,
        oauth_client_id: OAuthClientId::new(family.oauth_client_id)?,
        work_context: WorkContextId::new(family.work_context)?,
        principal: serde_json::from_value(serde_json::to_value(family.principal)?)?,
        scopes: family
            .scopes
            .into_iter()
            .map(ScopeName::new)
            .collect::<Result<_, _>>()?,
        generation: u64::try_from(family.current_generation)
            .context("stored refresh-token generation is negative")?,
        issued_at: family.issued_at,
        expires_at: family.expires_at,
    })
}

fn family_id(family: &GatewayRefreshFamilyRecord) -> Result<GatewayRefreshFamilyId> {
    let RecordIdKey::Uuid(value) = &family.id.key else {
        anyhow::bail!("stored refresh-token family id is not a UUID");
    };
    GatewayRefreshFamilyId::new(value.to_string()).map_err(Into::into)
}

fn refresh_delivery_aad(family: &GatewayRefreshFamilyRecord, generation: i64) -> Result<Vec<u8>> {
    Ok(format!(
        "veoveo-refresh-delivery-v2\0{}\0{}\0{}\0{}\0{}\0{generation}",
        family.authorization_server,
        family.profile,
        family.oauth_client_id,
        family.work_context,
        family_id(family)?,
    )
    .into_bytes())
}

fn serialize_principal(principal: &Principal) -> Result<OpenObject> {
    let value = serde_json::to_value(principal)?;
    let serde_json::Value::Object(object) = value else {
        anyhow::bail!("gateway principal did not serialize as an object");
    };
    Ok(OpenObject::new(object.into_iter().collect()))
}

fn random_refresh_token() -> Result<OAuthRefreshToken> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("failed to generate refresh-token entropy")?;
    OAuthRefreshToken::new(URL_SAFE_NO_PAD.encode(bytes)).map_err(Into::into)
}

fn refresh_token_hash(token: &OAuthRefreshToken) -> String {
    let digest = Sha256::digest(token.as_str().as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &[u8; 32] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn delivery_envelope_round_trips_only_with_its_bound_identity() {
        let cipher = RefreshTokenDeliveryCipher::new(TEST_KEY).unwrap();
        let token = OAuthRefreshToken::new("r".repeat(43)).unwrap();
        let envelope = cipher.seal(&token, b"profile-a/family-a/1").unwrap();

        assert_ne!(envelope.expose_secret(), token.as_str());
        assert_eq!(
            cipher
                .open(&envelope, b"profile-a/family-a/1")
                .unwrap()
                .as_str(),
            token.as_str(),
        );
        assert!(cipher.open(&envelope, b"profile-b/family-a/1").is_err());

        let debug = format!("{cipher:?} {envelope:?}");
        assert!(!debug.contains(token.as_str()));
        assert!(!debug.contains(envelope.expose_secret()));
        assert!(!debug.contains("0123456789abcdef"));
    }

    #[test]
    fn delivery_configuration_rejects_wrong_key_length_and_long_windows() {
        let short_key = BASE64_STANDARD.encode([0_u8; 31]);
        assert!(RefreshTokenDeliveryCipher::from_base64(&short_key).is_err());
        assert!(GatewayRefreshDeliveryWindow::from_seconds(NonZeroU32::new(31).unwrap()).is_err());
    }
}
