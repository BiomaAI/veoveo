use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use duckdb::{OptionalExt, params};
use serde::Serialize;
use veoveo_mcp_contract::{
    AuditEvent, AuthAuditEvent, AuthorizationServerId, GatewayAuthorizationCodeRecord,
    GatewayAuthorizationRequest, GatewayControlPlaneRevision, GatewayControlPlaneRevisionSource,
    GatewayJwtRevocation, GatewayProfileId, GatewayResourceSubscription, GatewayTaskId,
    GatewayTaskMapping, JwtId, McpMethodName, OAuthAuthorizationCode, OAuthClientId,
    OAuthStateValue, PolicyDecision, PolicyEffect, PolicyReasonCode, PrincipalId, ResourceUri,
    ServerSlug, SharedDuckDbConnection, TokenIssuer, UpstreamTaskId, open_duckdb,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GatewayAuditCounts {
    pub auth_events: u64,
    pub policy_events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayPolicyAuditMethodSummary {
    pub method: McpMethodName,
    pub allow_events: u64,
    pub deny_events: u64,
    pub total_events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayPolicyAuditReasonSummary {
    pub reason: PolicyReasonCode,
    pub events: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct GatewayAuditRetentionSummary {
    pub auth_events_deleted: u64,
    pub policy_events_deleted: u64,
}

#[derive(Debug, Clone)]
pub struct GatewayState {
    conn: SharedDuckDbConnection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GatewayAuditTable {
    Auth,
    Policy,
}

impl GatewayAuditTable {
    fn count_sql(self) -> &'static str {
        match self {
            Self::Auth => "SELECT COUNT(*) FROM gateway_auth_audit_events",
            Self::Policy => "SELECT COUNT(*) FROM gateway_audit_events",
        }
    }
}

impl GatewayState {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = open_duckdb(path)?;
        let state = Self { conn };
        state.initialize()?;
        Ok(state)
    }

    fn initialize(&self) -> Result<()> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS gateway_task_mappings (
                gateway_task_id TEXT PRIMARY KEY,
                upstream_server TEXT NOT NULL,
                upstream_task_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                owner TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL,
                mapping_json TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_gateway_task_mappings_upstream
            ON gateway_task_mappings(upstream_server, upstream_task_id);

            CREATE INDEX IF NOT EXISTS idx_gateway_task_mappings_owner
            ON gateway_task_mappings(profile, owner, upstream_server);

            CREATE TABLE IF NOT EXISTS gateway_resource_subscriptions (
                profile TEXT NOT NULL,
                owner TEXT NOT NULL,
                upstream_server TEXT NOT NULL,
                resource_uri TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL,
                subscription_json TEXT NOT NULL,
                PRIMARY KEY(profile, owner, upstream_server, resource_uri)
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_resource_subscriptions_owner
            ON gateway_resource_subscriptions(profile, owner, upstream_server);

            CREATE TABLE IF NOT EXISTS gateway_revoked_jwt_ids (
                profile TEXT NOT NULL,
                issuer TEXT NOT NULL,
                jwt_id TEXT NOT NULL,
                revoked_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                revocation_json TEXT NOT NULL,
                PRIMARY KEY(profile, issuer, jwt_id)
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_revoked_jwt_ids_expires
            ON gateway_revoked_jwt_ids(expires_at);

            CREATE TABLE IF NOT EXISTS gateway_client_assertion_jtis (
                authorization_server TEXT NOT NULL,
                client_id TEXT NOT NULL,
                jwt_id TEXT NOT NULL,
                seen_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                PRIMARY KEY(authorization_server, client_id, jwt_id)
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_client_assertion_jtis_expires
            ON gateway_client_assertion_jtis(expires_at);

            CREATE TABLE IF NOT EXISTS gateway_id_jag_jtis (
                authorization_server TEXT NOT NULL,
                client_id TEXT NOT NULL,
                jwt_id TEXT NOT NULL,
                seen_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                PRIMARY KEY(authorization_server, client_id, jwt_id)
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_id_jag_jtis_expires
            ON gateway_id_jag_jtis(expires_at);

            CREATE TABLE IF NOT EXISTS gateway_authorization_requests (
                idp_state TEXT PRIMARY KEY,
                profile TEXT NOT NULL,
                oauth_client_id TEXT NOT NULL,
                oidc_client TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                request_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_authorization_requests_expires
            ON gateway_authorization_requests(expires_at);

            CREATE INDEX IF NOT EXISTS idx_gateway_authorization_requests_client
            ON gateway_authorization_requests(profile, oauth_client_id);

            CREATE TABLE IF NOT EXISTS gateway_authorization_codes (
                code TEXT PRIMARY KEY,
                profile TEXT NOT NULL,
                oauth_client_id TEXT NOT NULL,
                oidc_client TEXT NOT NULL,
                principal TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                issued_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                consumed_at TIMESTAMP,
                code_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_authorization_codes_expires
            ON gateway_authorization_codes(expires_at);

            CREATE INDEX IF NOT EXISTS idx_gateway_authorization_codes_client
            ON gateway_authorization_codes(profile, oauth_client_id, principal);

            CREATE TABLE IF NOT EXISTS gateway_control_plane_revisions (
                revision_id TEXT PRIMARY KEY,
                sha256 TEXT NOT NULL,
                source TEXT NOT NULL,
                applied_at TIMESTAMP NOT NULL,
                applied_by TEXT NOT NULL,
                tenant TEXT,
                revision_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_control_plane_revisions_applied
            ON gateway_control_plane_revisions(applied_at);

            CREATE INDEX IF NOT EXISTS idx_gateway_control_plane_revisions_sha256
            ON gateway_control_plane_revisions(sha256);

            CREATE TABLE IF NOT EXISTS gateway_audit_events (
                event_id TEXT PRIMARY KEY,
                trace_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                method TEXT NOT NULL,
                action TEXT NOT NULL,
                principal TEXT,
                tenant TEXT,
                token_issuer TEXT,
                timestamp TIMESTAMP NOT NULL,
                latency_ms UBIGINT,
                target_json TEXT NOT NULL,
                decision_json TEXT NOT NULL,
                event_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_audit_profile_time
            ON gateway_audit_events(profile, timestamp);

            CREATE INDEX IF NOT EXISTS idx_gateway_audit_principal_time
            ON gateway_audit_events(principal, timestamp);

            CREATE TABLE IF NOT EXISTS gateway_auth_audit_events (
                event_id TEXT PRIMARY KEY,
                trace_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                protected_resource TEXT NOT NULL,
                outcome TEXT NOT NULL,
                reason TEXT NOT NULL,
                method TEXT NOT NULL,
                principal TEXT,
                tenant TEXT,
                token_issuer TEXT,
                token_subject TEXT,
                jwt_id TEXT,
                timestamp TIMESTAMP NOT NULL,
                latency_ms UBIGINT,
                event_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_auth_audit_profile_time
            ON gateway_auth_audit_events(profile, timestamp);

            CREATE INDEX IF NOT EXISTS idx_gateway_auth_audit_principal_time
            ON gateway_auth_audit_events(principal, timestamp);
            "#,
        )?;
        Ok(())
    }

    pub fn audit_counts(&self) -> Result<GatewayAuditCounts> {
        Ok(GatewayAuditCounts {
            auth_events: self.count_rows(GatewayAuditTable::Auth)?,
            policy_events: self.count_rows(GatewayAuditTable::Policy)?,
        })
    }

    pub fn policy_audit_method_summary(&self) -> Result<Vec<GatewayPolicyAuditMethodSummary>> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let mut stmt = conn.prepare(
            r#"
            SELECT method, decision_json
            FROM gateway_audit_events
            ORDER BY method, timestamp, event_id
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut summaries =
            std::collections::BTreeMap::<McpMethodName, GatewayPolicyAuditMethodSummary>::new();
        for row in rows {
            let (method, decision_json) = row?;
            let method = McpMethodName::new(method)?;
            let decision: PolicyDecision = serde_json::from_str(&decision_json)?;
            let entry = summaries.entry(method.clone()).or_insert_with(|| {
                GatewayPolicyAuditMethodSummary {
                    method,
                    allow_events: 0,
                    deny_events: 0,
                    total_events: 0,
                }
            });
            match decision.effect {
                PolicyEffect::Allow => entry.allow_events += 1,
                PolicyEffect::Deny => entry.deny_events += 1,
            }
            entry.total_events += 1;
        }
        Ok(summaries.into_values().collect())
    }

    pub fn policy_audit_reason_summary(&self) -> Result<Vec<GatewayPolicyAuditReasonSummary>> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let mut stmt = conn.prepare(
            r#"
            SELECT decision_json
            FROM gateway_audit_events
            ORDER BY timestamp, event_id
            "#,
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut summaries =
            std::collections::BTreeMap::<PolicyReasonCode, GatewayPolicyAuditReasonSummary>::new();
        for row in rows {
            let decision: PolicyDecision = serde_json::from_str(&row?)?;
            let entry =
                summaries
                    .entry(decision.reason)
                    .or_insert(GatewayPolicyAuditReasonSummary {
                        reason: decision.reason,
                        events: 0,
                    });
            entry.events += 1;
        }
        Ok(summaries.into_values().collect())
    }

    pub fn delete_audit_events_before(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<GatewayAuditRetentionSummary> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let policy_events_deleted = conn.execute(
            "DELETE FROM gateway_audit_events WHERE timestamp < ?1",
            params![cutoff],
        )?;
        let auth_events_deleted = conn.execute(
            "DELETE FROM gateway_auth_audit_events WHERE timestamp < ?1",
            params![cutoff],
        )?;
        Ok(GatewayAuditRetentionSummary {
            auth_events_deleted: u64::try_from(auth_events_deleted)?,
            policy_events_deleted: u64::try_from(policy_events_deleted)?,
        })
    }

    pub fn record_control_plane_revision(
        &self,
        revision: &GatewayControlPlaneRevision,
    ) -> Result<()> {
        let revision_json = serde_json::to_string(revision)?;
        let source = match revision.source {
            GatewayControlPlaneRevisionSource::AdminApi => "admin_api",
        };
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO gateway_control_plane_revisions (
                revision_id, sha256, source, applied_at, applied_by, tenant, revision_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                revision.revision_id.as_str(),
                revision.sha256.as_str(),
                source,
                revision.applied_at,
                revision.applied_by.as_str(),
                revision.tenant.as_ref().map(|value| value.as_str()),
                revision_json,
            ],
        )?;
        Ok(())
    }

    pub fn latest_control_plane_revision(&self) -> Result<Option<GatewayControlPlaneRevision>> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let revision_json: Option<String> = conn
            .query_row(
                r#"
                SELECT revision_json
                FROM gateway_control_plane_revisions
                ORDER BY applied_at DESC, revision_id DESC
                LIMIT 1
                "#,
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(revision_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?)
    }

    pub fn control_plane_revision_count(&self) -> Result<u64> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM gateway_control_plane_revisions",
            [],
            |row| row.get(0),
        )?;
        Ok(u64::try_from(count)?)
    }

    pub fn record_jwt_revocation(&self, revocation: &GatewayJwtRevocation) -> Result<()> {
        let revocation_json = serde_json::to_string(revocation)?;
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO gateway_revoked_jwt_ids (
                profile, issuer, jwt_id, revoked_at, expires_at, revocation_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(profile, issuer, jwt_id) DO UPDATE SET
                revoked_at = excluded.revoked_at,
                expires_at = excluded.expires_at,
                revocation_json = excluded.revocation_json
            "#,
            params![
                revocation.profile.as_str(),
                revocation.issuer.as_str(),
                revocation.jwt_id.as_str(),
                revocation.revoked_at,
                revocation.expires_at,
                revocation_json,
            ],
        )?;
        Ok(())
    }

    pub fn jwt_revocation(
        &self,
        profile: &GatewayProfileId,
        issuer: &TokenIssuer,
        jwt_id: &JwtId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayJwtRevocation>> {
        self.query_revocation(
            r#"
            SELECT revocation_json
            FROM gateway_revoked_jwt_ids
            WHERE profile = ?1 AND issuer = ?2 AND jwt_id = ?3 AND expires_at > ?4
            "#,
            params![profile.as_str(), issuer.as_str(), jwt_id.as_str(), now],
        )
    }

    pub fn prune_expired_jwt_revocations(&self, now: DateTime<Utc>) -> Result<u64> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let deleted = conn.execute(
            "DELETE FROM gateway_revoked_jwt_ids WHERE expires_at <= ?1",
            params![now],
        )?;
        Ok(u64::try_from(deleted)?)
    }

    pub fn record_client_assertion_jti(
        &self,
        authorization_server: &AuthorizationServerId,
        client_id: &OAuthClientId,
        jwt_id: &JwtId,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            "DELETE FROM gateway_client_assertion_jtis WHERE expires_at <= ?1",
            params![now],
        )?;
        let existing: Option<DateTime<Utc>> = conn
            .query_row(
                r#"
                SELECT expires_at
                FROM gateway_client_assertion_jtis
                WHERE authorization_server = ?1 AND client_id = ?2 AND jwt_id = ?3
                "#,
                params![
                    authorization_server.as_str(),
                    client_id.as_str(),
                    jwt_id.as_str()
                ],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_some_and(|existing_expires_at| existing_expires_at > now) {
            return Ok(false);
        }
        conn.execute(
            r#"
            INSERT INTO gateway_client_assertion_jtis (
                authorization_server, client_id, jwt_id, seen_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(authorization_server, client_id, jwt_id) DO UPDATE SET
                seen_at = excluded.seen_at,
                expires_at = excluded.expires_at
            "#,
            params![
                authorization_server.as_str(),
                client_id.as_str(),
                jwt_id.as_str(),
                now,
                expires_at,
            ],
        )?;
        Ok(true)
    }

    pub fn record_id_jag_jti(
        &self,
        authorization_server: &AuthorizationServerId,
        client_id: &OAuthClientId,
        jwt_id: &JwtId,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            "DELETE FROM gateway_id_jag_jtis WHERE expires_at <= ?1",
            params![now],
        )?;
        let existing: Option<DateTime<Utc>> = conn
            .query_row(
                r#"
                SELECT expires_at
                FROM gateway_id_jag_jtis
                WHERE authorization_server = ?1 AND client_id = ?2 AND jwt_id = ?3
                "#,
                params![
                    authorization_server.as_str(),
                    client_id.as_str(),
                    jwt_id.as_str()
                ],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_some_and(|existing_expires_at| existing_expires_at > now) {
            return Ok(false);
        }
        conn.execute(
            r#"
            INSERT INTO gateway_id_jag_jtis (
                authorization_server, client_id, jwt_id, seen_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(authorization_server, client_id, jwt_id) DO UPDATE SET
                seen_at = excluded.seen_at,
                expires_at = excluded.expires_at
            "#,
            params![
                authorization_server.as_str(),
                client_id.as_str(),
                jwt_id.as_str(),
                now,
                expires_at,
            ],
        )?;
        Ok(true)
    }

    pub fn record_authorization_request(
        &self,
        request: &GatewayAuthorizationRequest,
    ) -> Result<()> {
        let request_json = serde_json::to_string(request)?;
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO gateway_authorization_requests (
                idp_state, profile, oauth_client_id, oidc_client, redirect_uri,
                created_at, expires_at, request_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                request.idp_state.as_str(),
                request.profile.as_str(),
                request.oauth_client_id.as_str(),
                request.oidc_client.as_str(),
                request.redirect_uri.as_str(),
                request.created_at,
                request.expires_at,
                request_json,
            ],
        )?;
        Ok(())
    }

    pub fn consume_authorization_request(
        &self,
        idp_state: &OAuthStateValue,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationRequest>> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            "DELETE FROM gateway_authorization_requests WHERE expires_at <= ?1",
            params![now],
        )?;
        let request_json = conn
            .query_row(
                r#"
                SELECT request_json
                FROM gateway_authorization_requests
                WHERE idp_state = ?1 AND expires_at > ?2
                "#,
                params![idp_state.as_str(), now],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(request_json) = request_json else {
            return Ok(None);
        };
        conn.execute(
            "DELETE FROM gateway_authorization_requests WHERE idp_state = ?1",
            params![idp_state.as_str()],
        )?;
        Ok(Some(serde_json::from_str(&request_json)?))
    }

    pub fn record_authorization_code(&self, code: &GatewayAuthorizationCodeRecord) -> Result<()> {
        let code_json = serde_json::to_string(code)?;
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO gateway_authorization_codes (
                code, profile, oauth_client_id, oidc_client, principal, redirect_uri,
                issued_at, expires_at, consumed_at, code_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                code.code.as_str(),
                code.profile.as_str(),
                code.oauth_client_id.as_str(),
                code.oidc_client.as_str(),
                code.principal.id.as_str(),
                code.redirect_uri.as_str(),
                code.issued_at,
                code.expires_at,
                code.consumed_at,
                code_json,
            ],
        )?;
        Ok(())
    }

    pub fn consume_authorization_code(
        &self,
        code: &OAuthAuthorizationCode,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationCodeRecord>> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            "DELETE FROM gateway_authorization_codes WHERE expires_at <= ?1",
            params![now],
        )?;
        let code_json = conn
            .query_row(
                r#"
                SELECT code_json
                FROM gateway_authorization_codes
                WHERE code = ?1 AND expires_at > ?2 AND consumed_at IS NULL
                "#,
                params![code.as_str(), now],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(code_json) = code_json else {
            return Ok(None);
        };
        let mut record: GatewayAuthorizationCodeRecord = serde_json::from_str(&code_json)?;
        record.consumed_at = Some(now);
        let updated_json = serde_json::to_string(&record)?;
        let updated = conn.execute(
            r#"
            UPDATE gateway_authorization_codes
            SET consumed_at = ?2, code_json = ?3
            WHERE code = ?1 AND expires_at > ?2 AND consumed_at IS NULL
            "#,
            params![code.as_str(), now, updated_json],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        Ok(Some(record))
    }

    pub fn prune_expired_authorization_records(&self, now: DateTime<Utc>) -> Result<u64> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let requests = conn.execute(
            "DELETE FROM gateway_authorization_requests WHERE expires_at <= ?1",
            params![now],
        )?;
        let codes = conn.execute(
            "DELETE FROM gateway_authorization_codes WHERE expires_at <= ?1",
            params![now],
        )?;
        Ok(u64::try_from(requests + codes)?)
    }

    pub fn record_task_mapping(&self, mapping: &GatewayTaskMapping) -> Result<()> {
        let mapping_json = serde_json::to_string(mapping)?;
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO gateway_task_mappings (
                gateway_task_id, upstream_server, upstream_task_id, profile, owner,
                created_at, updated_at, mapping_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(gateway_task_id) DO UPDATE SET
                upstream_server = excluded.upstream_server,
                upstream_task_id = excluded.upstream_task_id,
                profile = excluded.profile,
                owner = excluded.owner,
                updated_at = excluded.updated_at,
                mapping_json = excluded.mapping_json
            "#,
            params![
                mapping.gateway_task_id.as_str(),
                mapping.upstream_server.as_str(),
                mapping.upstream_task_id.as_str(),
                mapping.profile.as_str(),
                mapping.owner.as_str(),
                mapping.created_at,
                mapping.updated_at,
                mapping_json,
            ],
        )?;
        Ok(())
    }

    pub fn task_mapping(
        &self,
        gateway_task_id: &GatewayTaskId,
    ) -> Result<Option<GatewayTaskMapping>> {
        self.query_mapping(
            "SELECT mapping_json FROM gateway_task_mappings WHERE gateway_task_id = ?1",
            params![gateway_task_id.as_str()],
        )
    }

    pub fn task_mapping_by_upstream(
        &self,
        upstream_server: &veoveo_mcp_contract::ServerSlug,
        upstream_task_id: &UpstreamTaskId,
    ) -> Result<Option<GatewayTaskMapping>> {
        self.query_mapping(
            "SELECT mapping_json FROM gateway_task_mappings WHERE upstream_server = ?1 AND upstream_task_id = ?2",
            params![upstream_server.as_str(), upstream_task_id.as_str()],
        )
    }

    pub fn task_mappings_for_owner(
        &self,
        profile: &GatewayProfileId,
        owner: &PrincipalId,
        upstream_server: &veoveo_mcp_contract::ServerSlug,
    ) -> Result<Vec<GatewayTaskMapping>> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let mut stmt = conn.prepare(
            r#"
            SELECT mapping_json
            FROM gateway_task_mappings
            WHERE profile = ?1 AND owner = ?2 AND upstream_server = ?3
            ORDER BY updated_at
            "#,
        )?;
        let rows = stmt.query_map(
            params![profile.as_str(), owner.as_str(), upstream_server.as_str()],
            |row| row.get::<_, String>(0),
        )?;
        let mut mappings = Vec::new();
        for row in rows {
            mappings.push(serde_json::from_str(&row?)?);
        }
        Ok(mappings)
    }

    pub fn record_resource_subscription(
        &self,
        subscription: &GatewayResourceSubscription,
    ) -> Result<()> {
        let subscription_json = serde_json::to_string(subscription)?;
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO gateway_resource_subscriptions (
                profile, owner, upstream_server, resource_uri,
                created_at, updated_at, subscription_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(profile, owner, upstream_server, resource_uri) DO UPDATE SET
                updated_at = excluded.updated_at,
                subscription_json = excluded.subscription_json
            "#,
            params![
                subscription.profile.as_str(),
                subscription.owner.as_str(),
                subscription.upstream_server.as_str(),
                subscription.resource_uri.as_str(),
                subscription.created_at,
                subscription.updated_at,
                subscription_json,
            ],
        )?;
        Ok(())
    }

    pub fn resource_subscription(
        &self,
        profile: &GatewayProfileId,
        owner: &PrincipalId,
        upstream_server: &ServerSlug,
        resource_uri: &ResourceUri,
    ) -> Result<Option<GatewayResourceSubscription>> {
        self.query_subscription(
            r#"
            SELECT subscription_json
            FROM gateway_resource_subscriptions
            WHERE profile = ?1 AND owner = ?2 AND upstream_server = ?3 AND resource_uri = ?4
            "#,
            params![
                profile.as_str(),
                owner.as_str(),
                upstream_server.as_str(),
                resource_uri.as_str()
            ],
        )
    }

    pub fn delete_resource_subscription(
        &self,
        profile: &GatewayProfileId,
        owner: &PrincipalId,
        upstream_server: &ServerSlug,
        resource_uri: &ResourceUri,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            r#"
            DELETE FROM gateway_resource_subscriptions
            WHERE profile = ?1 AND owner = ?2 AND upstream_server = ?3 AND resource_uri = ?4
            "#,
            params![
                profile.as_str(),
                owner.as_str(),
                upstream_server.as_str(),
                resource_uri.as_str()
            ],
        )?;
        Ok(())
    }

    pub fn record_audit_event(&self, event: &AuditEvent) -> Result<()> {
        let target_json = serde_json::to_string(&event.target)?;
        let decision_json = serde_json::to_string(&event.decision)?;
        let event_json = serde_json::to_string(event)?;
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO gateway_audit_events (
                event_id, trace_id, profile, method, action, principal, tenant, token_issuer,
                timestamp, latency_ms, target_json, decision_json, event_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(event_id) DO UPDATE SET
                trace_id = excluded.trace_id,
                profile = excluded.profile,
                method = excluded.method,
                action = excluded.action,
                principal = excluded.principal,
                tenant = excluded.tenant,
                token_issuer = excluded.token_issuer,
                timestamp = excluded.timestamp,
                latency_ms = excluded.latency_ms,
                target_json = excluded.target_json,
                decision_json = excluded.decision_json,
                event_json = excluded.event_json
            "#,
            params![
                event.event_id.as_str(),
                event.trace_id.as_str(),
                event.profile.as_str(),
                event.method.as_str(),
                format!("{:?}", event.action),
                event.principal.as_ref().map(|value| value.as_str()),
                event.tenant.as_ref().map(|value| value.as_str()),
                event.token_issuer.as_ref().map(|value| value.as_str()),
                event.timestamp,
                event.latency_ms,
                target_json,
                decision_json,
                event_json,
            ],
        )?;
        Ok(())
    }

    pub fn record_auth_audit_event(&self, event: &AuthAuditEvent) -> Result<()> {
        let event_json = serde_json::to_string(event)?;
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO gateway_auth_audit_events (
                event_id, trace_id, profile, protected_resource, outcome, reason, method,
                principal, tenant, token_issuer, token_subject, jwt_id, timestamp, latency_ms,
                event_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(event_id) DO UPDATE SET
                trace_id = excluded.trace_id,
                profile = excluded.profile,
                protected_resource = excluded.protected_resource,
                outcome = excluded.outcome,
                reason = excluded.reason,
                method = excluded.method,
                principal = excluded.principal,
                tenant = excluded.tenant,
                token_issuer = excluded.token_issuer,
                token_subject = excluded.token_subject,
                jwt_id = excluded.jwt_id,
                timestamp = excluded.timestamp,
                latency_ms = excluded.latency_ms,
                event_json = excluded.event_json
            "#,
            params![
                event.event_id.as_str(),
                event.trace_id.as_str(),
                event.profile.as_str(),
                event.protected_resource.as_str(),
                event.outcome.as_str(),
                event.reason.as_str(),
                event.method.as_str(),
                event.principal.as_ref().map(|value| value.as_str()),
                event.tenant.as_ref().map(|value| value.as_str()),
                event.token_issuer.as_ref().map(|value| value.as_str()),
                event.token_subject.as_ref().map(|value| value.as_str()),
                event.jwt_id.as_ref().map(|value| value.as_str()),
                event.timestamp,
                event.latency_ms,
                event_json,
            ],
        )?;
        Ok(())
    }

    #[cfg(test)]
    fn audit_event_count(&self) -> Result<u64> {
        self.count_rows(GatewayAuditTable::Policy)
    }

    #[cfg(test)]
    fn auth_audit_event_count(&self) -> Result<u64> {
        self.count_rows(GatewayAuditTable::Auth)
    }

    fn count_rows(&self, table: GatewayAuditTable) -> Result<u64> {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let count: i64 = conn.query_row(table.count_sql(), [], |row| row.get(0))?;
        Ok(u64::try_from(count)?)
    }

    fn query_mapping<P>(&self, sql: &str, params: P) -> Result<Option<GatewayTaskMapping>>
    where
        P: duckdb::Params,
    {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let mapping_json = conn
            .query_row(sql, params, |row| row.get::<_, String>(0))
            .optional()?;
        Ok(mapping_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?)
    }

    fn query_subscription<P>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Option<GatewayResourceSubscription>>
    where
        P: duckdb::Params,
    {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let subscription_json = conn
            .query_row(sql, params, |row| row.get::<_, String>(0))
            .optional()?;
        Ok(subscription_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?)
    }

    fn query_revocation<P>(&self, sql: &str, params: P) -> Result<Option<GatewayJwtRevocation>>
    where
        P: duckdb::Params,
    {
        let conn = self.conn.lock().expect("gateway state mutex poisoned");
        let revocation_json = conn
            .query_row(sql, params, |row| row.get::<_, String>(0))
            .optional()?;
        Ok(revocation_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use chrono::{TimeDelta, Utc};
    use std::collections::{BTreeMap, BTreeSet};

    use veoveo_mcp_contract::{
        AuthMethod, AuthOutcome, AuthReasonCode, GatewayAction, GatewayControlPlane,
        GatewayControlPlaneRevision, GatewayControlPlaneRevisionId,
        GatewayControlPlaneRevisionSource, McpMethodName, OAuthRedirectUri,
        OidcClientRegistrationId, OidcNonce, PkceCodeChallenge, PkceCodeChallengeMethod,
        PkceCodeVerifier, PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyTarget, Principal,
        PrincipalKind, ProtectedResourceId, ScopeName, ServerSlug, TokenSubject, TraceId,
        UpstreamTaskId,
    };

    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("veoveo-gateway-{name}-{unique}.duckdb"))
    }

    fn record_policy_audit(
        state: &GatewayState,
        event_id: &str,
        method: &str,
        action: GatewayAction,
        effect: PolicyEffect,
        reason: PolicyReasonCode,
    ) {
        let profile = GatewayProfileId::new("default").unwrap();
        let target = PolicyTarget::Tool {
            server: ServerSlug::new("media").unwrap(),
            tool: veoveo_mcp_contract::LocalToolName::new("run").unwrap(),
        };
        let trace_id = TraceId::new(format!("trace-{event_id}")).unwrap();
        let principal = PrincipalId::new("issuer#subject").unwrap();
        let decision = PolicyDecision {
            effect,
            reason,
            evaluated_at: Utc::now(),
            profile: profile.clone(),
            action,
            target: target.clone(),
            principal: Some(principal.clone()),
            tenant: None,
            policy_version: None,
            rule_id: None,
            trace_id: trace_id.clone(),
        };
        state
            .record_audit_event(&AuditEvent {
                event_id: TraceId::new(event_id).unwrap(),
                timestamp: Utc::now(),
                trace_id,
                profile,
                method: McpMethodName::new(method).unwrap(),
                action,
                target,
                decision,
                principal: Some(principal),
                tenant: None,
                token_issuer: None,
                latency_ms: Some(12),
                metadata: BTreeMap::new(),
            })
            .unwrap();
    }

    fn empty_control_plane() -> GatewayControlPlane {
        GatewayControlPlane {
            identity_providers: Vec::new(),
            authorization_servers: Vec::new(),
            servers: Vec::new(),
            profiles: Vec::new(),
            policies: Vec::new(),
            oauth_clients: Vec::new(),
            oidc_clients: Vec::new(),
            secrets: Vec::new(),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn control_plane_revision_round_trips_latest_across_restart() {
        let path = temp_path("control-plane-revision");
        let revision = GatewayControlPlaneRevision {
            revision_id: GatewayControlPlaneRevisionId::new("revision-1").unwrap(),
            sha256: "abc123".to_string(),
            source: GatewayControlPlaneRevisionSource::AdminApi,
            applied_at: Utc::now(),
            applied_by: PrincipalId::new("issuer#admin").unwrap(),
            tenant: None,
            control_plane: empty_control_plane(),
        };

        let state = GatewayState::open(&path).unwrap();
        state.record_control_plane_revision(&revision).unwrap();
        assert_eq!(state.control_plane_revision_count().unwrap(), 1);
        let state = GatewayState::open(&path).unwrap();

        assert_eq!(
            state.latest_control_plane_revision().unwrap(),
            Some(revision)
        );
        assert_eq!(state.control_plane_revision_count().unwrap(), 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn task_mapping_round_trips_by_gateway_and_upstream_ids() {
        let path = temp_path("tasks");
        let state = GatewayState::open(&path).unwrap();
        let mapping = GatewayTaskMapping {
            gateway_task_id: GatewayTaskId::new("gateway-task-1").unwrap(),
            upstream_server: ServerSlug::new("media").unwrap(),
            upstream_task_id: UpstreamTaskId::new("upstream-task-1").unwrap(),
            profile: GatewayProfileId::new("default").unwrap(),
            owner: PrincipalId::new("issuer#subject").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        state.record_task_mapping(&mapping).unwrap();

        assert_eq!(
            state.task_mapping(&mapping.gateway_task_id).unwrap(),
            Some(mapping.clone())
        );
        assert_eq!(
            state
                .task_mapping_by_upstream(&mapping.upstream_server, &mapping.upstream_task_id)
                .unwrap(),
            Some(mapping.clone())
        );
        assert_eq!(
            state
                .task_mappings_for_owner(&mapping.profile, &mapping.owner, &mapping.upstream_server)
                .unwrap(),
            vec![mapping]
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn resource_subscription_round_trips_and_deletes_by_owner() {
        let path = temp_path("subscriptions");
        let state = GatewayState::open(&path).unwrap();
        let subscription = GatewayResourceSubscription {
            profile: GatewayProfileId::new("default").unwrap(),
            owner: PrincipalId::new("issuer#subject").unwrap(),
            upstream_server: ServerSlug::new("media").unwrap(),
            resource_uri: ResourceUri::new("media://artifact/abc").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        state.record_resource_subscription(&subscription).unwrap();

        assert_eq!(
            state
                .resource_subscription(
                    &subscription.profile,
                    &subscription.owner,
                    &subscription.upstream_server,
                    &subscription.resource_uri,
                )
                .unwrap(),
            Some(subscription.clone())
        );

        state
            .delete_resource_subscription(
                &subscription.profile,
                &subscription.owner,
                &subscription.upstream_server,
                &subscription.resource_uri,
            )
            .unwrap();

        assert_eq!(
            state
                .resource_subscription(
                    &subscription.profile,
                    &subscription.owner,
                    &subscription.upstream_server,
                    &subscription.resource_uri,
                )
                .unwrap(),
            None
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn jwt_revocation_round_trips_and_prunes_expired_entries() {
        let path = temp_path("revoked-jwts");
        let state = GatewayState::open(&path).unwrap();
        let now = Utc::now();
        let profile = GatewayProfileId::new("default").unwrap();
        let issuer = TokenIssuer::new("https://idp.example.com").unwrap();
        let jwt_id = JwtId::new("jwt-1").unwrap();
        let revocation = GatewayJwtRevocation {
            profile: profile.clone(),
            issuer: issuer.clone(),
            jwt_id: jwt_id.clone(),
            revoked_at: now,
            expires_at: now + TimeDelta::hours(1),
            reason: Some("operator_request".to_string()),
        };

        state.record_jwt_revocation(&revocation).unwrap();

        assert_eq!(
            state
                .jwt_revocation(&profile, &issuer, &jwt_id, now)
                .unwrap(),
            Some(revocation)
        );
        assert_eq!(
            state
                .jwt_revocation(&profile, &issuer, &JwtId::new("jwt-2").unwrap(), now)
                .unwrap(),
            None
        );
        assert_eq!(
            state
                .prune_expired_jwt_revocations(now + TimeDelta::hours(2))
                .unwrap(),
            1
        );
        assert_eq!(
            state
                .jwt_revocation(&profile, &issuer, &jwt_id, now)
                .unwrap(),
            None
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn client_assertion_jti_replay_is_rejected_until_expiration() {
        let path = temp_path("client-assertion-jti");
        let state = GatewayState::open(&path).unwrap();
        let authorization_server = AuthorizationServerId::new("veoveo").unwrap();
        let client_id = OAuthClientId::new("veoveo-headless").unwrap();
        let jwt_id = JwtId::new("assertion-1").unwrap();
        let now = Utc::now();
        let expires_at = now + TimeDelta::minutes(5);

        assert!(
            state
                .record_client_assertion_jti(
                    &authorization_server,
                    &client_id,
                    &jwt_id,
                    expires_at,
                    now,
                )
                .unwrap()
        );
        assert!(
            !state
                .record_client_assertion_jti(
                    &authorization_server,
                    &client_id,
                    &jwt_id,
                    expires_at,
                    now + TimeDelta::seconds(1),
                )
                .unwrap()
        );
        assert!(
            state
                .record_client_assertion_jti(
                    &authorization_server,
                    &client_id,
                    &jwt_id,
                    now + TimeDelta::minutes(10),
                    expires_at + TimeDelta::seconds(1),
                )
                .unwrap()
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn id_jag_jti_replay_is_rejected_until_expiration() {
        let path = temp_path("id-jag-jti");
        let state = GatewayState::open(&path).unwrap();
        let authorization_server = AuthorizationServerId::new("veoveo").unwrap();
        let client_id = OAuthClientId::new("veoveo-browser").unwrap();
        let jwt_id = JwtId::new("id-jag-1").unwrap();
        let now = Utc::now();
        let expires_at = now + TimeDelta::minutes(5);

        assert!(
            state
                .record_id_jag_jti(&authorization_server, &client_id, &jwt_id, expires_at, now)
                .unwrap()
        );
        assert!(
            !state
                .record_id_jag_jti(
                    &authorization_server,
                    &client_id,
                    &jwt_id,
                    expires_at,
                    now + TimeDelta::seconds(1),
                )
                .unwrap()
        );
        assert!(
            state
                .record_id_jag_jti(
                    &authorization_server,
                    &client_id,
                    &jwt_id,
                    now + TimeDelta::minutes(10),
                    expires_at + TimeDelta::seconds(1),
                )
                .unwrap()
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn authorization_request_consumes_once_across_restart() {
        let path = temp_path("authorization-request");
        let now = Utc::now();
        let request = GatewayAuthorizationRequest {
            idp_state: OAuthStateValue::new("state-1").unwrap(),
            profile: GatewayProfileId::new("default").unwrap(),
            oauth_client_id: OAuthClientId::new("veoveo-browser").unwrap(),
            oidc_client: OidcClientRegistrationId::new("enterprise").unwrap(),
            redirect_uri: OAuthRedirectUri::new("https://veoveo.bioma.ai/oauth/callback").unwrap(),
            client_state: Some(OAuthStateValue::new("client-state-1").unwrap()),
            requested_scopes: BTreeSet::from([ScopeName::new("media:use").unwrap()]),
            code_challenge: PkceCodeChallenge::new("A".repeat(43)).unwrap(),
            code_challenge_method: PkceCodeChallengeMethod::S256,
            idp_code_verifier: PkceCodeVerifier::new("B".repeat(43)).unwrap(),
            idp_code_challenge: PkceCodeChallenge::new("C".repeat(43)).unwrap(),
            idp_code_challenge_method: PkceCodeChallengeMethod::S256,
            nonce: OidcNonce::new("nonce-1").unwrap(),
            created_at: now,
            expires_at: now + TimeDelta::minutes(5),
        };

        let state = GatewayState::open(&path).unwrap();
        state.record_authorization_request(&request).unwrap();
        assert!(state.record_authorization_request(&request).is_err());
        let state = GatewayState::open(&path).unwrap();

        assert_eq!(
            state
                .consume_authorization_request(&request.idp_state, now + TimeDelta::seconds(1))
                .unwrap(),
            Some(request.clone())
        );
        assert_eq!(
            state
                .consume_authorization_request(&request.idp_state, now + TimeDelta::seconds(2))
                .unwrap(),
            None
        );

        let expired = GatewayAuthorizationRequest {
            idp_state: OAuthStateValue::new("state-expired").unwrap(),
            created_at: now - TimeDelta::minutes(10),
            expires_at: now - TimeDelta::minutes(5),
            ..request
        };
        state.record_authorization_request(&expired).unwrap();
        assert_eq!(
            state
                .consume_authorization_request(&expired.idp_state, now)
                .unwrap(),
            None
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn authorization_code_consumes_once_across_restart() {
        let path = temp_path("authorization-code");
        let now = Utc::now();
        let principal = Principal {
            id: PrincipalId::new("https://idp.example.com#00u123").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            subject: TokenSubject::new("00u123").unwrap(),
            tenant: None,
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::from([ScopeName::new("media:use").unwrap()]),
            data_labels: BTreeSet::new(),
            authenticated_at: Some(now),
        };
        let code = GatewayAuthorizationCodeRecord {
            code: OAuthAuthorizationCode::new("B".repeat(43)).unwrap(),
            profile: GatewayProfileId::new("default").unwrap(),
            oauth_client_id: OAuthClientId::new("veoveo-browser").unwrap(),
            oidc_client: OidcClientRegistrationId::new("enterprise").unwrap(),
            redirect_uri: OAuthRedirectUri::new("https://veoveo.bioma.ai/oauth/callback").unwrap(),
            client_state: Some(OAuthStateValue::new("client-state-1").unwrap()),
            scopes: BTreeSet::from([ScopeName::new("media:use").unwrap()]),
            code_challenge: PkceCodeChallenge::new("C".repeat(43)).unwrap(),
            code_challenge_method: PkceCodeChallengeMethod::S256,
            principal,
            issued_at: now,
            expires_at: now + TimeDelta::minutes(5),
            consumed_at: None,
        };

        let state = GatewayState::open(&path).unwrap();
        state.record_authorization_code(&code).unwrap();
        assert!(state.record_authorization_code(&code).is_err());
        let state = GatewayState::open(&path).unwrap();

        let consumed = state
            .consume_authorization_code(&code.code, now + TimeDelta::seconds(1))
            .unwrap()
            .expect("authorization code should consume once");
        assert_eq!(consumed.code, code.code);
        assert_eq!(consumed.consumed_at, Some(now + TimeDelta::seconds(1)));
        assert_eq!(
            state
                .consume_authorization_code(&code.code, now + TimeDelta::seconds(2))
                .unwrap(),
            None
        );

        let expired = GatewayAuthorizationCodeRecord {
            code: OAuthAuthorizationCode::new("D".repeat(43)).unwrap(),
            issued_at: now - TimeDelta::minutes(10),
            expires_at: now - TimeDelta::minutes(5),
            consumed_at: None,
            ..code
        };
        state.record_authorization_code(&expired).unwrap();
        assert_eq!(
            state
                .consume_authorization_code(&expired.code, now)
                .unwrap(),
            None
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn audit_event_records_structured_evidence() {
        let path = temp_path("audit");
        let state = GatewayState::open(&path).unwrap();
        let profile = GatewayProfileId::new("default").unwrap();
        let action = GatewayAction::ToolsList;
        let target = PolicyTarget::Tool {
            server: ServerSlug::new("media").unwrap(),
            tool: veoveo_mcp_contract::LocalToolName::new("run").unwrap(),
        };
        let trace_id = TraceId::new("trace-1").unwrap();
        let decision = PolicyDecision {
            effect: PolicyEffect::Allow,
            reason: PolicyReasonCode::PolicyAllow,
            evaluated_at: Utc::now(),
            profile: profile.clone(),
            action,
            target: target.clone(),
            principal: Some(PrincipalId::new("issuer#subject").unwrap()),
            tenant: None,
            policy_version: None,
            rule_id: None,
            trace_id: trace_id.clone(),
        };

        state
            .record_audit_event(&AuditEvent {
                event_id: TraceId::new("event-1").unwrap(),
                timestamp: Utc::now(),
                trace_id,
                profile,
                method: McpMethodName::new("tools/list").unwrap(),
                action,
                target,
                decision,
                principal: Some(PrincipalId::new("issuer#subject").unwrap()),
                tenant: None,
                token_issuer: None,
                latency_ms: Some(12),
                metadata: BTreeMap::new(),
            })
            .unwrap();

        assert_eq!(state.audit_event_count().unwrap(), 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn policy_audit_method_summary_counts_allow_and_deny_events() {
        let path = temp_path("audit-method-summary");
        let state = GatewayState::open(&path).unwrap();

        record_policy_audit(
            &state,
            "event-tools-allow",
            "tools/list",
            GatewayAction::ToolsList,
            PolicyEffect::Allow,
            PolicyReasonCode::PolicyAllow,
        );
        record_policy_audit(
            &state,
            "event-tools-deny",
            "tools/list",
            GatewayAction::ToolsList,
            PolicyEffect::Deny,
            PolicyReasonCode::PolicyDeny,
        );
        record_policy_audit(
            &state,
            "event-resource-allow",
            "resources/read",
            GatewayAction::ResourcesRead,
            PolicyEffect::Allow,
            PolicyReasonCode::PolicyAllow,
        );
        record_policy_audit(
            &state,
            "event-resource-label-deny",
            "resources/read",
            GatewayAction::ResourcesRead,
            PolicyEffect::Deny,
            PolicyReasonCode::MissingDataLabel,
        );

        let summary = state.policy_audit_method_summary().unwrap();
        let summary_by_method: BTreeMap<String, (u64, u64, u64)> = summary
            .into_iter()
            .map(|entry| {
                (
                    entry.method.as_str().to_string(),
                    (entry.allow_events, entry.deny_events, entry.total_events),
                )
            })
            .collect();

        assert_eq!(
            summary_by_method.get("tools/list"),
            Some(&(1_u64, 1_u64, 2_u64))
        );
        assert_eq!(
            summary_by_method.get("resources/read"),
            Some(&(1_u64, 1_u64, 2_u64))
        );

        let reason_summary = state.policy_audit_reason_summary().unwrap();
        let summary_by_reason: BTreeMap<PolicyReasonCode, u64> = reason_summary
            .into_iter()
            .map(|entry| (entry.reason, entry.events))
            .collect();
        assert_eq!(
            summary_by_reason.get(&PolicyReasonCode::MissingDataLabel),
            Some(&1_u64)
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn auth_audit_event_records_structured_evidence() {
        let path = temp_path("auth-audit");
        let state = GatewayState::open(&path).unwrap();

        state
            .record_auth_audit_event(&AuthAuditEvent {
                event_id: TraceId::new("event-1").unwrap(),
                timestamp: Utc::now(),
                trace_id: TraceId::new("trace-1").unwrap(),
                profile: GatewayProfileId::new("default").unwrap(),
                protected_resource: ProtectedResourceId::new("https://veoveo.bioma.ai/mcp/default")
                    .unwrap(),
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::MissingAuthorizationHeader,
                method: AuthMethod::BearerJwt,
                principal: None,
                tenant: None,
                token_issuer: None,
                token_subject: None,
                jwt_id: None,
                latency_ms: Some(3),
                metadata: BTreeMap::new(),
            })
            .unwrap();

        assert_eq!(state.auth_audit_event_count().unwrap(), 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn audit_retention_deletes_only_events_before_cutoff() {
        let path = temp_path("audit-retention");
        let state = GatewayState::open(&path).unwrap();
        let profile = GatewayProfileId::new("default").unwrap();
        let target = PolicyTarget::Tool {
            server: ServerSlug::new("media").unwrap(),
            tool: veoveo_mcp_contract::LocalToolName::new("run").unwrap(),
        };
        let old = Utc::now() - TimeDelta::days(10);
        let fresh = Utc::now();

        for (event_id, timestamp) in [("old-policy", old), ("fresh-policy", fresh)] {
            let trace_id = TraceId::new(format!("trace-{event_id}")).unwrap();
            let decision = PolicyDecision {
                effect: PolicyEffect::Allow,
                reason: PolicyReasonCode::PolicyAllow,
                evaluated_at: timestamp,
                profile: profile.clone(),
                action: GatewayAction::ToolsList,
                target: target.clone(),
                principal: Some(PrincipalId::new("issuer#subject").unwrap()),
                tenant: None,
                policy_version: None,
                rule_id: None,
                trace_id: trace_id.clone(),
            };
            state
                .record_audit_event(&AuditEvent {
                    event_id: TraceId::new(event_id).unwrap(),
                    timestamp,
                    trace_id,
                    profile: profile.clone(),
                    method: McpMethodName::new("tools/list").unwrap(),
                    action: GatewayAction::ToolsList,
                    target: target.clone(),
                    decision,
                    principal: Some(PrincipalId::new("issuer#subject").unwrap()),
                    tenant: None,
                    token_issuer: None,
                    latency_ms: Some(12),
                    metadata: BTreeMap::new(),
                })
                .unwrap();
        }

        for (event_id, timestamp) in [("old-auth", old), ("fresh-auth", fresh)] {
            state
                .record_auth_audit_event(&AuthAuditEvent {
                    event_id: TraceId::new(event_id).unwrap(),
                    timestamp,
                    trace_id: TraceId::new(format!("trace-{event_id}")).unwrap(),
                    profile: profile.clone(),
                    protected_resource: ProtectedResourceId::new(
                        "https://veoveo.bioma.ai/mcp/default",
                    )
                    .unwrap(),
                    outcome: AuthOutcome::Allow,
                    reason: AuthReasonCode::AuthAllow,
                    method: AuthMethod::BearerJwt,
                    principal: Some(PrincipalId::new("issuer#subject").unwrap()),
                    tenant: None,
                    token_issuer: None,
                    token_subject: None,
                    jwt_id: None,
                    latency_ms: Some(3),
                    metadata: BTreeMap::new(),
                })
                .unwrap();
        }

        assert_eq!(
            state
                .delete_audit_events_before(Utc::now() - TimeDelta::days(1))
                .unwrap(),
            GatewayAuditRetentionSummary {
                auth_events_deleted: 1,
                policy_events_deleted: 1,
            }
        );
        assert_eq!(
            state.audit_counts().unwrap(),
            GatewayAuditCounts {
                auth_events: 1,
                policy_events: 1,
            }
        );

        let _ = std::fs::remove_file(path);
    }
}
