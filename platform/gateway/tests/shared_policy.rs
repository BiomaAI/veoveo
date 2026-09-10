//! The lightweight worker catalog and gateway index must make the same decisions.
use std::collections::BTreeSet;
use veoveo_mcp_contract::{
    GatewayAction, GatewayControlPlane, LocalToolName, PolicyTarget, Principal, PrincipalId,
    PrincipalKind, RoleId, TenantId, TokenIssuer, TokenSubject, TraceId,
};
use veoveo_mcp_gateway::{GatewayCatalog, PolicyRequest};
use veoveo_policy::PolicyCatalog;

#[test]
fn gateway_and_background_catalogs_preserve_policy_semantics() {
    let plane: GatewayControlPlane =
        serde_json::from_str(include_str!("../../../configs/gateway.local.json")).unwrap();
    let gateway = GatewayCatalog::from_control_plane(plane.clone()).unwrap();
    let background = PolicyCatalog::new(plane.clone()).unwrap();
    for kind in [PrincipalKind::User, PrincipalKind::Service] {
        for profile in &plane.profiles {
            for with_scopes in [false, true] {
                let principal = Principal {
                    id: PrincipalId::new("https://policy.test#actor").unwrap(),
                    kind,
                    issuer: TokenIssuer::new("https://policy.test").unwrap(),
                    subject: TokenSubject::new("actor").unwrap(),
                    tenant: Some(TenantId::new("enterprise").unwrap()),
                    groups: BTreeSet::new(),
                    group_roles: BTreeSet::new(),
                    roles: BTreeSet::from([
                        RoleId::new("operator").unwrap(),
                        RoleId::new("administrator").unwrap(),
                    ]),
                    scopes: if with_scopes {
                        profile.required_scopes.iter().cloned().collect()
                    } else {
                        BTreeSet::new()
                    },
                    data_labels: BTreeSet::new(),
                    assurances: BTreeSet::new(),
                    authenticated_at: None,
                };
                for server in &plane.servers {
                    for tool in server
                        .tools
                        .iter()
                        .cloned()
                        .chain([LocalToolName::new("unregistered_policy_probe").unwrap()])
                    {
                        let target = PolicyTarget::Tool {
                            server: server.slug.clone(),
                            tool,
                        };
                        let trace = TraceId::new("shared-policy-probe").unwrap();
                        let request = PolicyRequest {
                            principal: &principal,
                            profile: &profile.id,
                            action: GatewayAction::ToolsCall,
                            target: &target,
                            trace_id: &trace,
                        };
                        let expected = gateway.decide(request.clone());
                        let mut actual = veoveo_policy::decide(&background, request);
                        // The evaluator records the time of each separate call.
                        actual.evaluated_at = expected.evaluated_at;
                        assert_eq!(actual, expected);
                    }
                }
            }
        }
    }
}
