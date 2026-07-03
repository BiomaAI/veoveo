use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use duckdb::params;
use serde::Serialize;
use veoveo_mcp_contract::{
    AuditEvent, AuthAuditEvent, AuthMethod, AuthOutcome, AuthReasonCode, McpMethodName,
    PolicyDecision, PolicyEffect, PolicyReasonCode,
};

use super::GatewayState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GatewayAuditCounts {
    pub auth_events: u64,
    pub policy_events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayAuthAuditMethodSummary {
    pub method: AuthMethod,
    pub allow_events: u64,
    pub deny_events: u64,
    pub total_events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayAuthAuditReasonSummary {
    pub reason: AuthReasonCode,
    pub events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayAuthAuditMetadataSummary {
    pub metadata_key: String,
    pub metadata_value: String,
    pub events: u64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayPolicyAuditMetadataSummary {
    pub metadata_key: String,
    pub metadata_value: String,
    pub events: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct GatewayAuditRetentionSummary {
    pub auth_events_deleted: u64,
    pub policy_events_deleted: u64,
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
    pub fn audit_counts(&self) -> Result<GatewayAuditCounts> {
        Ok(GatewayAuditCounts {
            auth_events: self.count_rows(GatewayAuditTable::Auth)?,
            policy_events: self.count_rows(GatewayAuditTable::Policy)?,
        })
    }

    pub fn auth_audit_method_summary(&self) -> Result<Vec<GatewayAuthAuditMethodSummary>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT event_json
            FROM gateway_auth_audit_events
            ORDER BY method, timestamp, event_id
            "#,
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut summaries = BTreeMap::<AuthMethod, GatewayAuthAuditMethodSummary>::new();
        for row in rows {
            let event: AuthAuditEvent = serde_json::from_str(&row?)?;
            let entry = summaries
                .entry(event.method)
                .or_insert(GatewayAuthAuditMethodSummary {
                    method: event.method,
                    allow_events: 0,
                    deny_events: 0,
                    total_events: 0,
                });
            match event.outcome {
                AuthOutcome::Allow => entry.allow_events += 1,
                AuthOutcome::Deny => entry.deny_events += 1,
            }
            entry.total_events += 1;
        }
        Ok(summaries.into_values().collect())
    }

    pub fn auth_audit_reason_summary(&self) -> Result<Vec<GatewayAuthAuditReasonSummary>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT event_json
            FROM gateway_auth_audit_events
            ORDER BY reason, timestamp, event_id
            "#,
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut summaries = BTreeMap::<AuthReasonCode, GatewayAuthAuditReasonSummary>::new();
        for row in rows {
            let event: AuthAuditEvent = serde_json::from_str(&row?)?;
            let entry = summaries
                .entry(event.reason)
                .or_insert(GatewayAuthAuditReasonSummary {
                    reason: event.reason,
                    events: 0,
                });
            entry.events += 1;
        }
        Ok(summaries.into_values().collect())
    }

    pub fn auth_audit_metadata_summary(
        &self,
        metadata_key: &str,
    ) -> Result<Vec<GatewayAuthAuditMetadataSummary>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT event_json
            FROM gateway_auth_audit_events
            ORDER BY timestamp, event_id
            "#,
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut summaries = BTreeMap::<String, u64>::new();
        for row in rows {
            let event: AuthAuditEvent = serde_json::from_str(&row?)?;
            if let Some(metadata_value) = event.metadata.get(metadata_key) {
                *summaries.entry(metadata_value.clone()).or_default() += 1;
            }
        }
        Ok(summaries
            .into_iter()
            .map(|(metadata_value, events)| GatewayAuthAuditMetadataSummary {
                metadata_key: metadata_key.to_string(),
                metadata_value,
                events,
            })
            .collect())
    }

    pub fn policy_audit_method_summary(&self) -> Result<Vec<GatewayPolicyAuditMethodSummary>> {
        let conn = self.conn.lock();
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
        let mut summaries = BTreeMap::<McpMethodName, GatewayPolicyAuditMethodSummary>::new();
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
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT decision_json
            FROM gateway_audit_events
            ORDER BY timestamp, event_id
            "#,
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut summaries = BTreeMap::<PolicyReasonCode, GatewayPolicyAuditReasonSummary>::new();
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

    pub fn policy_audit_metadata_summary(
        &self,
        metadata_key: &str,
    ) -> Result<Vec<GatewayPolicyAuditMetadataSummary>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT event_json
            FROM gateway_audit_events
            ORDER BY timestamp, event_id
            "#,
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut summaries = BTreeMap::<String, u64>::new();
        for row in rows {
            let event: AuditEvent = serde_json::from_str(&row?)?;
            if let Some(metadata_value) = event.metadata.get(metadata_key) {
                *summaries.entry(metadata_value.clone()).or_default() += 1;
            }
        }
        Ok(summaries
            .into_iter()
            .map(
                |(metadata_value, events)| GatewayPolicyAuditMetadataSummary {
                    metadata_key: metadata_key.to_string(),
                    metadata_value,
                    events,
                },
            )
            .collect())
    }

    pub fn delete_audit_events_before(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<GatewayAuditRetentionSummary> {
        let conn = self.conn.lock();
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

    pub fn record_audit_event(&self, event: &AuditEvent) -> Result<()> {
        let target_json = serde_json::to_string(&event.target)?;
        let decision_json = serde_json::to_string(&event.decision)?;
        let event_json = serde_json::to_string(event)?;
        let conn = self.conn.lock();
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
        let conn = self.conn.lock();
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
    pub(super) fn audit_event_count(&self) -> Result<u64> {
        self.count_rows(GatewayAuditTable::Policy)
    }

    #[cfg(test)]
    pub(super) fn auth_audit_event_count(&self) -> Result<u64> {
        self.count_rows(GatewayAuditTable::Auth)
    }

    fn count_rows(&self, table: GatewayAuditTable) -> Result<u64> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(table.count_sql(), [], |row| row.get(0))?;
        Ok(u64::try_from(count)?)
    }
}
