use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::Utc;
use veoveo_mcp_contract::{
    AuthAuditEvent, AuthMethod, AuthOutcome, AuthReasonCode, GatewayProfileId, ProtectedResourceId,
    TraceId,
};

use super::GatewayState;

fn temp_path(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let process_id = std::process::id();
    std::env::temp_dir().join(format!(
        "veoveo-gateway-{name}-{process_id}-{unique}.duckdb"
    ))
}

fn record_auth_audit(
    state: &GatewayState,
    event_id: &str,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    method: AuthMethod,
) {
    record_auth_audit_with_metadata(state, event_id, outcome, reason, method, BTreeMap::new());
}

fn record_auth_audit_with_metadata(
    state: &GatewayState,
    event_id: &str,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    method: AuthMethod,
    metadata: BTreeMap<String, String>,
) {
    state
        .record_auth_audit_event(&AuthAuditEvent {
            event_id: TraceId::new(event_id).unwrap(),
            timestamp: Utc::now(),
            trace_id: TraceId::new(format!("trace-{event_id}")).unwrap(),
            profile: GatewayProfileId::new("default").unwrap(),
            protected_resource: ProtectedResourceId::new("https://veoveo.bioma.ai/mcp/default")
                .unwrap(),
            outcome,
            reason,
            method,
            principal: None,
            principal_attributes: None,
            tenant: None,
            token_issuer: None,
            token_subject: None,
            jwt_id: None,
            latency_ms: Some(3),
            metadata,
        })
        .unwrap();
}

#[test]
fn auth_audit_event_records_structured_evidence() {
    let path = temp_path("auth-audit");
    let state = GatewayState::open(&path).unwrap();

    record_auth_audit(
        &state,
        "event-1",
        AuthOutcome::Deny,
        AuthReasonCode::MissingAuthorizationHeader,
        AuthMethod::BearerJwt,
    );
    record_auth_audit(
        &state,
        "event-2",
        AuthOutcome::Allow,
        AuthReasonCode::AuthAllow,
        AuthMethod::BearerJwt,
    );
    record_auth_audit(
        &state,
        "event-3",
        AuthOutcome::Deny,
        AuthReasonCode::ClientAssertionReplay,
        AuthMethod::ClientCredentialsPrivateKeyJwt,
    );
    record_auth_audit_with_metadata(
        &state,
        "event-4",
        AuthOutcome::Allow,
        AuthReasonCode::AuthAllow,
        AuthMethod::EnterpriseManagedIdJag,
        BTreeMap::from([
            ("principal_kind".to_string(), "user".to_string()),
            ("principal_data_labels".to_string(), "cui".to_string()),
            ("principal_assurances".to_string(), "us_person".to_string()),
        ]),
    );

    assert_eq!(state.auth_audit_event_count().unwrap(), 4);
    let method_summary: BTreeMap<AuthMethod, (u64, u64, u64)> = state
        .auth_audit_method_summary()
        .unwrap()
        .into_iter()
        .map(|entry| {
            (
                entry.method,
                (entry.allow_events, entry.deny_events, entry.total_events),
            )
        })
        .collect();
    assert_eq!(method_summary.get(&AuthMethod::BearerJwt), Some(&(1, 1, 2)));
    assert_eq!(
        method_summary.get(&AuthMethod::ClientCredentialsPrivateKeyJwt),
        Some(&(0, 1, 1))
    );
    assert_eq!(
        method_summary.get(&AuthMethod::EnterpriseManagedIdJag),
        Some(&(1, 0, 1))
    );
    let reason_summary: BTreeMap<AuthReasonCode, u64> = state
        .auth_audit_reason_summary()
        .unwrap()
        .into_iter()
        .map(|entry| (entry.reason, entry.events))
        .collect();
    assert_eq!(
        reason_summary.get(&AuthReasonCode::MissingAuthorizationHeader),
        Some(&1)
    );
    assert_eq!(reason_summary.get(&AuthReasonCode::AuthAllow), Some(&2));
    assert_eq!(
        reason_summary.get(&AuthReasonCode::ClientAssertionReplay),
        Some(&1)
    );
    let metadata_summary: BTreeMap<String, u64> = state
        .auth_audit_metadata_summary("principal_data_labels")
        .unwrap()
        .into_iter()
        .map(|entry| (entry.metadata_value, entry.events))
        .collect();
    assert_eq!(metadata_summary.get("cui"), Some(&1));

    let _ = std::fs::remove_file(path);
}
