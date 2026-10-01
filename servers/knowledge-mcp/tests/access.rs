use chrono::{Duration, Utc};
use std::collections::BTreeSet;
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
        collections: BTreeSet::from([descriptor.collection().clone()]),
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
        caller.allows(&descriptor, &observation, now)
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
