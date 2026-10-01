use chrono::{Duration, Utc};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_knowledge_mcp::access::SearchCaller;
use veoveo_mcp_contract::access::{GroupMembership, GroupRole};
use veoveo_mcp_knowledge_extension::*;
use veoveo_types::*;

#[test]
fn final_access_uses_source_policy_group_grants_and_record_deadlines() {
    let now = Utc::now();
    let descriptor = CollectionDescriptor::new(
        "fixture.records".parse().unwrap(),
        "record".parse().unwrap(),
        ResourceTemplateUri::new("fixture://records{?cursor}").unwrap(),
        Freshness::max_age(30),
        ChangeSignal::Listen,
        AccessModel::WorkContext,
        IndexingMode::Content,
    )
    .unwrap();
    let mut caller = SearchCaller {
        principal: "reader".parse().unwrap(),
        tenant: "tenant".parse().unwrap(),
        profile: "operations".parse().unwrap(),
        active_work_context: "operations".parse().unwrap(),
        collections: BTreeMap::from([(descriptor.collection().clone(), selection())]),
        work_contexts: BTreeSet::new(),
        memberships: BTreeSet::from([GroupMembership {
            group: "readers".parse().unwrap(),
            role: GroupRole::Read,
        }]),
        scopes: BTreeSet::new(),
        clearance: BTreeSet::new(),
    };
    let mut access = AccessDescriptor {
        tenant: caller.tenant.clone(),
        work_context: caller.active_work_context.clone(),
        read_policy: ReadPolicy::Subjects {},
        owner: AccessSubject::Principal("owner".parse().unwrap()),
        grants: vec![
            ReadGrant::new(AccessSubject::Group("readers".parse().unwrap()))
                .until(now + Duration::seconds(30)),
        ],
        data_labels: vec![],
        expires_at: None,
    };
    let allows = |caller: &SearchCaller, access: &AccessDescriptor| {
        let observation = Observation::builder(
            descriptor.collection().clone(),
            Revision::new("1").unwrap(),
            content_digest("text"),
            now,
        )
        .access(access.clone())
        .build(&descriptor)
        .unwrap();
        caller.allows(
            &descriptor,
            &ResourceUri::new("fixture://record/member").unwrap(),
            &observation,
            now,
        )
    };
    assert!(
        allows(&caller, &access),
        "a current group read grant is sufficient"
    );
    access.grants[0].expires_at = Some(now);
    assert!(
        !allows(&caller, &access),
        "an expired group grant grants nothing"
    );
    caller.work_contexts.insert(access.work_context.clone());
    assert!(
        !allows(&caller, &access),
        "recorded context membership cannot widen subjects-only policy"
    );
    access.read_policy = ReadPolicy::WorkContext {};
    assert!(allows(&caller, &access));
    access.read_policy = ReadPolicy::SubjectsInContext {
        profile: Some(caller.profile.clone()),
    };
    caller.principal = "owner".parse().unwrap();
    assert!(allows(&caller, &access));
    caller.profile = "elsewhere".parse().unwrap();
    assert!(!allows(&caller, &access));
    caller.profile = "operations".parse().unwrap();
    access.read_policy = ReadPolicy::SelectedWorkContextMembers {};
    caller.work_contexts.clear();
    assert!(
        !allows(&caller, &access),
        "ownership cannot replace required membership"
    );
    access.read_policy = ReadPolicy::SelectedWorkContext {};
    caller.active_work_context = "elsewhere".parse().unwrap();
    assert!(
        allows(&caller, &access),
        "this policy independently admits the owner"
    );
    for policy in [
        ReadPolicy::Tenant {},
        ReadPolicy::Subjects {},
        ReadPolicy::WorkContext {},
        ReadPolicy::SelectedWorkContext {},
    ] {
        access.read_policy = policy;
        access.data_labels = vec!["secret".parse().unwrap()];
        assert!(
            !allows(&caller, &access),
            "labels constrain every grant and sharing path"
        );
        access.data_labels.clear();
        access.expires_at = Some(now);
        assert!(
            !allows(&caller, &access),
            "record expiry constrains every read path"
        );
        access.expires_at = None;
        access.tenant = "foreign".parse().unwrap();
        assert!(!allows(&caller, &access), "grants cannot cross tenants");
        access.tenant = caller.tenant.clone();
    }
}

