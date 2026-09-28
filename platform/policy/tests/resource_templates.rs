use std::collections::BTreeSet;
use veoveo_mcp_contract::{
    Exposure, GatewayAction, GatewayControlPlane, PolicyEffect, PolicyReasonCode, PolicyRuleId,
    PolicyTarget, Principal, PrincipalKind, ResourceProjectionMode, ResourceSelector,
    ResourceUriPrefix, ResourceUriTemplate, ServerSlug, TokenIssuer, TokenSubject, TraceId,
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
    let catalog = PolicyCatalog::new(plane).unwrap();
    decide(
        &catalog,
        PolicyRequest {
            principal: actor,
            profile: &profile,
            action,
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
