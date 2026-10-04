#[path = "../../../testing/fixtures/catalog_registry.rs"]
mod catalog_fixture;

use std::collections::BTreeSet;
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{
    CanonicalTaskId, GatewayControlPlane, PolicyEffect, PolicyReasonCode, PolicyTarget, Principal,
    PrincipalKind, ServerSlug, TaskExposure, TokenIssuer, TokenSubject, TraceId,
};
use veoveo_policy::{PolicyCatalog, PolicyRequest, decide};
use veoveo_types::{PrincipalId, RoleId, ScopeName, TaskId, TenantId};

fn actor() -> Principal {
    Principal {
        id: PrincipalId::new("task-policy-test").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
        subject: TokenSubject::new("task-policy-test").unwrap(),
        tenant: Some(TenantId::new("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::from([RoleId::new("operator").unwrap()]),
        scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: None,
    }
}

fn check(plane: GatewayControlPlane, actor: &Principal, target: &PolicyTarget) -> PolicyReasonCode {
    let profile = plane.profiles[0].id.clone();
    let catalog = PolicyCatalog::new(plane, catalog_fixture::registry()).unwrap();
    decide(
        &catalog,
        PolicyRequest {
            principal: actor,
            profile: &profile,
            action: GatewayAction::TasksCancel.into(),
            target,
            trace_id: &TraceId::new("task-policy").unwrap(),
        },
    )
    .reason
}

#[test]
fn native_and_gateway_tasks_share_server_exposure_and_policy_requirements() {
    let plane: GatewayControlPlane =
        serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap();
    let server = ServerSlug::new("media").unwrap();
    for target in [
        PolicyTarget::PlatformTask {
            server: server.clone(),
            task_id: TaskId::new(),
        },
        PolicyTarget::Task {
            server,
            task_id: CanonicalTaskId::new("gtr_fixture").unwrap(),
        },
    ] {
        assert_eq!(
            check(plane.clone(), &actor(), &target),
            PolicyReasonCode::PolicyAllow
        );
        let mut missing_scope = actor();
        missing_scope.scopes.clear();
        assert_eq!(
            check(plane.clone(), &missing_scope, &target),
            PolicyReasonCode::MissingScope
        );
        let mut hidden = plane.clone();
        hidden.profiles[0].servers[0].tasks = TaskExposure::Disabled;
        assert_eq!(
            check(hidden, &actor(), &target),
            PolicyReasonCode::PolicyDeny
        );
        let mut denied = plane.clone();
        denied.policies[0].rules[0].effect = PolicyEffect::Deny;
        denied.policies[0].rules[0].servers = BTreeSet::from([ServerSlug::new("media").unwrap()]);
        assert_eq!(
            check(denied, &actor(), &target),
            PolicyReasonCode::PolicyDeny
        );
    }
}
