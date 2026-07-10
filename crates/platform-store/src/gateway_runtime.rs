use std::{collections::BTreeMap, time::Duration};

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, RecordIdKey};
use uuid::Uuid;

use crate::identity::PLATFORM_ID_NAMESPACE;
use crate::store::primary_transaction_error;
use crate::{
    AuditEventRecord, GatewayAuthorizationCodeStateRecord, GatewayAuthorizationRequestRecord,
    GatewayJwtRevocationRecord, GatewayRefreshFamilyRecord, GatewayRefreshTokenRecord,
    GatewayReplayKind, GatewayReplayRecord, GatewayResourceSubscriptionRecord, OpenObject,
    OutboxDraft, PlatformStore, StoreError,
};

const GATEWAY_POLICY_AUDIT: &str = "gateway_policy";
const GATEWAY_AUTH_AUDIT: &str = "gateway_auth";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatewayAuditKind {
    Policy,
    Auth,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GatewayRefreshRotation {
    pub family: GatewayRefreshFamilyRecord,
    pub consumed: GatewayRefreshTokenRecord,
    pub replacement: GatewayRefreshTokenRecord,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GatewayRefreshRedelivery {
    pub family: GatewayRefreshFamilyRecord,
    pub replacement: GatewayRefreshTokenRecord,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GatewayRefreshRotationOutcome {
    Rotated(Box<GatewayRefreshRotation>),
    Redelivered(Box<GatewayRefreshRedelivery>),
    ReplayDetected(Box<GatewayRefreshFamilyRecord>),
    Invalid,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GatewayRefreshRetentionSummary {
    pub delivery_envelopes_deleted: u64,
    pub tokens_deleted: u64,
    pub families_deleted: u64,
}

impl GatewayAuditKind {
    pub const fn resource_type(self) -> &'static str {
        match self {
            Self::Policy => GATEWAY_POLICY_AUDIT,
            Self::Auth => GATEWAY_AUTH_AUDIT,
        }
    }
}

pub fn gateway_resource_subscription_record_id(
    profile: &str,
    owner: &str,
    upstream_server: &str,
    resource_uri: &str,
) -> RecordId {
    deterministic_gateway_record_id(
        "gateway_resource_subscription",
        "subscription",
        &[profile, owner, upstream_server, resource_uri],
    )
}

pub fn gateway_jwt_revocation_record_id(profile: &str, issuer: &str, jwt_id: &str) -> RecordId {
    deterministic_gateway_record_id(
        "gateway_jwt_revocation",
        "jwt-revocation",
        &[profile, issuer, jwt_id],
    )
}

pub fn gateway_replay_record_id(
    kind: GatewayReplayKind,
    authorization_server: &str,
    client_id: &str,
    jwt_id: &str,
) -> RecordId {
    let kind = match kind {
        GatewayReplayKind::ClientAssertion => "client-assertion",
        GatewayReplayKind::IdJag => "id-jag",
    };
    deterministic_gateway_record_id(
        "gateway_replay_id",
        kind,
        &[authorization_server, client_id, jwt_id],
    )
}

pub fn gateway_authorization_request_record_id(idp_state: &str) -> RecordId {
    deterministic_gateway_record_id(
        "gateway_authorization_request",
        "authorization-request",
        &[idp_state],
    )
}

pub fn gateway_authorization_code_record_id(code: &str) -> RecordId {
    deterministic_gateway_record_id("gateway_authorization_code", "authorization-code", &[code])
}

pub fn gateway_refresh_family_record_id(id: Uuid) -> RecordId {
    RecordId::new("gateway_refresh_family", surrealdb::types::Uuid::from(id))
}

pub fn gateway_refresh_token_record_id(id: Uuid) -> RecordId {
    RecordId::new("gateway_refresh_token", surrealdb::types::Uuid::from(id))
}

impl PlatformStore {
    pub async fn upsert_gateway_resource_subscription(
        &self,
        record: GatewayResourceSubscriptionRecord,
    ) -> Result<(), StoreError> {
        let outbox = gateway_outbox(
            "gateway_subscription",
            &record.id,
            "gateway.resource_subscription.upserted",
            BTreeMap::from([
                ("profile".into(), serde_json::json!(&record.profile)),
                (
                    "upstream_server".into(),
                    serde_json::json!(&record.upstream_server),
                ),
            ]),
        );
        self.db
            .query("BEGIN TRANSACTION; UPSERT ONLY $record CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("record", record.id.clone()))
            .bind(("content", record))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn gateway_resource_subscription(
        &self,
        id: RecordId,
    ) -> Result<Option<GatewayResourceSubscriptionRecord>, StoreError> {
        Ok(self.db.select(id).await?)
    }

    pub async fn delete_gateway_resource_subscription(
        &self,
        id: RecordId,
    ) -> Result<(), StoreError> {
        let outbox = gateway_outbox(
            "gateway_subscription",
            &id,
            "gateway.resource_subscription.deleted",
            BTreeMap::new(),
        );
        self.db
            .query("BEGIN TRANSACTION; DELETE ONLY $record RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("record", id))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn upsert_gateway_jwt_revocation(
        &self,
        record: GatewayJwtRevocationRecord,
    ) -> Result<(), StoreError> {
        let outbox = gateway_outbox(
            "gateway_jwt_revocation",
            &record.id,
            "gateway.jwt.revoked",
            BTreeMap::from([("profile".into(), serde_json::json!(&record.profile))]),
        );
        self.db
            .query("BEGIN TRANSACTION; UPSERT ONLY $record CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("record", record.id.clone()))
            .bind(("content", record))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn gateway_jwt_revocation(
        &self,
        id: RecordId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayJwtRevocationRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $record WHERE expires_at > $now;")
            .bind(("record", id))
            .bind(("now", now))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn prune_expired_gateway_jwt_revocations(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        delete_count(
            self,
            "DELETE gateway_jwt_revocation WHERE expires_at <= $now RETURN BEFORE;",
            now,
        )
        .await
    }

    /// Atomically claims a replay identifier. A live existing identifier wins,
    /// while an expired record may be replaced by exactly one caller.
    pub async fn register_gateway_replay_id(
        &self,
        record: GatewayReplayRecord,
        now: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        const MAX_ATTEMPTS: u32 = 8;
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .db
                .query("UPSERT ONLY $record CONTENT $content WHERE expires_at = NONE OR expires_at <= $now RETURN AFTER;")
                .bind(("record", record.id.clone()))
                .bind(("content", record.clone()))
                .bind(("now", now))
                .await
                .and_then(|response| response.check());
            match response {
                Ok(mut response) => {
                    let claimed: Option<GatewayReplayRecord> = response.take(0)?;
                    return Ok(claimed.is_some());
                }
                Err(error)
                    if is_retryable_transaction_conflict(&error) && attempt + 1 < MAX_ATTEMPTS =>
                {
                    retry_backoff(attempt).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("gateway replay registration attempts return or fail")
    }

    pub async fn prune_expired_gateway_replay_ids(
        &self,
        kind: GatewayReplayKind,
        now: DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let mut response = self
            .db
            .query(
                "DELETE gateway_replay_id WHERE kind = $kind AND expires_at <= $now RETURN BEFORE;",
            )
            .bind(("kind", kind))
            .bind(("now", now))
            .await?
            .check()?;
        let deleted: Vec<GatewayReplayRecord> = response.take(0)?;
        u64::try_from(deleted.len()).map_err(|_| StoreError::MissingRecord {
            operation: "gateway replay retention count conversion",
        })
    }

    pub async fn create_gateway_authorization_request(
        &self,
        record: GatewayAuthorizationRequestRecord,
    ) -> Result<(), StoreError> {
        self.db
            .query("CREATE ONLY $record CONTENT $content RETURN NONE;")
            .bind(("record", record.id.clone()))
            .bind(("content", record))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn consume_gateway_authorization_request(
        &self,
        id: RecordId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationRequestRecord>, StoreError> {
        const MAX_ATTEMPTS: u32 = 8;
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .db
                .query("DELETE ONLY $record WHERE expires_at > $now RETURN BEFORE;")
                .bind(("record", id.clone()))
                .bind(("now", now))
                .await
                .and_then(|response| response.check());
            match response {
                Ok(mut response) => return Ok(response.take(0)?),
                Err(error)
                    if is_retryable_transaction_conflict(&error) && attempt + 1 < MAX_ATTEMPTS =>
                {
                    retry_backoff(attempt).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("authorization request consume attempts return or fail")
    }

    pub async fn create_gateway_authorization_code(
        &self,
        record: GatewayAuthorizationCodeStateRecord,
    ) -> Result<(), StoreError> {
        self.db
            .query("CREATE ONLY $record CONTENT $content RETURN NONE;")
            .bind(("record", record.id.clone()))
            .bind(("content", record))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn consume_gateway_authorization_code(
        &self,
        id: RecordId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationCodeStateRecord>, StoreError> {
        const MAX_ATTEMPTS: u32 = 8;
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .db
                .query("UPDATE ONLY $record SET consumed_at = $now, payload.consumed_at = $now WHERE expires_at > $now AND consumed_at = NONE RETURN BEFORE;")
                .bind(("record", id.clone()))
                .bind(("now", now))
                .await
                .and_then(|response| response.check());
            match response {
                Ok(mut response) => return Ok(response.take(0)?),
                Err(error)
                    if is_retryable_transaction_conflict(&error) && attempt + 1 < MAX_ATTEMPTS =>
                {
                    retry_backoff(attempt).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("authorization code consume attempts return or fail")
    }

    pub async fn prune_expired_gateway_authorization_records(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let mut response = self
            .db
            .query("DELETE gateway_authorization_request WHERE expires_at <= $now RETURN BEFORE; DELETE gateway_authorization_code WHERE expires_at <= $now RETURN BEFORE;")
            .bind(("now", now))
            .await?
            .check()?;
        let requests: Vec<GatewayAuthorizationRequestRecord> = response.take(0)?;
        let codes: Vec<GatewayAuthorizationCodeStateRecord> = response.take(1)?;
        u64::try_from(requests.len().saturating_add(codes.len())).map_err(|_| {
            StoreError::MissingRecord {
                operation: "gateway authorization retention count conversion",
            }
        })
    }

    pub async fn create_gateway_refresh_family(
        &self,
        family: GatewayRefreshFamilyRecord,
        token: GatewayRefreshTokenRecord,
    ) -> Result<(), StoreError> {
        validate_refresh_pair(&family, &token, 0)?;
        let outbox = gateway_outbox(
            "gateway_refresh_family",
            &family.id,
            "gateway.refresh_family.issued",
            BTreeMap::from([
                ("profile".into(), serde_json::json!(&family.profile)),
                (
                    "oauth_client_id".into(),
                    serde_json::json!(&family.oauth_client_id),
                ),
                ("expires_at".into(), serde_json::json!(family.expires_at)),
            ]),
        );
        self.db
            .query("BEGIN TRANSACTION; CREATE ONLY $family CONTENT $family_content RETURN NONE; CREATE ONLY $refresh_record CONTENT $refresh_content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("family", family.id.clone()))
            .bind(("family_content", family))
            .bind(("refresh_record", token.id.clone()))
            .bind(("refresh_content", token))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn gateway_refresh_grant_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<(GatewayRefreshTokenRecord, GatewayRefreshFamilyRecord)>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM gateway_refresh_token WHERE token_hash = $token_hash LIMIT 1;")
            .bind(("token_hash", token_hash.to_owned()))
            .await?
            .check()?;
        let mut tokens: Vec<GatewayRefreshTokenRecord> = response.take(0)?;
        let Some(token) = tokens.pop() else {
            return Ok(None);
        };
        let family: Option<GatewayRefreshFamilyRecord> =
            self.db.select(token.family.clone()).await?;
        Ok(family.map(|family| (token, family)))
    }

    pub async fn revoke_gateway_refresh_family_by_hash(
        &self,
        token_hash: &str,
        authorization_server: &str,
        profile: &str,
        oauth_client_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayRefreshFamilyRecord>, StoreError> {
        let Some((token, family)) = self.gateway_refresh_grant_by_hash(token_hash).await? else {
            return Ok(None);
        };
        if family.authorization_server != authorization_server
            || family.profile != profile
            || family.oauth_client_id != oauth_client_id
            || token.expires_at <= now
            || family.expires_at <= now
        {
            return Ok(None);
        }
        if family.revoked_at.is_some() {
            return Ok(Some(family));
        }
        let outbox = gateway_outbox(
            "gateway_refresh_family",
            &family.id,
            "gateway.refresh_family.revoked",
            BTreeMap::from([
                ("reason".into(), serde_json::json!("client_revocation")),
                ("generation".into(), serde_json::json!(token.generation)),
            ]),
        );
        const MAX_ATTEMPTS: u32 = 8;
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .db
                .query("BEGIN TRANSACTION; LET $revoked = (UPDATE ONLY $family SET revoked_at = $now, revocation_reason = 'client_revocation' WHERE revoked_at = NONE AND expires_at > $now RETURN AFTER); IF $revoked = NONE { THROW 'gateway_refresh_family_invalid'; }; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
                .bind(("family", family.id.clone()))
                .bind(("now", now))
                .bind(("outbox", outbox.clone()))
                .await
                .and_then(|mut response| match primary_transaction_error(response.take_errors()) {
                    Some(error) => Err(error),
                    None => Ok(()),
                });
            match response {
                Ok(()) => {
                    let mut revoked = family;
                    revoked.revoked_at = Some(now);
                    revoked.revocation_reason = Some("client_revocation".to_owned());
                    return Ok(Some(revoked));
                }
                Err(error)
                    if is_refresh_family_invalid_error(&error)
                        || is_retryable_transaction_failure(&error) =>
                {
                    let observed: Option<GatewayRefreshFamilyRecord> =
                        self.db.select(family.id.clone()).await?;
                    match observed {
                        Some(observed) if observed.revoked_at.is_some() => {
                            return Ok(Some(observed));
                        }
                        Some(observed) if observed.expires_at <= now => return Ok(None),
                        Some(_) if attempt + 1 < MAX_ATTEMPTS => {
                            retry_backoff(attempt).await;
                        }
                        _ => return Err(error.into()),
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("gateway refresh revocation attempts return or fail")
    }

    pub async fn rotate_gateway_refresh_token(
        &self,
        current_hash: &str,
        replacement: GatewayRefreshTokenRecord,
        now: DateTime<Utc>,
        success_audit: AuditEventRecord,
        duplicate_delivery_audit: AuditEventRecord,
    ) -> Result<GatewayRefreshRotationOutcome, StoreError> {
        debug_assert_eq!(
            success_audit.resource_type,
            GatewayAuditKind::Auth.resource_type()
        );
        debug_assert_eq!(
            duplicate_delivery_audit.resource_type,
            GatewayAuditKind::Auth.resource_type()
        );
        let Some((token, family)) = self.gateway_refresh_grant_by_hash(current_hash).await? else {
            return Ok(GatewayRefreshRotationOutcome::Invalid);
        };
        validate_refresh_pair(&family, &replacement, token.generation + 1)?;
        if token.consumed_at.is_some() || token.replay_detected_at.is_some() {
            return self
                .redeliver_or_revoke_gateway_refresh(
                    &token,
                    &family,
                    now,
                    &duplicate_delivery_audit,
                )
                .await;
        }
        if token.expires_at <= now
            || family.expires_at <= now
            || family.revoked_at.is_some()
            || token.generation != family.current_generation
        {
            return Ok(GatewayRefreshRotationOutcome::Invalid);
        }

        let outbox = gateway_outbox(
            "gateway_refresh_family",
            &family.id,
            "gateway.refresh_token.rotated",
            BTreeMap::from([
                (
                    "generation".into(),
                    serde_json::json!(replacement.generation),
                ),
                ("profile".into(), serde_json::json!(&family.profile)),
            ]),
        );
        let audit_outbox = gateway_audit_outbox(GatewayAuditKind::Auth, &success_audit);
        const MAX_ATTEMPTS: u32 = 8;
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .db
                .query("BEGIN TRANSACTION; LET $consumed = (UPDATE ONLY $current SET consumed_at = $now, replacement = $replacement, delivery_envelope = NONE, delivery_expires_at = NONE WHERE consumed_at = NONE AND replay_detected_at = NONE AND expires_at > $now RETURN AFTER); IF $consumed = NONE { THROW 'gateway_refresh_token_replay'; }; LET $family_updated = (UPDATE ONLY $family SET current_generation = $next_generation WHERE revoked_at = NONE AND expires_at > $now AND current_generation = $current_generation RETURN AFTER); IF $family_updated = NONE { THROW 'gateway_refresh_family_invalid'; }; CREATE ONLY $replacement CONTENT $replacement_content RETURN NONE; CREATE ONLY $audit_record CONTENT $audit_content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; CREATE outbox_event CONTENT $audit_outbox RETURN NONE; COMMIT TRANSACTION;")
                .bind(("current", token.id.clone()))
                .bind(("family", family.id.clone()))
                .bind(("replacement", replacement.id.clone()))
                .bind(("replacement_content", replacement.clone()))
                .bind(("now", now))
                .bind(("current_generation", token.generation))
                .bind(("next_generation", replacement.generation))
                .bind(("audit_record", success_audit.id.clone()))
                .bind(("audit_content", success_audit.clone()))
                .bind(("outbox", outbox.clone()))
                .bind(("audit_outbox", audit_outbox.clone()))
                .await
                .and_then(|mut response| match primary_transaction_error(response.take_errors()) {
                    Some(error) => Err(error),
                    None => Ok(()),
                });
            match response {
                Ok(_) => {
                    let mut rotated_family = family;
                    rotated_family.current_generation = replacement.generation;
                    let mut consumed = token;
                    consumed.consumed_at = Some(now);
                    consumed.replacement = Some(replacement.id.clone());
                    consumed.delivery_envelope = None;
                    consumed.delivery_expires_at = None;
                    return Ok(GatewayRefreshRotationOutcome::Rotated(Box::new(
                        GatewayRefreshRotation {
                            family: rotated_family,
                            consumed,
                            replacement,
                        },
                    )));
                }
                Err(error) if is_refresh_replay_error(&error) => {
                    let Some((observed_token, observed_family)) =
                        self.gateway_refresh_grant_by_hash(current_hash).await?
                    else {
                        return Ok(GatewayRefreshRotationOutcome::Invalid);
                    };
                    return self
                        .redeliver_or_revoke_gateway_refresh(
                            &observed_token,
                            &observed_family,
                            now,
                            &duplicate_delivery_audit,
                        )
                        .await;
                }
                Err(error) if is_refresh_family_invalid_error(&error) => {
                    return Ok(GatewayRefreshRotationOutcome::Invalid);
                }
                Err(error) if is_retryable_transaction_failure(&error) => {
                    if let Some(outcome) = self
                        .reconcile_gateway_refresh_rotation(
                            current_hash,
                            &replacement,
                            now,
                            &duplicate_delivery_audit,
                        )
                        .await?
                    {
                        return Ok(outcome);
                    }
                    if attempt + 1 < MAX_ATTEMPTS {
                        retry_backoff(attempt).await;
                        continue;
                    }
                    return Err(error.into());
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("gateway refresh rotation attempts return or fail")
    }

    pub async fn prune_expired_gateway_refresh_state(
        &self,
        now: DateTime<Utc>,
    ) -> Result<GatewayRefreshRetentionSummary, StoreError> {
        let mut response = self
            .db
            .query("UPDATE gateway_refresh_token SET delivery_envelope = NONE, delivery_expires_at = NONE WHERE delivery_expires_at != NONE AND delivery_expires_at <= $now RETURN BEFORE; DELETE gateway_refresh_token WHERE expires_at <= $now RETURN BEFORE; DELETE gateway_refresh_family WHERE expires_at <= $now RETURN BEFORE;")
            .bind(("now", now))
            .await?
            .check()?;
        let delivery_envelopes: Vec<GatewayRefreshTokenRecord> = response.take(0)?;
        let tokens: Vec<GatewayRefreshTokenRecord> = response.take(1)?;
        let families: Vec<GatewayRefreshFamilyRecord> = response.take(2)?;
        Ok(GatewayRefreshRetentionSummary {
            delivery_envelopes_deleted: delivery_envelopes.len() as u64,
            tokens_deleted: tokens.len() as u64,
            families_deleted: families.len() as u64,
        })
    }

    pub async fn clear_expired_gateway_refresh_delivery_envelopes(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let mut response = self
            .db
            .query("UPDATE gateway_refresh_token SET delivery_envelope = NONE, delivery_expires_at = NONE WHERE delivery_expires_at != NONE AND delivery_expires_at <= $now RETURN BEFORE;")
            .bind(("now", now))
            .await?
            .check()?;
        let cleared: Vec<GatewayRefreshTokenRecord> = response.take(0)?;
        u64::try_from(cleared.len()).map_err(|_| StoreError::MissingRecord {
            operation: "gateway refresh delivery-envelope retention count conversion",
        })
    }

    pub async fn record_gateway_audit_event(
        &self,
        kind: GatewayAuditKind,
        record: AuditEventRecord,
    ) -> Result<(), StoreError> {
        debug_assert_eq!(record.resource_type, kind.resource_type());
        let outbox = gateway_audit_outbox(kind, &record);
        self.db
            .query("BEGIN TRANSACTION; CREATE ONLY $record CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("record", record.id.clone()))
            .bind(("content", record))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn gateway_audit_events(
        &self,
        kind: GatewayAuditKind,
    ) -> Result<Vec<AuditEventRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM audit_event WHERE resource_type = $resource_type ORDER BY occurred_at ASC, id ASC;")
            .bind(("resource_type", kind.resource_type()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn gateway_audit_event_count(
        &self,
        kind: GatewayAuditKind,
    ) -> Result<u64, StoreError> {
        let mut response = self
            .db
            .query("SELECT VALUE count FROM (SELECT count() AS count FROM audit_event WHERE resource_type = $resource_type GROUP ALL);")
            .bind(("resource_type", kind.resource_type()))
            .await?
            .check()?;
        let counts: Vec<i64> = response.take(0)?;
        u64::try_from(counts.first().copied().unwrap_or_default()).map_err(|_| {
            StoreError::MissingRecord {
                operation: "gateway audit count conversion",
            }
        })
    }

    pub async fn delete_gateway_audit_events_before(
        &self,
        kind: GatewayAuditKind,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let mut response = self
            .db
            .query("DELETE audit_event WHERE resource_type = $resource_type AND occurred_at < $cutoff RETURN BEFORE;")
            .bind(("resource_type", kind.resource_type()))
            .bind(("cutoff", cutoff))
            .await?
            .check()?;
        let deleted: Vec<AuditEventRecord> = response.take(0)?;
        u64::try_from(deleted.len()).map_err(|_| StoreError::MissingRecord {
            operation: "gateway audit retention count conversion",
        })
    }
}

impl PlatformStore {
    async fn reconcile_gateway_refresh_rotation(
        &self,
        current_hash: &str,
        replacement: &GatewayRefreshTokenRecord,
        now: DateTime<Utc>,
        duplicate_delivery_audit: &AuditEventRecord,
    ) -> Result<Option<GatewayRefreshRotationOutcome>, StoreError> {
        let Some((token, family)) = self.gateway_refresh_grant_by_hash(current_hash).await? else {
            return Ok(Some(GatewayRefreshRotationOutcome::Invalid));
        };
        if token.consumed_at.is_some()
            && token.replacement.as_ref() == Some(&replacement.id)
            && family.current_generation == replacement.generation
            && family.revoked_at.is_none()
        {
            let persisted_replacement: Option<GatewayRefreshTokenRecord> =
                self.db.select(replacement.id.clone()).await?;
            if persisted_replacement.as_ref() == Some(replacement) {
                return Ok(Some(GatewayRefreshRotationOutcome::Rotated(Box::new(
                    GatewayRefreshRotation {
                        family,
                        consumed: token,
                        replacement: replacement.clone(),
                    },
                ))));
            }
        }
        if token.consumed_at.is_some() || token.replay_detected_at.is_some() {
            return self
                .redeliver_or_revoke_gateway_refresh(&token, &family, now, duplicate_delivery_audit)
                .await
                .map(Some);
        }
        if token.expires_at <= now
            || family.expires_at <= now
            || family.revoked_at.is_some()
            || token.generation != family.current_generation
        {
            return Ok(Some(GatewayRefreshRotationOutcome::Invalid));
        }
        Ok(None)
    }

    async fn redeliver_or_revoke_gateway_refresh(
        &self,
        token: &GatewayRefreshTokenRecord,
        family: &GatewayRefreshFamilyRecord,
        now: DateTime<Utc>,
        duplicate_delivery_audit: &AuditEventRecord,
    ) -> Result<GatewayRefreshRotationOutcome, StoreError> {
        let replacement: Option<GatewayRefreshTokenRecord> = match token.replacement.as_ref() {
            Some(replacement_id) => self.db.select(replacement_id.clone()).await?,
            None => None,
        };
        if token.consumed_at.is_some()
            && token.replay_detected_at.is_none()
            && family.revoked_at.is_none()
            && family.expires_at > now
            && replacement.as_ref().is_some_and(|replacement| {
                replacement.family == family.id
                    && replacement.generation == token.generation + 1
                    && replacement.generation == family.current_generation
                    && replacement.expires_at > now
                    && replacement.consumed_at.is_none()
                    && replacement.replay_detected_at.is_none()
                    && replacement.delivery_envelope.is_some()
                    && replacement
                        .delivery_expires_at
                        .is_some_and(|expires_at| expires_at > now)
            })
        {
            self.record_gateway_audit_event(
                GatewayAuditKind::Auth,
                duplicate_delivery_audit.clone(),
            )
            .await?;
            return Ok(GatewayRefreshRotationOutcome::Redelivered(Box::new(
                GatewayRefreshRedelivery {
                    family: family.clone(),
                    replacement: replacement.expect("replacement was checked above"),
                },
            )));
        }

        let family = self
            .revoke_gateway_refresh_family_for_replay(token, family, now)
            .await?;
        Ok(GatewayRefreshRotationOutcome::ReplayDetected(Box::new(
            family,
        )))
    }

    async fn revoke_gateway_refresh_family_for_replay(
        &self,
        token: &GatewayRefreshTokenRecord,
        family: &GatewayRefreshFamilyRecord,
        now: DateTime<Utc>,
    ) -> Result<GatewayRefreshFamilyRecord, StoreError> {
        let outbox = gateway_outbox(
            "gateway_refresh_family",
            &family.id,
            "gateway.refresh_family.revoked",
            BTreeMap::from([
                ("reason".into(), serde_json::json!("token_replay")),
                ("generation".into(), serde_json::json!(token.generation)),
            ]),
        );
        const MAX_ATTEMPTS: u32 = 8;
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .db
                .query("BEGIN TRANSACTION; LET $revoked = (UPDATE ONLY $family SET revoked_at = $now, revocation_reason = 'token_replay' WHERE revoked_at = NONE RETURN AFTER); UPDATE ONLY $refresh_record SET replay_detected_at = $now WHERE replay_detected_at = NONE RETURN NONE; IF $revoked != NONE { CREATE outbox_event CONTENT $outbox RETURN NONE; }; COMMIT TRANSACTION;")
                .bind(("family", family.id.clone()))
                .bind(("refresh_record", token.id.clone()))
                .bind(("now", now))
                .bind(("outbox", outbox.clone()))
                .await
                .and_then(|mut response| match primary_transaction_error(response.take_errors()) {
                    Some(error) => Err(error),
                    None => Ok(()),
                });
            match response {
                Ok(_) => {
                    let mut revoked = family.clone();
                    revoked.revoked_at.get_or_insert(now);
                    revoked
                        .revocation_reason
                        .get_or_insert_with(|| "token_replay".to_owned());
                    return Ok(revoked);
                }
                Err(error) if is_retryable_transaction_failure(&error) => {
                    let observed_family: Option<GatewayRefreshFamilyRecord> =
                        self.db.select(family.id.clone()).await?;
                    let observed_token: Option<GatewayRefreshTokenRecord> =
                        self.db.select(token.id.clone()).await?;
                    if let (Some(observed_family), Some(observed_token)) =
                        (observed_family, observed_token)
                        && observed_family.revoked_at.is_some()
                        && observed_token.replay_detected_at.is_some()
                    {
                        return Ok(observed_family);
                    }
                    if attempt + 1 < MAX_ATTEMPTS {
                        retry_backoff(attempt).await;
                        continue;
                    }
                    return Err(error.into());
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("gateway refresh replay revocation attempts return or fail")
    }
}

fn deterministic_gateway_record_id(table: &str, kind: &str, values: &[&str]) -> RecordId {
    let mut identity = String::from(kind);
    for value in values {
        identity.push('\0');
        identity.push_str(value);
    }
    let id = Uuid::new_v5(&PLATFORM_ID_NAMESPACE, identity.as_bytes());
    RecordId::new(table, surrealdb::types::Uuid::from(id))
}

fn gateway_outbox(
    aggregate_type: &str,
    record: &RecordId,
    event_type: &str,
    mut payload: BTreeMap<String, serde_json::Value>,
) -> OutboxDraft {
    let record_id = gateway_record_identity(record);
    payload.insert("record_id".into(), serde_json::json!(&record_id));
    OutboxDraft::now(
        None,
        aggregate_type,
        record_id,
        event_type,
        1,
        OpenObject::new(payload),
    )
}

fn gateway_audit_outbox(kind: GatewayAuditKind, record: &AuditEventRecord) -> OutboxDraft {
    gateway_outbox(
        "gateway_audit",
        &record.id,
        "gateway.audit.recorded",
        BTreeMap::from([
            ("kind".into(), serde_json::json!(kind.resource_type())),
            ("action".into(), serde_json::json!(&record.action)),
            ("outcome".into(), serde_json::json!(record.outcome)),
        ]),
    )
}

fn gateway_record_identity(record: &RecordId) -> String {
    let RecordIdKey::Uuid(id) = &record.key else {
        unreachable!("gateway runtime records always use UUID keys")
    };
    format!("{}:{id}", record.table.as_str())
}

async fn delete_count(
    store: &PlatformStore,
    statement: &str,
    now: DateTime<Utc>,
) -> Result<u64, StoreError> {
    let mut response = store
        .db
        .query(statement)
        .bind(("now", now))
        .await?
        .check()?;
    let deleted: Vec<GatewayJwtRevocationRecord> = response.take(0)?;
    u64::try_from(deleted.len()).map_err(|_| StoreError::MissingRecord {
        operation: "gateway retention count conversion",
    })
}

fn is_retryable_transaction_conflict(error: &surrealdb::Error) -> bool {
    matches!(
        error.query_details(),
        Some(surrealdb::types::QueryError::TransactionConflict)
    ) || error.message().starts_with("Transaction conflict:")
}

fn is_retryable_transaction_failure(error: &surrealdb::Error) -> bool {
    is_retryable_transaction_conflict(error)
        || error
            .message()
            .contains("not executed due to a failed transaction")
}

fn is_refresh_replay_error(error: &surrealdb::Error) -> bool {
    error.is_thrown() && error.message().contains("gateway_refresh_token_replay")
}

fn is_refresh_family_invalid_error(error: &surrealdb::Error) -> bool {
    error.is_thrown() && error.message().contains("gateway_refresh_family_invalid")
}

fn validate_refresh_pair(
    family: &GatewayRefreshFamilyRecord,
    token: &GatewayRefreshTokenRecord,
    expected_generation: i64,
) -> Result<(), StoreError> {
    if token.family != family.id {
        return Err(StoreError::InvalidGatewayRefreshTransition {
            reason: "token and family identities differ",
        });
    }
    if token.generation != expected_generation {
        return Err(StoreError::InvalidGatewayRefreshTransition {
            reason: "token generation is not the expected successor",
        });
    }
    if token.expires_at != family.expires_at || token.issued_at >= token.expires_at {
        return Err(StoreError::InvalidGatewayRefreshTransition {
            reason: "token expiry is outside its family lifetime",
        });
    }
    Ok(())
}

async fn retry_backoff(attempt: u32) {
    tokio::time::sleep(Duration::from_millis(1_u64 << attempt)).await;
}
