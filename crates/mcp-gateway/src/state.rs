use std::path::Path;

use anyhow::Result;
use duckdb::{OptionalExt, params};
use serde::Serialize;
use veoveo_mcp_contract::{
    AuditEvent, AuthAuditEvent, GatewayProfileId, GatewayTaskId, GatewayTaskMapping, PrincipalId,
    SharedDuckDbConnection, UpstreamTaskId, open_duckdb,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GatewayAuditCounts {
    pub auth_events: u64,
    pub policy_events: u64,
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
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use chrono::Utc;
    use std::collections::BTreeMap;

    use veoveo_mcp_contract::{
        AuthMethod, AuthOutcome, AuthReasonCode, GatewayAction, McpMethodName, PolicyDecision,
        PolicyEffect, PolicyReasonCode, PolicyTarget, ProtectedResourceId, ServerSlug, TraceId,
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
}
