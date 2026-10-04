#[path = "../../../testing/fixtures/catalog_registry.rs"]
mod catalog_fixture;

use std::collections::BTreeSet;
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{
    Exposure, GatewayControlPlane, PolicyEffect, PolicyReasonCode, PolicyRuleId, PolicyTarget,
    Principal, PrincipalKind, ResourceProjectionMode, ResourceSelector, ResourceUriPrefix,
    ResourceUriTemplate, ServerSlug, TokenIssuer, TokenSubject, TraceId,
};
use veoveo_policy::{PolicyCatalog, PolicyRequest, decide};
use veoveo_types::{
    PrincipalId, ResourceScheme, ResourceTemplateUri, ResourceUri, RoleId, ScopeName, TenantId,
};

fn plane() -> GatewayControlPlane {
    serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap()
}

fn principal() -> Principal {
    Principal {
        id: PrincipalId::new("template-test").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
        subject: TokenSubject::new("template-test").unwrap(),
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

fn check(
    plane: GatewayControlPlane,
    actor: &Principal,
    action: GatewayAction,
    target: &PolicyTarget,
) -> PolicyReasonCode {
    let profile = plane.profiles[0].id.clone();
    let catalog = PolicyCatalog::new(plane, catalog_fixture::registry()).unwrap();
    decide(
        &catalog,
        PolicyRequest {
            principal: actor,
            profile: &profile,
            action: action.into(),
            target,
            trace_id: &TraceId::new("template-policy").unwrap(),
        },
    )
    .reason
}

fn target(uri: &str) -> PolicyTarget {
    PolicyTarget::ResourceTemplate {
        server: ServerSlug::new("media").unwrap(),
        uri: ResourceTemplateUri::new(uri).unwrap(),
    }
}

#[test]
fn declarations_keep_lexical_selectors_and_do_not_grant_expanded_reads() {
    let selectors = [
        (Exposure::All, true),
        (Exposure::None, false),
        (
            Exposure::Listed(vec![ResourceSelector::Scheme {
                scheme: ResourceScheme::new("media").unwrap(),
            }]),
            true,
        ),
        (
            Exposure::Listed(vec![ResourceSelector::UriPrefix {
                prefix: ResourceUriPrefix::new("media://model/").unwrap(),
            }]),
            true,
        ),
        (
            Exposure::Listed(vec![ResourceSelector::UriPrefix {
                prefix: ResourceUriPrefix::new("media://model/fixed").unwrap(),
            }]),
            false,
        ),
        (
            Exposure::Listed(vec![ResourceSelector::Template {
                uri_template: ResourceUriTemplate::new("media://model/{id}").unwrap(),
            }]),
            true,
        ),
        (
            Exposure::Listed(vec![ResourceSelector::Template {
                uri_template: ResourceUriTemplate::new("media://other/{id}").unwrap(),
            }]),
            false,
        ),
    ];
    for (selectors, allowed) in selectors {
        let mut plane = plane();
        plane.profiles[0].servers[0].resources = selectors;
        for action in [
            GatewayAction::CompletionComplete,
            GatewayAction::ResourcesTemplatesList,
        ] {
            for uri in [
                "media://model/{id}",
                "media://model/{+id}{?cursor}",
                "media://model/literal",
            ] {
                assert_eq!(
                    check(plane.clone(), &principal(), action, &target(uri)),
                    if allowed {
                        PolicyReasonCode::PolicyAllow
                    } else {
                        PolicyReasonCode::PolicyDeny
                    },
                    "{action:?} {uri}"
                );
            }
        }
        assert_eq!(
            check(
                plane,
                &principal(),
                GatewayAction::ResourcesRead,
                &target("media://model/{id}")
            ),
            PolicyReasonCode::PolicyDeny
        );
    }
    // A catalog declaration cannot replace authorization of a particular address.
    let mut plane = plane();
    plane.profiles[0].servers[0].resources = Exposure::Listed(vec![ResourceSelector::UriPrefix {
        prefix: ResourceUriPrefix::new("media://model/allowed/").unwrap(),
    }]);
    let read = PolicyTarget::Resource {
        server: ServerSlug::new("media").unwrap(),
        uri: ResourceUri::new("media://model/denied/item").unwrap(),
    };
    assert_eq!(
        check(plane, &principal(), GatewayAction::ResourcesRead, &read),
        PolicyReasonCode::PolicyDeny
    );
}

#[test]
fn template_ownership_scope_requirements_filters_and_denials_stay_in_force() {
    let template = target("media://model/{id}");
    let mut actor = principal();
    assert_eq!(
        check(
            plane(),
            &actor,
            GatewayAction::CompletionComplete,
            &template
        ),
        PolicyReasonCode::PolicyAllow
    );
    actor.scopes.clear();
    assert_eq!(
        check(
            plane(),
            &actor,
            GatewayAction::CompletionComplete,
            &template
        ),
        PolicyReasonCode::MissingScope
    );
    assert_eq!(
        check(
            plane(),
            &principal(),
            GatewayAction::CompletionComplete,
            &target("foreign://model/{id}")
        ),
        PolicyReasonCode::UnknownResource
    );
    let mut plane = plane();
    plane.policies[0].rules[0].resource_schemes =
        BTreeSet::from([ResourceScheme::new("media").unwrap()]);
    let mut denial = plane.policies[0].rules[0].clone();
    denial.id = PolicyRuleId::new("deny-template").unwrap();
    denial.effect = PolicyEffect::Deny;
    plane.policies[0].rules.push(denial);
    assert_eq!(
        check(
            plane.clone(),
            &principal(),
            GatewayAction::CompletionComplete,
            &template
        ),
        PolicyReasonCode::PolicyDeny
    );
    plane.policies[0].rules.pop();
    plane.servers[0].resource_projection = ResourceProjectionMode::ServerOwned;
    plane.policies[0].rules[0].resource_schemes =
        BTreeSet::from([ResourceScheme::new("ui").unwrap()]);
    assert_eq!(
        check(
            plane,
            &principal(),
            GatewayAction::CompletionComplete,
            &template
        ),
        PolicyReasonCode::PolicyDeny
    );
}

#[test]
fn ui_ownership_and_action_target_types_are_independent_of_literal_template_syntax() {
    let mut plane = plane();
    plane.servers[0].resource_projection = ResourceProjectionMode::ServerOwned;
    plane.profiles[0].servers[0].resources = Exposure::All;
    plane.policies[0].rules[0]
        .resource_schemes
        .insert(ResourceScheme::new("ui").unwrap());
    assert_eq!(
        check(
            plane.clone(),
            &principal(),
            GatewayAction::ResourcesTemplatesList,
            &target("ui://media/{page}")
        ),
        PolicyReasonCode::PolicyAllow
    );
    assert_eq!(
        check(
            plane.clone(),
            &principal(),
            GatewayAction::ResourcesTemplatesList,
            &target("ui://other/{page}")
        ),
        PolicyReasonCode::UnknownResource
    );
    let concrete = PolicyTarget::Resource {
        server: ServerSlug::new("media").unwrap(),
        uri: ResourceUri::new("media://model/literal").unwrap(),
    };
    for action in [
        GatewayAction::CompletionComplete,
        GatewayAction::ResourcesTemplatesList,
    ] {
        assert_eq!(
            check(plane.clone(), &principal(), action, &concrete),
            PolicyReasonCode::PolicyDeny
        );
        assert_eq!(
            check(
                plane.clone(),
                &principal(),
                action,
                &target("media://model/literal")
            ),
            PolicyReasonCode::PolicyAllow
        );
    }
}

#[test]
fn resource_read_selection_matches_concrete_policy_for_each_rule_requirement() {
    use veoveo_policy::admit_resource_reads;
    let actor = principal();
    let uris = [
        "media://model/a",
        "media://model/a-tail",
        "media://model/a-tail-tail",
        "media://model/-tail",
        "media://model/",
        "media://other/a",
        "foreign://model/a",
    ];
    let exposures = [
        Exposure::All,
        Exposure::None,
        Exposure::Listed(vec![ResourceSelector::Scheme {
            scheme: "media".parse().unwrap(),
        }]),
        Exposure::Listed(vec![ResourceSelector::UriPrefix {
            prefix: ResourceUriPrefix::new("media://model/a").unwrap(),
        }]),
        Exposure::Listed(vec![ResourceSelector::Template {
            uri_template: ResourceUriTemplate::new("media://model/{id}-tail").unwrap(),
        }]),
        Exposure::Listed(vec![ResourceSelector::Template {
            uri_template: ResourceUriTemplate::new("media://model/{id}").unwrap(),
        }]),
    ];
    for exposure in exposures {
        for scenario in 0..13 {
            let mut plane = plane();
            plane.profiles[0].servers[0].resources = exposure.clone();
            let mut actor = actor.clone();
            let rule = &mut plane.policies[0].rules[0];
            match scenario {
                1 => actor.scopes.clear(),
                2 => {
                    rule.principal_ids.insert("another".parse().unwrap());
                }
                3 => {
                    rule.tenant_ids.insert("another".parse().unwrap());
                }
                4 => {
                    rule.groups.insert("another".parse().unwrap());
                }
                5 => {
                    rule.roles.insert("another".parse().unwrap());
                }
                6 => {
                    rule.required_scopes
                        .insert("another:scope".parse().unwrap());
                }
                7 => {
                    rule.required_data_labels.insert("another".parse().unwrap());
                }
                8 => {
                    rule.required_assurances
                        .insert(veoveo_mcp_contract::PrincipalAssurance::UsPerson);
                }
                9 => {
                    let mut deny = rule.clone();
                    deny.id = PolicyRuleId::new("deny-read").unwrap();
                    deny.effect = PolicyEffect::Deny;
                    plane.policies[0].rules.push(deny);
                }
                10 => {
                    actor.data_labels.insert("unknown".parse().unwrap());
                }
                11 => {
                    actor.tenant = Some("unknown".parse().unwrap());
                }
                12 => {
                    rule.resource_schemes = BTreeSet::from(["ui".parse().unwrap()]);
                }
                _ => {}
            }
            // Custom rules below use only declared references. Unknown caller
            // claims remain useful negative cases in a valid catalog.
            if scenario == 3 {
                plane.policies[0].rules[0].tenant_ids =
                    BTreeSet::from(["tenant-a".parse().unwrap()]);
                actor.tenant = None;
            }
            if scenario == 7 {
                plane.policies[0].rules[0].required_data_labels.clear();
                if let Some(label) = plane.data_labels.first() {
                    plane.policies[0].rules[0]
                        .required_data_labels
                        .insert(label.id.clone());
                }
            }
            if scenario == 12 {
                plane.servers[0].resource_projection = ResourceProjectionMode::ServerOwned;
            }
            if scenario == 6 {
                for client in &mut plane.oauth_clients {
                    client
                        .allowed_scopes
                        .insert("another:scope".parse().unwrap());
                }
            }
            let profile = plane.profiles[0].id.clone();
            let server = plane.servers[0].slug.clone();
            let catalog = PolicyCatalog::new(plane, catalog_fixture::registry()).unwrap();
            let admitted = admit_resource_reads(&catalog, &actor, &profile, &server);
            for text in uris {
                let uri = ResourceUri::new(text).unwrap();
                let ordinary = decide(
                    &catalog,
                    PolicyRequest {
                        principal: &actor,
                        profile: &profile,
                        action: GatewayAction::ResourcesRead.into(),
                        target: &PolicyTarget::Resource {
                            server: server.clone(),
                            uri: uri.clone(),
                        },
                        trace_id: &TraceId::new("read-selection-parity").unwrap(),
                    },
                );
                assert_eq!(
                    admitted
                        .as_ref()
                        .is_ok_and(|selection| selection.matches_uri(&uri)),
                    ordinary.effect == PolicyEffect::Allow,
                    "scenario {scenario}, URI {text}, decision {ordinary:?}"
                );
            }
        }
    }
}
