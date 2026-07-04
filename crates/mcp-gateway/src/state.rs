use std::path::Path;

use anyhow::Result;
use veoveo_mcp_contract::{SharedDuckDbConnection, open_duckdb};

mod audit;
mod auth_state;
mod schema;
mod subscriptions;
mod tasks;

#[cfg(test)]
mod auth_audit_tests;

pub use audit::{
    GatewayAuditCounts, GatewayAuditRetentionSummary, GatewayAuthAuditMetadataSummary,
    GatewayAuthAuditMethodSummary, GatewayAuthAuditReasonSummary,
    GatewayPolicyAuditMetadataSummary, GatewayPolicyAuditMethodSummary,
    GatewayPolicyAuditReasonSummary,
};
pub use auth_state::GatewayReplayRetentionSummary;

#[derive(Debug, Clone)]
pub struct GatewayState {
    conn: SharedDuckDbConnection,
}

impl GatewayState {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = open_duckdb(path)?;
        let state = Self { conn };
        state.initialize()?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use chrono::{TimeDelta, Utc};
    use std::collections::{BTreeMap, BTreeSet};

    use veoveo_mcp_contract::{
        AuditEvent, AuthAuditEvent, AuthMethod, AuthOutcome, AuthReasonCode, AuthorizationServerId,
        DataLabelId, GatewayAction, GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest,
        GatewayJwtRevocation, GatewayProfileId, GatewayResourceSubscription, GatewayTaskId,
        GatewayTaskMapping, GroupId, JwtId, McpMethodName, OAuthAuthorizationCode, OAuthClientId,
        OAuthRedirectUri, OAuthStateValue, OidcClientRegistrationId, OidcNonce, PkceCodeChallenge,
        PkceCodeChallengeMethod, PkceCodeVerifier, PolicyDecision, PolicyEffect, PolicyReasonCode,
        PolicyTarget, Principal, PrincipalAssurance, PrincipalAuditAttributes, PrincipalId,
        PrincipalKind, ProtectedResourceId, ResourceUri, RoleId, ScopeName, ServerSlug,
        TokenIssuer, TokenSubject, TraceId, UpstreamTaskId,
    };

    use super::*;

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

    fn record_policy_audit(
        state: &GatewayState,
        event_id: &str,
        method: &str,
        action: GatewayAction,
        effect: PolicyEffect,
        reason: PolicyReasonCode,
    ) {
        record_policy_audit_with_metadata(
            state,
            event_id,
            method,
            action,
            effect,
            reason,
            BTreeMap::new(),
        );
    }

