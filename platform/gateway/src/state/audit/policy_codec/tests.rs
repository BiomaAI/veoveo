use super::super::canonical_policy_record;
use super::*;
use crate::GatewayState;
use chrono::Utc;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use veoveo_mcp_contract::{
    GatewayProfileId, McpMethodName, PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyRuleId,
    PrincipalAuditAttributes, PrincipalKind, ServerSlug, TokenIssuer, TraceId,
};
use veoveo_platform_store::GatewayAuditKind;
use veoveo_types::{
    PolicyVersion, PrincipalId, ResourceTemplateUri, ResourceUri, ScopeName, TenantId,
};

use crate::test_store as fixture;

fn event(action: GatewayAction, target: PolicyTarget) -> AuditEvent {
    let trace = TraceId::new(uuid::Uuid::now_v7().to_string()).unwrap();
    let profile = GatewayProfileId::new("operator").unwrap();
    let timestamp = Utc::now();
    let principal = Some(PrincipalId::new("template-fixture-actor").unwrap());
    let tenant = Some(TenantId::new("template-fixture-tenant").unwrap());
    let decision = PolicyDecision {
        effect: PolicyEffect::Allow,
        reason: PolicyReasonCode::PolicyAllow,
        evaluated_at: timestamp,
        profile: profile.clone(),
        action,
        target: target.clone(),
        principal: principal.clone(),
        tenant: tenant.clone(),
        policy_version: Some(PolicyVersion::new("fixture-policy").unwrap()),
        rule_id: Some(PolicyRuleId::new("allow-template").unwrap()),
        trace_id: trace.clone(),
    };
    AuditEvent {
        event_id: trace.clone(),
        timestamp,
        trace_id: trace,
        profile,
        method: McpMethodName::new(action.mcp_method().unwrap()).unwrap(),
        action,
        target,
        decision,
        principal,
        principal_attributes: Some(PrincipalAuditAttributes {
            kind: PrincipalKind::User,
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(timestamp),
        }),
        tenant,
        token_issuer: Some(TokenIssuer::new("https://fixture.example").unwrap()),
        latency_ms: Some(12),
        metadata: BTreeMap::from([("fixture".into(), "retained".into())]),
    }
}

fn resource(uri: &str) -> PolicyTarget {
    PolicyTarget::Resource {
        server: ServerSlug::new("media").unwrap(),
        uri: ResourceUri::new(uri).unwrap(),
    }
}

fn template(uri: &str) -> PolicyTarget {
    PolicyTarget::ResourceTemplate {
        server: ServerSlug::new("media").unwrap(),
        uri: ResourceTemplateUri::new(uri).unwrap(),
    }
}

fn legacy(event: &AuditEvent) -> OpenObject {
    OpenObject::new(BTreeMap::from([
        ("gateway_kind".into(), "gateway_policy".into()),
        ("event".into(), serde_json::to_value(event).unwrap()),
    ]))
}

#[test]
fn v1_decoding_uses_the_action_for_literal_and_expressive_templates() {
    for action in [
        GatewayAction::CompletionComplete,
        GatewayAction::ResourcesTemplatesList,
    ] {
        for uri in ["media://model/literal", "media://model/{+id}{?cursor}"] {
            let mut old = event(action, resource(uri));
            old.decision.effect = PolicyEffect::Deny;
            old.decision.reason = PolicyReasonCode::MissingScope;
            let decoded = decode(&legacy(&old)).unwrap();
            let mut expected = old;
            expected.target = template(uri);
            expected.decision.target = expected.target.clone();
            assert_eq!(decoded, expected);
        }
    }
    // Discovery historically classified usage/artifact templates as read targets.
    for old in [
        PolicyTarget::Usage {
            server: ServerSlug::new("media").unwrap(),
            usage_uri: ResourceUri::new("media://usage/task/{task_id}").unwrap(),
        },
        PolicyTarget::Artifact {
            server: ServerSlug::new("media").unwrap(),
            artifact_uri: ResourceUri::new("media://artifact/{artifact_id}").unwrap(),
        },
    ] {
        let decoded = decode(&legacy(&event(GatewayAction::ResourcesTemplatesList, old))).unwrap();
        assert!(matches!(
            decoded.target,
            PolicyTarget::ResourceTemplate { .. }
        ));
        assert_eq!(decoded.target, decoded.decision.target);
    }
    let read = event(
        GatewayAction::ResourcesRead,
        resource("media://model/literal"),
    );
    assert_eq!(decode(&legacy(&read)).unwrap(), read);
    let prompt = event(
        GatewayAction::CompletionComplete,
        PolicyTarget::Prompt {
            server: ServerSlug::new("media").unwrap(),
            prompt: veoveo_mcp_contract::PromptName::new("media-model-select").unwrap(),
        },
    );
    assert_eq!(decode(&legacy(&prompt)).unwrap(), prompt);
}