fn selection() -> veoveo_types::ResourceSelection {
    veoveo_types::ResourceSelection {
        scheme: "fixture".parse().unwrap(),
        selectors: vec![veoveo_types::ResourceSelector::Scheme {
            scheme: "fixture".parse().unwrap(),
        }],
    }
}

#[test]
fn authenticated_policy_resolves_source_selection_and_current_context_membership() {
    use veoveo_mcp_contract::{
        Exposure, GatewayControlPlane, Principal, PrincipalKind, WorkContextMembershipRule,
    };
    use veoveo_policy::PolicyCatalog;
    let mut plane: GatewayControlPlane =
        serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap();
    let principal = Principal {
        id: "knowledge-reader".parse().unwrap(),
        kind: PrincipalKind::User,
        issuer: "https://idp.example.com".parse().unwrap(),
        subject: "reader".parse().unwrap(),
        tenant: Some("tenant-a".parse().unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::from(["operator:use".parse().unwrap()]),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: None,
    };
    plane.profiles[0].servers[0].resources = Exposure::Listed(vec![ResourceSelector::UriPrefix {
        prefix: ResourceUriPrefix::new("media://model/public/").unwrap(),
    }]);
    let context = &plane.work_contexts[0];
    let authority = InvocationAuthority {
        work_context: context.id.clone(),
        tenant: context.tenant.clone(),
        policy_revision: context.policy_revision.clone(),
        membership: WorkContextMembershipLevel::Contributor,
        output_policy: context.output_policy.clone(),
        provenance: InvocationProvenance::Direct {
            initiator: principal.id.clone(),
        },
    };
    let mut foreign = context.clone();
    foreign.id = "foreign-context".parse().unwrap();
    foreign.tenant = "tenant-b".parse().unwrap();
    foreign.memberships = vec![WorkContextMembershipRule {
        level: WorkContextMembershipLevel::Owner,
        principals: BTreeSet::from([principal.id.clone()]),
        groups: BTreeSet::new(),
        roles: BTreeSet::new(),
        oauth_clients: BTreeSet::new(),
    }];
    plane.tenants.push(veoveo_mcp_contract::TenantDefinition {
        id: foreign.tenant.clone(),
        title: Some("Foreign tenant".into()),
        description: None,
        metadata: serde_json::Value::Null,
    });
    plane.work_contexts.push(foreign);
    let descriptor = CollectionDescriptor::new(
        "media.models".parse().unwrap(),
        "model".parse().unwrap(),
        ResourceTemplateUri::new("media://models{?cursor}").unwrap(),
        Freshness::max_age(30),
        ChangeSignal::Revalidate,
        AccessModel::Profile,
        IndexingMode::Content,
    )
    .unwrap();
    let catalog = PolicyCatalog::new(plane.clone()).unwrap();
    let profile = plane.profiles[0].id.clone();
    let client = "operator-service".parse().unwrap();
    let resolve = |catalog: &PolicyCatalog| {
        SearchCaller::from_policy(
            catalog,
            &principal,
            &profile,
            &client,
            &authority,
            [&descriptor],
        )
    };
    let reader = resolve(&catalog).unwrap();
    assert!(reader.work_contexts.contains(&authority.work_context));
    assert!(
        !reader
            .work_contexts
            .contains(&WorkContextId::new("foreign-context").unwrap())
    );
    let selected = &reader.collections[descriptor.collection()];
    assert!(selected.matches_uri(&ResourceUri::new("media://model/public/a").unwrap()));
    assert!(!selected.matches_uri(&ResourceUri::new("media://model/private/a").unwrap()));
    plane.profiles[0].servers[0].resources = Exposure::None;
    assert!(
        resolve(&PolicyCatalog::new(plane.clone()).unwrap())
            .unwrap()
            .collections
            .is_empty()
    );
    plane.work_contexts[0].memberships = vec![WorkContextMembershipRule {
        level: WorkContextMembershipLevel::Viewer,
        principals: BTreeSet::from(["someone-else".parse().unwrap()]),
        groups: BTreeSet::new(),
        roles: BTreeSet::new(),
        oauth_clients: BTreeSet::new(),
    }];
    assert!(
        resolve(&PolicyCatalog::new(plane).unwrap()).is_err(),
        "revoked active-context membership fails closed"
    );
}