    fn record_policy_audit_with_metadata(
        state: &GatewayState,
        event_id: &str,
        method: &str,
        action: GatewayAction,
        effect: PolicyEffect,
        reason: PolicyReasonCode,
        metadata: BTreeMap<String, String>,
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
                principal_attributes: None,
                tenant: None,
                token_issuer: None,
                latency_ms: Some(12),
                metadata,
            })
            .unwrap();
    }

    #[test]
    fn task_mapping_round_trips_by_gateway_and_upstream_ids() {
        let path = temp_path("tasks");
        let state = GatewayState::open(&path).unwrap();
        let now = Utc::now();
        let mapping = GatewayTaskMapping {
            gateway_task_id: GatewayTaskId::new("gateway-task-1").unwrap(),
            upstream_server: ServerSlug::new("media").unwrap(),
            upstream_task_id: UpstreamTaskId::new("upstream-task-1").unwrap(),
            profile: GatewayProfileId::new("default").unwrap(),
            owner: PrincipalId::new("issuer#subject").unwrap(),
            created_at: now,
            updated_at: now,
        };
        let other_server_mapping = GatewayTaskMapping {
            gateway_task_id: GatewayTaskId::new("gateway-task-2").unwrap(),
            upstream_server: ServerSlug::new("simulation").unwrap(),
            upstream_task_id: UpstreamTaskId::new("upstream-task-2").unwrap(),
            profile: mapping.profile.clone(),
            owner: mapping.owner.clone(),
            created_at: now,
            updated_at: now + chrono::TimeDelta::seconds(1),
        };

        state.record_task_mapping(&mapping).unwrap();
        state.record_task_mapping(&other_server_mapping).unwrap();

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
            vec![mapping.clone()]
        );
        assert_eq!(
            state
                .task_mappings_for_profile_owner(&mapping.profile, &mapping.owner)
                .unwrap(),
            vec![mapping, other_server_mapping]
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
    fn replay_id_retention_prunes_expired_client_assertion_and_id_jag_rows() {
        let path = temp_path("replay-id-retention");
        let state = GatewayState::open(&path).unwrap();
        let authorization_server = AuthorizationServerId::new("veoveo").unwrap();
        let client_id = OAuthClientId::new("veoveo-headless").unwrap();
        let old_seen_at = Utc::now() - TimeDelta::minutes(10);
        let now = Utc::now();
        let old_expires_at = now - TimeDelta::minutes(5);
        let fresh_expires_at = now + TimeDelta::minutes(5);
        let old_client_assertion_jti = JwtId::new("old-client-assertion").unwrap();
        let fresh_client_assertion_jti = JwtId::new("fresh-client-assertion").unwrap();
        let old_id_jag_jti = JwtId::new("old-id-jag").unwrap();
        let fresh_id_jag_jti = JwtId::new("fresh-id-jag").unwrap();

        assert!(
            state
                .record_client_assertion_jti(
                    &authorization_server,
                    &client_id,
                    &old_client_assertion_jti,
                    old_expires_at,
                    old_seen_at,
                )
                .unwrap()
        );
        assert!(
            state
                .record_client_assertion_jti(
                    &authorization_server,
                    &client_id,
                    &fresh_client_assertion_jti,
                    fresh_expires_at,
                    old_seen_at,
                )
                .unwrap()
        );
        assert!(
            state
                .record_id_jag_jti(
                    &authorization_server,
                    &client_id,
                    &old_id_jag_jti,
                    old_expires_at,
                    old_seen_at,
                )
                .unwrap()
        );
        assert!(
            state
                .record_id_jag_jti(
                    &authorization_server,
                    &client_id,
                    &fresh_id_jag_jti,
                    fresh_expires_at,
                    old_seen_at,
                )
                .unwrap()
        );

        assert_eq!(
            state.prune_expired_replay_ids(now).unwrap(),
            GatewayReplayRetentionSummary {
                client_assertion_jtis_deleted: 1,
                id_jag_jtis_deleted: 1,
            }
        );
        assert!(
            state
                .record_client_assertion_jti(
                    &authorization_server,
                    &client_id,
                    &old_client_assertion_jti,
                    fresh_expires_at,
                    now + TimeDelta::seconds(1),
                )
                .unwrap()
        );
        assert!(
            !state
                .record_client_assertion_jti(
                    &authorization_server,
                    &client_id,
                    &fresh_client_assertion_jti,
                    fresh_expires_at,
                    now + TimeDelta::seconds(1),
                )
                .unwrap()
        );
        assert!(
            state
                .record_id_jag_jti(
                    &authorization_server,
                    &client_id,
                    &old_id_jag_jti,
                    fresh_expires_at,
                    now + TimeDelta::seconds(1),
                )
                .unwrap()
        );
        assert!(
            !state
                .record_id_jag_jti(
                    &authorization_server,
                    &client_id,
                    &fresh_id_jag_jti,
                    fresh_expires_at,
                    now + TimeDelta::seconds(1),
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
            requested_scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
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
            scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(now),
        };
        let code = GatewayAuthorizationCodeRecord {
            code: OAuthAuthorizationCode::new("B".repeat(43)).unwrap(),
            profile: GatewayProfileId::new("default").unwrap(),
            oauth_client_id: OAuthClientId::new("veoveo-browser").unwrap(),
            oidc_client: OidcClientRegistrationId::new("enterprise").unwrap(),
            redirect_uri: OAuthRedirectUri::new("https://veoveo.bioma.ai/oauth/callback").unwrap(),
            client_state: Some(OAuthStateValue::new("client-state-1").unwrap()),
            scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
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
        let principal = Principal {
            id: PrincipalId::new("issuer#subject").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("issuer").unwrap(),
            subject: TokenSubject::new("subject").unwrap(),
            tenant: None,
            groups: BTreeSet::from([GroupId::new("engineering").unwrap()]),
            roles: BTreeSet::from([RoleId::new("operator").unwrap()]),
            scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
            data_labels: BTreeSet::from([DataLabelId::new("cui").unwrap()]),
            assurances: BTreeSet::from([PrincipalAssurance::UsPerson]),
            authenticated_at: Some(Utc::now()),
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
                principal: Some(principal.id.clone()),
                principal_attributes: Some(PrincipalAuditAttributes::from(&principal)),
                tenant: None,
                token_issuer: None,
                latency_ms: Some(12),
                metadata: BTreeMap::new(),
            })
            .unwrap();

        assert_eq!(state.audit_event_count().unwrap(), 1);
        let event_json: String = state
            .conn
            .lock()
            .query_row("SELECT event_json FROM gateway_audit_events", [], |row| {
                row.get(0)
            })
            .unwrap();
        let event: AuditEvent = serde_json::from_str(&event_json).unwrap();
        let attributes = event
            .principal_attributes
            .expect("principal audit attributes should be stored");
        assert_eq!(attributes.kind, PrincipalKind::User);
        assert_eq!(attributes.groups, principal.groups);
        assert_eq!(attributes.roles, principal.roles);
        assert_eq!(attributes.scopes, principal.scopes);
        assert_eq!(attributes.data_labels, principal.data_labels);
        assert_eq!(attributes.assurances, principal.assurances);

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
    fn policy_audit_metadata_summary_counts_selected_metadata_values() {
        let path = temp_path("audit-metadata-summary");
        let state = GatewayState::open(&path).unwrap();

        record_policy_audit_with_metadata(
            &state,
            "event-admin-succeeded-1",
            "admin/control-plane/result",
            GatewayAction::AdminWrite,
            PolicyEffect::Allow,
            PolicyReasonCode::PolicyAllow,
            BTreeMap::from([("operation_status".to_string(), "succeeded".to_string())]),
        );
        record_policy_audit_with_metadata(
            &state,
            "event-admin-succeeded-2",
            "admin/jwt-revocations/result",
            GatewayAction::AdminWrite,
            PolicyEffect::Allow,
            PolicyReasonCode::PolicyAllow,
            BTreeMap::from([("operation_status".to_string(), "succeeded".to_string())]),
        );
        record_policy_audit_with_metadata(
            &state,
            "event-admin-rejected",
            "admin/jwt-revocations/result",
            GatewayAction::AdminWrite,
            PolicyEffect::Allow,
            PolicyReasonCode::PolicyAllow,
            BTreeMap::from([("operation_status".to_string(), "rejected".to_string())]),
        );

        let summary = state
            .policy_audit_metadata_summary("operation_status")
            .unwrap();
        let summary_by_value: BTreeMap<String, u64> = summary
            .into_iter()
            .map(|entry| (entry.metadata_value, entry.events))
            .collect();
        assert_eq!(summary_by_value.get("succeeded"), Some(&2_u64));
        assert_eq!(summary_by_value.get("rejected"), Some(&1_u64));

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
                    principal_attributes: None,
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
                        "https://veoveo.bioma.ai/mcp/operator",
                    )
                    .unwrap(),
                    outcome: AuthOutcome::Allow,
                    reason: AuthReasonCode::AuthAllow,
                    method: AuthMethod::BearerJwt,
                    principal: Some(PrincipalId::new("issuer#subject").unwrap()),
                    principal_attributes: None,
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