#[test]
fn v2_writes_and_reads_preserve_event_identity_and_template_target() {
    for action in [
        GatewayAction::CompletionComplete,
        GatewayAction::ResourcesTemplatesList,
    ] {
        let event = event(action, template("media://model/{+id}{?cursor}"));
        let record = canonical_policy_record(&event).unwrap();
        assert_eq!(record.details.as_map()[FORMAT_KEY], CURRENT_FORMAT);
        assert_eq!(
            record.details.as_map()["event"]["target"]["kind"],
            "resource_template"
        );
        assert_eq!(decode(&record.details).unwrap(), event);
        assert_eq!(record.request_id.as_deref(), Some(event.event_id.as_str()));
    }
}

#[test]
fn incompatible_versions_and_inconsistent_targets_fail_without_payload_disclosure() {
    let current = event(
        GatewayAction::CompletionComplete,
        template("media://model/{id}"),
    );
    assert!(decode(&legacy(&current)).is_err());
    let old = event(
        GatewayAction::CompletionComplete,
        resource("media://private-fixture/{id:65536}"),
    );
    let error = decode(&legacy(&old)).unwrap_err();
    assert!(!format!("{error:#}").contains("private-fixture"));
    for version in [
        serde_json::json!("private-fixture-future-version"),
        serde_json::json!(null),
        serde_json::json!(2),
    ] {
        let mut details = legacy(&current).as_map().clone();
        details.insert(FORMAT_KEY.into(), version);
        let error = decode(&OpenObject::new(details)).unwrap_err();
        assert!(!format!("{error:#}").contains("private-fixture"));
    }
    let mut mismatch = current.clone();
    mismatch.decision.action = GatewayAction::ResourcesRead;
    assert!(canonical_policy_record(&mismatch).is_err());
    mismatch = current;
    mismatch.decision.target = template("media://other/{id}");
    assert!(canonical_policy_record(&mismatch).is_err());
    assert!(
        canonical_policy_record(&event(
            GatewayAction::ResourcesRead,
            template("media://model/{id}")
        ))
        .is_err()
    );
    let old = event(
        GatewayAction::CompletionComplete,
        resource("media://model/literal"),
    );
    assert!(canonical_policy_record(&old).is_err());
    assert!(decode(&mark_current(legacy(&old))).is_err());
}

#[tokio::test]
async fn separate_store_connections_read_both_formats_without_rewriting_legacy_rows() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let writer = GatewayState::new(db.a.clone());
        let reader = GatewayState::new(db.b.clone());
        let current = event(
            GatewayAction::CompletionComplete,
            template("media://model/{+id}{?cursor}"),
        );
        writer.record_audit_event(&current).await.unwrap();
        let old = event(
            GatewayAction::CompletionComplete,
            resource("media://model/literal"),
        );
        let mut upgraded = old.clone();
        upgraded.target = template("media://model/literal");
        upgraded.decision.target = upgraded.target.clone();
        let mut old_record = canonical_policy_record(&upgraded).unwrap();
        old_record.details = legacy(&old);
        db.a.record_gateway_audit_event(GatewayAuditKind::Policy, old_record.clone())
            .await
            .unwrap();
        let events = reader.policy_audit_events().await.unwrap();
        assert_eq!(events, [current, upgraded]);
        let stored =
            db.b.gateway_audit_events(GatewayAuditKind::Policy)
                .await
                .unwrap();
        assert_eq!(
            stored
                .iter()
                .find(|record| record.id == old_record.id)
                .unwrap(),
            &old_record
        );
        assert_eq!(
            reader.policy_audit_method_summary().await.unwrap()[0].allow_events,
            2
        );
        assert!(writer.record_audit_event(&old).await.is_err());
        assert_eq!(reader.audit_counts().await.unwrap().policy_events, 2);
    })
    .await
    .expect("isolated policy audit qualification exceeded 90 seconds");
}
