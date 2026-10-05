//! Persistence qualification uses synthetic normalized vectors, never inference or GPU evidence.
use chrono::Utc;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use veoveo_embedding_contract::*;
use veoveo_knowledge_contract::*;
use veoveo_mcp_knowledge_extension::{
    self as source, AccessDescriptor, AccessModel, ChangeSignal, CollectionDescriptor,
    CollectionId, EntityKind, Freshness, IndexingMode, Observation, Revision,
};
use veoveo_platform_store::{PlatformStore, knowledge::CandidateScope};
use veoveo_types::{
    AccessSubject, DataLabelId, PrincipalId, ResourceTemplateUri, ResourceUri, Sha256Digest,
    TenantId, WorkContextId,
};

#[path = "knowledge/bulk.rs"]
mod bulk;
#[path = "knowledge/catalog.rs"]
mod catalog;
#[path = "knowledge/coordinator.rs"]
mod coordinator;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

fn space(revision: &str) -> EmbeddingSpace {
    EmbeddingSpace {
        model: EmbeddingModelId::parse("synthetic-fixture").unwrap(),
        revision: EmbeddingModelRevision::parse(revision).unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        runtime_image: Sha256Digest::from_bytes([7; 32]),
    }
}
fn registration(tenant: &str) -> CollectionRegistration {
    CollectionRegistration {
        source_contract_revision: 3,
        tenant: TenantId::parse(tenant).unwrap(),
        descriptor: CollectionDescriptor::new(
            CollectionId::try_from("fixture.records".to_owned()).unwrap(),
            EntityKind::parse("record").unwrap(),
            ResourceTemplateUri::new("fixture://records{?cursor}").unwrap(),
            Freshness::max_age(30),
            ChangeSignal::Listen,
            AccessModel::WorkContext,
            IndexingMode::Content,
        )
        .unwrap(),
        approval: KnowledgeCollectionApproval {
            collection: "fixture.records".parse().unwrap(),
            mode: CollectionApproval::Index,
            stewards: ["stewards".parse().unwrap()].into(),
            authoritative_for: Default::default(),
            data_labels: ["secret".parse().unwrap(), "restricted".parse().unwrap()].into(),
        },
        control_revision: Sha256Digest::from_bytes([1; 32]),
    }
}
fn spec(registration: &CollectionRegistration, revision: &str) -> GenerationSpec {
    GenerationSpec::new(
        space(revision),
        "Find relevant passages",
        ChunkSettings::new("fixture-v1", 1000, 100).unwrap(),
        BTreeMap::from([(
            registration.descriptor.collection().clone(),
            registration.revision(),
        )]),
    )
    .unwrap()
}
fn member(
    registration: &CollectionRegistration,
    spec: &GenerationSpec,
    id: &str,
    context: &str,
    labels: &[&str],
) -> IndexedMember {
    member_with_policy(
        registration,
        spec,
        id,
        context,
        labels,
        source::ReadPolicy::WorkContext {},
        vec![],
    )
}
fn member_with_policy(
    registration: &CollectionRegistration,
    spec: &GenerationSpec,
    id: &str,
    context: &str,
    labels: &[&str],
    read_policy: source::ReadPolicy,
    grants: Vec<AccessSubject>,
) -> IndexedMember {
    member_with_access(
        registration,
        spec,
        id,
        AccessDescriptor {
            expires_at: None,
            read_policy,
            tenant: registration.tenant.clone(),
            work_context: WorkContextId::parse(context).unwrap(),
            owner: AccessSubject::Principal(PrincipalId::parse("author").unwrap()),
            grants: grants.into_iter().map(source::ReadGrant::new).collect(),
            data_labels: labels
                .iter()
                .map(|v| DataLabelId::parse(*v).unwrap())
                .collect(),
        },
    )
}

fn member_with_access(
    registration: &CollectionRegistration,
    spec: &GenerationSpec,
    id: &str,
    access: AccessDescriptor,
) -> IndexedMember {
    let text = "Fixture knowledge content";
    let observation = Observation::builder(
        registration.descriptor.collection().clone(),
        Revision::parse("r1").unwrap(),
        source::content_digest(text),
        Utc::now(),
    )
    .access(access)
    .build(&registration.descriptor)
    .unwrap();
    let vector = EmbeddingVector::new(spec.space().clone(), vec![1.0, 0.0, 0.0]).unwrap();
    let chunk = IndexedChunk::from_range(text, 0..text.len(), vector, spec).unwrap();
    let uri = veoveo_types::ResourceUriBuilder::new("fixture://records")
        .unwrap()
        .segment(veoveo_types::UriSegment::new(id).unwrap())
        .build()
        .unwrap();
    IndexedMember::new(
        registration,
        spec,
        uri,
        observation,
        text,
        MemberTitle::new("Fixture knowledge").unwrap(),
        vec![chunk],
    )
    .unwrap()
}
async fn insert(
    store: &PlatformStore,
    lease: &veoveo_platform_store::knowledge::CoordinatorLease,
    registration: &CollectionRegistration,
    gen_id: GenerationId,
    spec: &GenerationSpec,
    member: &IndexedMember,
) {
    let ticket = store
        .begin_knowledge_member_read(lease, registration, gen_id, spec, member.uri())
        .await
        .unwrap();
    store
        .replace_knowledge_member(&ticket, member)
        .await
        .unwrap();
}
fn scope(registration: &CollectionRegistration) -> CandidateScope {
    CandidateScope {
        scopes: BTreeSet::new(),
        profile: "operations".parse().unwrap(),
        active_work_context: "operations".parse().unwrap(),
        tenant: registration.tenant.clone(),
        collections: BTreeMap::from([(registration.descriptor.collection().clone(), selection())]),
        work_contexts: BTreeSet::from([WorkContextId::parse("operations").unwrap()]),
        subjects: BTreeSet::from([AccessSubject::Principal(
            PrincipalId::parse("reader").unwrap(),
        )]),
        clearance: BTreeSet::new(),
    }
}

#[tokio::test]
async fn generations_fence_reads_and_apply_current_approval_and_access_in_sql() {
    tokio::time::timeout(Duration::from_secs(120), qualify())
        .await
        .expect("knowledge qualification exceeded 120 seconds");
}

#[tokio::test]
async fn source_scopes_and_selected_context_membership_cannot_be_bypassed_by_ownership() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let mut registration = registration("knowledge-members");
        let lease =
            db.a.claim_knowledge_coordinator(
                &registration.tenant,
                veoveo_platform_store::knowledge::CoordinatorId::new(),
            )
            .await
            .unwrap()
            .unwrap();
        registration.descriptor = registration
            .descriptor
            .with_required_scopes(["fixture:read".parse().unwrap()]);
        let specification = spec(&registration, "members");
        let generation = GenerationId::new();
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        db.a.create_knowledge_generation(&lease, &registration.tenant, generation, &specification)
            .await
            .unwrap();
        let member = member_with_policy(
            &registration,
            &specification,
            "member",
            "operations",
            &[],
            source::ReadPolicy::SelectedWorkContextMembers {},
            vec![AccessSubject::Principal("reader".parse().unwrap())],
        );
        insert(
            &db.a,
            &lease,
            &registration,
            generation,
            &specification,
            &member,
        )
        .await;
        complete(&db.a, &lease, &registration, generation)
            .await
            .unwrap();
        db.a.activate_knowledge_generation(&lease, &registration.tenant, generation, None)
            .await
            .unwrap();
        let mut caller = scope(&registration);
        assert_candidates(&db.b, &caller, generation, &[]).await;
        caller.scopes.insert("fixture:read".parse().unwrap());
        assert_candidates(&db.b, &caller, generation, &["member"]).await;
        caller.active_work_context = "other".parse().unwrap();
        assert_candidates(&db.b, &caller, generation, &[]).await;
        caller
            .subjects
            .insert(AccessSubject::Principal("author".parse().unwrap()));
        assert_candidates(&db.b, &caller, generation, &[]).await;
        caller.active_work_context = "operations".parse().unwrap();
        caller.work_contexts.clear();
        assert_candidates(&db.b, &caller, generation, &[]).await;
        caller.work_contexts.insert("operations".parse().unwrap());
        let table = format!("knowledge_chunk_{}", generation.as_uuid().simple());
        db.a.client()
            .query(include_str!("queries/knowledge/source_scopes_and_selected_context_membership_cannot_be_bypassed_by_ownership.surql"))
            .bind(("table", table.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        caller.scopes.clear();
        assert_candidates(&db.b, &caller, generation, &[]).await;
        caller.scopes.insert("fixture:read".parse().unwrap());
        assert!(
            db.b.knowledge_candidates_page(&caller, generation, None, 1)
                .await
                .is_err()
        );
    })
    .await
    .expect("knowledge membership qualification exceeded 120 seconds");
}

#[tokio::test]
async fn source_read_policies_preserve_subject_context_and_profile_restrictions() {
    tokio::time::timeout(Duration::from_secs(120), qualify_read_policies())
        .await
        .expect("source policy qualification exceeded 120 seconds");
}

#[tokio::test]
async fn selected_context_and_expiry_are_admitted_in_sql_before_decoding() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("knowledge-expiry");
        let lease = db.a.claim_knowledge_coordinator(&registration.tenant, veoveo_platform_store::knowledge::CoordinatorId::new()).await.unwrap().unwrap();
        let specification = spec(&registration, "expiry");
        let generation = GenerationId::new();
        db.a.register_knowledge_collection(&registration, None).await.unwrap();
        db.a.create_knowledge_generation(&lease, &registration.tenant, generation, &specification).await.unwrap();
        let past = Utc::now() - chrono::TimeDelta::minutes(1);
        let deadline = Utc::now() + chrono::TimeDelta::seconds(4);
        let reader = AccessSubject::Principal("reader".parse().unwrap());
        let group = AccessSubject::Group("reviewers".parse().unwrap());
        for (id, policy, expires_at, grants) in [
            ("aaa-expired-tenant", source::ReadPolicy::Tenant {}, Some(past), vec![]),
            ("bbb-expired-context", source::ReadPolicy::SelectedWorkContext {}, Some(past), vec![]),
            ("ccc-expired-grant", source::ReadPolicy::Subjects {}, None, vec![source::ReadGrant::new(reader.clone()).until(past)]),
            ("ddd-expired-group", source::ReadPolicy::Subjects {}, None, vec![source::ReadGrant::new(group.clone()).until(past)]),
            ("eee-selected", source::ReadPolicy::SelectedWorkContext {}, None, vec![]),
            ("fff-live-grant", source::ReadPolicy::SelectedWorkContext {}, None, vec![source::ReadGrant::new(reader).until(deadline)]),
            ("zzz-visible", source::ReadPolicy::Tenant {}, None, vec![]),
        ] {
            let member = member_with_access(&registration, &specification, id, AccessDescriptor {
                tenant: registration.tenant.clone(), work_context: "operations".parse().unwrap(),
                read_policy: policy, owner: AccessSubject::Principal("author".parse().unwrap()),
                grants, data_labels: vec![], expires_at,
            });
            insert(&db.a, &lease, &registration, generation, &specification, &member).await;
        }
        complete(&db.a, &lease, &registration, generation).await.unwrap();
        db.a.activate_knowledge_generation(&lease, &registration.tenant, generation, None).await.unwrap();
        let mut reader = scope(&registration);
        reader.subjects.insert(group);
        assert_candidates(&db.b, &reader, generation, &["eee-selected", "fff-live-grant", "zzz-visible"]).await;
        reader.active_work_context = "other".parse().unwrap();
        assert_candidates(&db.b, &reader, generation, &["fff-live-grant", "zzz-visible"]).await;
        reader.work_contexts.clear();
        assert_candidates(&db.b, &reader, generation, &["fff-live-grant", "zzz-visible"]).await;
        let mut owner = reader.clone();
        owner.subjects = BTreeSet::from([AccessSubject::Principal("author".parse().unwrap())]);
        assert_candidates(&db.b, &owner, generation,
            &["ccc-expired-grant", "ddd-expired-group", "eee-selected", "fff-live-grant", "zzz-visible"]).await;
        // No reindex or mutation occurs when time alone ends a grant.
        tokio::time::sleep((deadline - Utc::now()).to_std().unwrap_or_default() + Duration::from_millis(10)).await;
        assert_candidates(&db.b, &reader, generation, &["zzz-visible"]).await;
        let table = format!("knowledge_chunk_{}", generation.as_uuid().simple());
        db.a.client().query(include_str!("queries/knowledge/selected_context_and_expiry_are_admitted_in_sql_before_decoding.surql"))
            .bind(("table", table.clone()))
            .await.unwrap().check().unwrap();
        let page = db.b.knowledge_candidates_page(&reader, generation, None, 1).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].uri.as_str(), "fixture://records/zzz-visible");
        let mut active = reader;
        active.active_work_context = "operations".parse().unwrap();
        active.work_contexts.insert("operations".parse().unwrap());
        assert!(db.b.knowledge_candidates_page(&active, generation, None, 1).await.is_err(),
            "admitted corrupt observations must fail explicitly");
    }).await.expect("expiry admission qualification exceeded 120 seconds");
}

async fn qualify_read_policies() {
    use source::ReadPolicy;
    let db = fixture::TestDb::new().await;
    let registration = registration("knowledge-policies");
    let lease =
        db.a.claim_knowledge_coordinator(
            &registration.tenant,
            veoveo_platform_store::knowledge::CoordinatorId::new(),
        )
        .await
        .unwrap()
        .unwrap();
    let specification = spec(&registration, "policies");
    let generation = GenerationId::new();
    db.a.register_knowledge_collection(&registration, None)
        .await
        .unwrap();
    db.a.create_knowledge_generation(&lease, &registration.tenant, generation, &specification)
        .await
        .unwrap();
    let records = [
        ("aaa-owner", ReadPolicy::Subjects {}, vec![]),
        (
            "bbb-task",
            ReadPolicy::SubjectsInContext {
                profile: Some("operations".parse().unwrap()),
            },
            vec![],
        ),
        (
            "ccc-context-subjects",
            ReadPolicy::SubjectsInContext { profile: None },
            vec![],
        ),
        (
            "ddd-grant",
            ReadPolicy::Subjects {},
            vec![AccessSubject::Group("reviewers".parse().unwrap())],
        ),
        ("eee-shared", ReadPolicy::WorkContext {}, vec![]),
        ("zzz-tenant", ReadPolicy::Tenant {}, vec![]),
    ];
    for (id, policy, grants) in records {
        let member = member_with_policy(
            &registration,
            &specification,
            id,
            "operations",
            &[],
            policy,
            grants,
        );
        insert(
            &db.a,
            &lease,
            &registration,
            generation,
            &specification,
            &member,
        )
        .await;
    }
    complete(&db.a, &lease, &registration, generation)
        .await
        .unwrap();
    db.a.activate_knowledge_generation(&lease, &registration.tenant, generation, None)
        .await
        .unwrap();

    let reader = scope(&registration);
    assert_candidates(&db.b, &reader, generation, &["eee-shared", "zzz-tenant"]).await;
    let mut owner = reader.clone();
    owner.subjects = BTreeSet::from([AccessSubject::Principal("author".parse().unwrap())]);
    assert_candidates(
        &db.b,
        &owner,
        generation,
        &[
            "aaa-owner",
            "bbb-task",
            "ccc-context-subjects",
            "ddd-grant",
            "eee-shared",
            "zzz-tenant",
        ],
    )
    .await;

    let mut wrong_profile = owner.clone();
    wrong_profile.profile = "another".parse().unwrap();
    assert_candidates(
        &db.b,
        &wrong_profile,
        generation,
        &[
            "aaa-owner",
            "ccc-context-subjects",
            "ddd-grant",
            "eee-shared",
            "zzz-tenant",
        ],
    )
    .await;
    // Membership in operations does not replace the Task's selected-context requirement.
    let mut wrong_context = owner.clone();
    wrong_context.active_work_context = "another".parse().unwrap();
    assert_candidates(
        &db.b,
        &wrong_context,
        generation,
        &["aaa-owner", "ddd-grant", "eee-shared", "zzz-tenant"],
    )
    .await;
    let mut grantee = reader.clone();
    grantee.work_contexts.clear();
    grantee
        .subjects
        .insert(AccessSubject::Group("reviewers".parse().unwrap()));
    assert_candidates(&db.b, &grantee, generation, &["ddd-grant", "zzz-tenant"]).await;
    let mut no_membership = reader.clone();
    no_membership.work_contexts.clear();
    assert_candidates(&db.b, &no_membership, generation, &["zzz-tenant"]).await;
    let mut unexposed = owner.clone();
    unexposed.collections.clear();
    assert_candidates(&db.b, &unexposed, generation, &[]).await;
    let mut foreign = owner.clone();
    foreign.tenant = "another".parse().unwrap();
    assert_candidates(&db.b, &foreign, generation, &[]).await;

    // These earlier-sorting private rows are malformed. A reader must still get
    // its full page; decoding and then discarding a denied row cannot pass.
    let table = format!("knowledge_chunk_{}", generation.as_uuid().simple());
    db.a.client()
        .query(include_str!(
            "queries/knowledge/qualify_read_policies.surql"
        ))
        .bind(("table", table.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let page =
        db.b.knowledge_candidates_page(&reader, generation, None, 1)
            .await
            .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].uri.as_str(), "fixture://records/eee-shared");
    let next =
        db.b.knowledge_candidates_page(&reader, generation, Some(&page[0].cursor()), 1)
            .await
            .unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].uri.as_str(), "fixture://records/zzz-tenant");
    assert!(
        db.b.knowledge_candidates_page(&owner, generation, None, 1)
            .await
            .is_err()
    );
}

async fn assert_candidates(
    store: &PlatformStore,
    scope: &CandidateScope,
    generation: GenerationId,
    expected: &[&str],
) {
    let actual = store
        .knowledge_candidates_page(scope, generation, None, 100)
        .await
        .unwrap();
    let uris: Vec<_> = actual.iter().map(|row| row.uri.as_str()).collect();
    let expected: Vec<_> = expected
        .iter()
        .map(|id| {
            veoveo_types::ResourceUriBuilder::new("fixture://records")
                .unwrap()
                .segment(veoveo_types::UriSegment::new(*id).unwrap())
                .build()
                .unwrap()
        })
        .collect();
    assert_eq!(
        uris,
        expected.iter().map(|uri| uri.as_str()).collect::<Vec<_>>()
    );
}

async fn qualify() {
    let db = fixture::TestDb::new().await;
    let registration = registration("knowledge-a");
    let lease =
        db.a.claim_knowledge_coordinator(
            &registration.tenant,
            veoveo_platform_store::knowledge::CoordinatorId::new(),
        )
        .await
        .unwrap()
        .unwrap();
    let tenant = &registration.tenant;
    let collection = registration.descriptor.collection();
    db.a.register_knowledge_collection(&registration, None)
        .await
        .unwrap();
    assert_eq!(
        db.b.knowledge_collection(tenant, collection).await.unwrap(),
        Some(registration.clone())
    );
    let first = GenerationId::new();
    let first_spec = spec(&registration, "space-one");
    db.a.create_knowledge_generation(&lease, tenant, first, &first_spec)
        .await
        .unwrap();
    assert!(
        db.b.activate_knowledge_generation(&lease, tenant, first, None)
            .await
            .is_err()
    );
    let visible = member(&registration, &first_spec, "visible", "operations", &[]);
    let hidden = member(
        &registration,
        &first_spec,
        "aaa-secret",
        "operations",
        &["secret"],
    );
    let other_context = member(&registration, &first_spec, "aaa-other", "other", &[]);
    for m in [&hidden, &other_context, &visible] {
        insert(&db.a, &lease, &registration, first, &first_spec, m).await;
    }
    complete(&db.a, &lease, &registration, first).await.unwrap();
    assert!(
        !db.b
            .knowledge_member_observed(&registration, visible.uri())
            .await
            .unwrap(),
        "building members cannot authorize subscriptions"
    );
    db.a.activate_knowledge_generation(&lease, tenant, first, None)
        .await
        .unwrap();
    assert_eq!(
        db.b.active_knowledge_generation(tenant).await.unwrap(),
        Some(first)
    );
    assert!(
        db.b.knowledge_member_observed(&registration, visible.uri())
            .await
            .unwrap()
    );
    let mut foreign_registration = registration.clone();
    foreign_registration.tenant = "foreign".parse().unwrap();
    assert!(
        !db.b
            .knowledge_member_observed(&foreign_registration, visible.uri())
            .await
            .unwrap()
    );
    let scope = scope(&registration);
    let rows =
        db.b.knowledge_candidates_page(&scope, first, None, 1)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].uri, *visible.uri());
    assert!(
        db.b.knowledge_candidates_page(&scope, first, Some(&rows[0].cursor()), 1)
            .await
            .unwrap()
            .is_empty()
    );
    let mut denied_scope = scope.clone();
    denied_scope.work_contexts.clear();
    assert!(
        db.b.knowledge_candidates_page(&denied_scope, first, None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    denied_scope.tenant = TenantId::parse("foreign").unwrap();
    assert!(
        db.b.knowledge_candidates_page(&denied_scope, first, None, 100)
            .await
            .unwrap()
            .is_empty()
    );

    // Malformed hidden observations must be excluded before typed decoding and LIMIT.
    let table = format!("knowledge_chunk_{}", first.as_uuid().simple());
    db.a.client()
        .query(include_str!("queries/knowledge/qualify.surql"))
        .bind(("table", table.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        db.b.knowledge_candidates_page(&scope, first, None, 1)
            .await
            .unwrap()
            .len(),
        1
    );

    let late =
        db.a.begin_knowledge_member_read(&lease, &registration, first, &first_spec, visible.uri())
            .await
            .unwrap();
    let current =
        db.b.begin_knowledge_member_read(&lease, &registration, first, &first_spec, visible.uri())
            .await
            .unwrap();
    assert!(
        !db.a
            .knowledge_member_observed(&registration, visible.uri())
            .await
            .unwrap(),
        "in-flight refresh fences subscription admission"
    );
    assert!(
        db.a.replace_knowledge_member(&late, &visible)
            .await
            .is_err()
    );
    assert!(
        db.b.knowledge_candidates_page(&scope, first, None, 1)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(complete(&db.a, &lease, &registration, first).await.is_err());
    db.a.replace_knowledge_member(&current, &visible)
        .await
        .unwrap();
    assert!(
        db.b.knowledge_member_observed(&registration, visible.uri())
            .await
            .unwrap()
    );
    assert_eq!(
        db.b.knowledge_candidates_page(&scope, first, None, 1)
            .await
            .unwrap()
            .len(),
        1
    );

    let deletion =
        db.b.begin_knowledge_member_read(&lease, &registration, first, &first_spec, visible.uri())
            .await
            .unwrap();
    db.a.confirm_knowledge_member_deleted(&deletion)
        .await
        .unwrap();
    assert!(
        !db.b
            .knowledge_member_observed(&registration, visible.uri())
            .await
            .unwrap()
    );
    assert!(
        db.a.replace_knowledge_member(&deletion, &visible)
            .await
            .is_err()
    );
    assert!(
        db.b.knowledge_candidates_page(&scope, first, None, 1)
            .await
            .unwrap()
            .is_empty()
    );
    complete(&db.a, &lease, &registration, first).await.unwrap();

    let second = GenerationId::new();
    let second_spec = spec(&registration, "space-two");
    db.a.create_knowledge_generation(&lease, tenant, second, &second_spec)
        .await
        .unwrap();
    let ticket = db
        .a
        .begin_knowledge_member_read(&lease, &registration, second, &second_spec, visible.uri())
        .await
        .unwrap();
    assert!(
        db.a.replace_knowledge_member(&ticket, &visible)
            .await
            .is_err(),
        "different embedding spaces cannot mix"
    );
    let updated = member(&registration, &second_spec, "visible", "operations", &[]);
    db.a.replace_knowledge_member(&ticket, &updated)
        .await
        .unwrap();
    complete(&db.a, &lease, &registration, second)
        .await
        .unwrap();
    assert!(
        db.b.activate_knowledge_generation(&lease, tenant, second, None)
            .await
            .is_err()
    );
    db.b.activate_knowledge_generation(&lease, tenant, second, Some(first))
        .await
        .unwrap();
    assert!(
        db.a.knowledge_member_observed(&registration, visible.uri())
            .await
            .unwrap()
    );
    assert!(
        db.a.knowledge_candidates_page(&scope, second, Some(&rows[0].cursor()), 1)
            .await
            .is_err(),
        "a page cursor cannot cross index generations"
    );
    assert!(
        db.a.knowledge_candidates_page(&scope, first, None, 1)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.a.knowledge_candidates_page(&scope, second, None, 1)
            .await
            .unwrap()
            .len(),
        1
    );

    let mut revoked = registration.clone();
    revoked.approval.mode = CollectionApproval::CatalogOnly;
    db.a.register_knowledge_collection(&revoked, Some(&registration.revision()))
        .await
        .unwrap();
    assert!(
        !db.b
            .knowledge_member_observed(&registration, visible.uri())
            .await
            .unwrap()
    );
    assert!(
        !db.b
            .knowledge_member_observed(&revoked, visible.uri())
            .await
            .unwrap()
    );
    assert!(
        db.b.register_knowledge_collection(&registration, Some(&registration.revision()))
            .await
            .is_err()
    );
    assert!(
        db.b.knowledge_candidates_page(&scope, second, None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        db.b.begin_knowledge_member_read(
            &lease,
            &registration,
            second,
            &second_spec,
            &ResourceUri::new("fixture://records/visible").unwrap()
        )
        .await
        .is_err()
    );
    assert!(
        db.a.remove_knowledge_generation(&lease, tenant, second)
            .await
            .is_err(),
        "active generations are fenced from cleanup"
    );
    db.a.remove_knowledge_generation(&lease, tenant, first)
        .await
        .unwrap();
    assert!(
        db.b.knowledge_generation(tenant, first)
            .await
            .unwrap()
            .is_none()
    );
    let mut response =
        db.b.client()
            .query(include_str!("queries/knowledge/qualify_2.surql"))
            .bind((
                "generation",
                veoveo_platform_store::RecordId::new(
                    "knowledge_generation",
                    surrealdb::types::Uuid::from(first.as_uuid()),
                ),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
    let members: Vec<veoveo_platform_store::RecordId> = response.take(0).unwrap();
    assert!(
        members.is_empty(),
        "native references cascade reclaimed generation members"
    );
}

fn selection() -> veoveo_types::ResourceSelection {
    veoveo_types::ResourceSelection {
        scheme: "fixture".parse().unwrap(),
        selectors: vec![veoveo_types::ResourceSelector::Scheme {
            scheme: "fixture".parse().unwrap(),
        }],
    }
}

#[tokio::test]
async fn lexical_resource_selection_matches_policy_before_pagination() {
    use veoveo_types::{ResourceSelector, ResourceUriPrefix, ResourceUriTemplate};
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("knowledge-selectors");
        let lease =
            db.a.claim_knowledge_coordinator(
                &registration.tenant,
                veoveo_platform_store::knowledge::CoordinatorId::new(),
            )
            .await
            .unwrap()
            .unwrap();
        let spec = spec(&registration, "selection");
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let generation = GenerationId::new();
        db.a.create_knowledge_generation(&lease, &registration.tenant, generation, &spec)
            .await
            .unwrap();
        let mut uris = Vec::new();
        for id in [
            "a",
            "ab",
            "-tail",
            "a-tail",
            "a-tail-tail",
            "a-x-tail",
            "a-x-tail-extra",
            "café",
            "a%20b",
            "a?kind=end",
            "a' OR true",
            "a/b",
        ] {
            let member = member(&registration, &spec, id, "operations", &[]);
            uris.push(member.uri().clone());
            insert(&db.a, &lease, &registration, generation, &spec, &member).await;
        }
        complete(&db.a, &lease, &registration, generation)
            .await
            .unwrap();
        db.a.activate_knowledge_generation(&lease, &registration.tenant, generation, None)
            .await
            .unwrap();
        uris.sort();
        let selectors = [
            vec![],
            vec![ResourceSelector::Scheme {
                scheme: "fixture".parse().unwrap(),
            }],
            vec![ResourceSelector::Scheme {
                scheme: "foreign".parse().unwrap(),
            }],
            vec![ResourceSelector::UriPrefix {
                prefix: ResourceUriPrefix::new("fixture://records/a").unwrap(),
            }],
            vec![ResourceSelector::UriPrefix {
                prefix: ResourceUriPrefix::new("fixture://records/a%2F").unwrap(),
            }],
            vec![ResourceSelector::Template {
                uri_template: ResourceUriTemplate::new("fixture://records/{id}").unwrap(),
            }],
            vec![ResourceSelector::Template {
                uri_template: ResourceUriTemplate::new("fixture://records/{id}-tail").unwrap(),
            }],
            vec![ResourceSelector::Template {
                uri_template: ResourceUriTemplate::new("fixture://records/{id}-x-{suffix}")
                    .unwrap(),
            }],
            vec![ResourceSelector::Template {
                uri_template: ResourceUriTemplate::new("fixture://records/{id}-x-{suffix}-extra")
                    .unwrap(),
            }],
            vec![ResourceSelector::Template {
                uri_template: ResourceUriTemplate::new("fixture://records/{id}%20{rest}").unwrap(),
            }],
        ];
        for selectors in selectors {
            let mut scope = scope(&registration);
            scope
                .collections
                .get_mut(registration.descriptor.collection())
                .unwrap()
                .selectors = selectors;
            let selection = &scope.collections[registration.descriptor.collection()];
            let expected: Vec<_> = uris
                .iter()
                .filter(|uri| selection.matches_uri(uri))
                .cloned()
                .collect();
            let mut found = Vec::new();
            let mut cursor = None;
            loop {
                let page =
                    db.b.knowledge_candidates_page(&scope, generation, cursor.as_ref(), 2)
                        .await
                        .unwrap();
                let remaining = expected.len() - found.len();
                assert_eq!(
                    page.len(),
                    remaining.min(2),
                    "denied rows cannot consume LIMIT"
                );
                if page.is_empty() {
                    break;
                }
                cursor = page.last().map(|row| row.cursor());
                found.extend(page.into_iter().map(|row| row.uri));
            }
            assert_eq!(found, expected, "{selection:?}");
        }
    })
    .await
    .expect("resource selection qualification exceeded 120 seconds");
}

#[tokio::test]
async fn catalog_admits_before_source_paging_and_exact_record_decoding() {
    tokio::time::timeout(Duration::from_secs(180), async {
        use veoveo_platform_store::knowledge::CatalogSelection;
        let db = fixture::TestDb::new().await;
        let tenant = TenantId::parse("catalog-native").unwrap();
        let mut approvals = BTreeMap::new();
        let mut expected = Vec::new();
        for n in 0..105 {
            let mut r = registration(tenant.as_str());
            let owner = format!("source-{n:03}");
            r.descriptor = CollectionDescriptor::new(
                format!("{owner}.records").parse().unwrap(), "record".parse().unwrap(),
                ResourceTemplateUri::new(format!("{owner}://records{{?cursor}}")).unwrap(),
                Freshness::max_age(30), ChangeSignal::Listen, AccessModel::WorkContext, IndexingMode::Content,
            ).unwrap().with_required_scopes(["catalog:read".parse().unwrap()]);
            r.approval.collection = r.descriptor.collection().clone();
            db.a.register_knowledge_collection(&r, None).await.unwrap();
            approvals.insert(r.approval.collection.clone(), r.approval.clone());
            expected.push(r);
        }
        let scopes = ["catalog:read".parse().unwrap()].into();
        use veoveo_platform_store::knowledge::CatalogCompletion;
        let values = db.b.complete_knowledge_catalog(&tenant, &approvals, &scopes, CatalogCompletion::Source, "source-").await.unwrap();
        assert_eq!(values.len(), 101, "completion uses one lookahead beyond the protocol bound");
        assert_eq!(values[0].to_string(), "source-000");
        assert_eq!(db.b.complete_knowledge_catalog(&tenant, &approvals, &scopes, CatalogCompletion::Collection, "source-10").await.unwrap().len(), 5);
        assert!(db.b.complete_knowledge_catalog(&tenant, &approvals, &BTreeSet::new(), CatalogCompletion::Source, "").await.unwrap().is_empty());
        assert!(db.b.complete_knowledge_catalog(&"wrong-tenant".parse().unwrap(), &approvals, &scopes, CatalogCompletion::Source, "").await.unwrap().is_empty());
        let first = db.b.readable_knowledge_sources(&tenant, &approvals, &scopes, None).await.unwrap();
        assert_eq!(first.len(), 101, "one SQL page plus lookahead");
        assert_eq!(first[0].server.as_str(), "source-000");
        let next = db.b.readable_knowledge_sources(&tenant, &approvals, &scopes, Some(&first[99].server)).await.unwrap();
        assert_eq!(next.len(), 5); assert_eq!(next[0].server.as_str(), "source-100");
        let selected = expected[104].descriptor.collection();
        // Corrupt another approved document: an exact read cannot decode it.
        db.a.client().query(include_str!("queries/knowledge/catalog_admits_before_source_paging_and_exact_record_decoding.surql")).await.unwrap().check().unwrap();
        let exact = db.b.readable_knowledge_collections(&tenant, &approvals, &scopes, CatalogSelection::Collection(selected)).await.unwrap();
        assert_eq!(exact, vec![expected[104].clone()]);
        let source = db.b.readable_knowledge_collections(&tenant, &approvals, &scopes, CatalogSelection::Source(selected.server())).await.unwrap();
        assert_eq!(source, exact);
        // SQL scope admission and approval removal must exclude the corrupt row.
        assert!(db.b.readable_knowledge_sources(&tenant, &approvals, &BTreeSet::new(), None).await.unwrap().is_empty());
        approvals.remove(expected[0].descriptor.collection());
        assert_eq!(db.b.complete_knowledge_catalog(&tenant, &approvals, &scopes, CatalogCompletion::Source, "").await.unwrap()[0].to_string(), "source-001");
        assert_eq!(db.b.readable_knowledge_sources(&tenant, &approvals, &scopes, None).await.unwrap()[0].server.as_str(), "source-001");
        approvals.get_mut(selected).unwrap().data_labels.clear();
        assert!(db.b.readable_knowledge_collections(&tenant, &approvals, &scopes, CatalogSelection::Collection(selected)).await.unwrap().is_empty(), "changed approval cannot reveal the old registration");
    }).await.expect("catalog SQL qualification exceeded 180 seconds");
}

#[tokio::test]
async fn subscription_root_selection_applies_current_approval_scopes_and_tenant_in_sql() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let mut allowed = registration("root-native");
        allowed.descriptor = allowed.descriptor.with_required_scopes(["fixture:read".parse().unwrap()]);
        db.a.register_knowledge_collection(&allowed, None).await.unwrap();
        let root = source::enumeration_uri(&allowed.descriptor, None).unwrap();
        let scopes = ["fixture:read".parse().unwrap()].into();
        let mut approvals = BTreeMap::from([(allowed.approval.collection.clone(), allowed.approval.clone())]);
        assert_eq!(db.b.knowledge_collection_at_root(&allowed.tenant, &root, &approvals, &scopes).await.unwrap(), Some(allowed.clone()));
        assert!(db.b.knowledge_collection_at_root(&allowed.tenant, &root, &approvals, &BTreeSet::new()).await.unwrap().is_none());
        assert!(db.b.knowledge_collection_at_root(&"another".parse().unwrap(), &root, &approvals, &scopes).await.unwrap().is_none());
        assert!(db.b.knowledge_collection_at_root(&allowed.tenant, &ResourceUri::new("fixture://records?cursor=next").unwrap(), &approvals, &scopes).await.unwrap().is_none());
        let mut changed = approvals.clone();
        changed.get_mut(&allowed.approval.collection).unwrap().data_labels.clear();
        assert!(db.b.knowledge_collection_at_root(&allowed.tenant, &root, &changed, &scopes).await.unwrap().is_none());

        let mut alias = allowed.clone();
        alias.descriptor = CollectionDescriptor::new("fixture.alias".parse().unwrap(), "record".parse().unwrap(),
            ResourceTemplateUri::new("fixture://records{?cursor}").unwrap(), Freshness::max_age(30),
            ChangeSignal::Listen, AccessModel::WorkContext, IndexingMode::Content).unwrap()
            .with_required_scopes(["fixture:read".parse().unwrap()]);
        alias.approval.collection = alias.descriptor.collection().clone();
        alias.approval.mode = CollectionApproval::CatalogOnly;
        db.a.register_knowledge_collection(&alias, None).await.unwrap();
        approvals.insert(alias.approval.collection.clone(), alias.approval.clone());
        assert_eq!(db.b.knowledge_collection_at_root(&allowed.tenant, &root, &approvals, &scopes).await.unwrap(), Some(allowed.clone()), "catalog-only registrations cannot authorize resource observation");
        let prior = alias.revision();
        alias.approval.mode = CollectionApproval::Index;
        db.a.register_knowledge_collection(&alias, Some(&prior)).await.unwrap();
        approvals.insert(alias.approval.collection.clone(), alias.approval.clone());
        assert!(db.b.knowledge_collection_at_root(&allowed.tenant, &root, &approvals, &scopes).await.is_err(), "ambiguous approved roots cannot select an arbitrary collection");
        // Poison the denied document: decoding before SQL admission would fail.
        db.a.client().query(include_str!("queries/knowledge/subscription_root_selection_applies_current_approval_scopes_and_tenant_in_sql.surql")).await.unwrap().check().unwrap();
        approvals.remove(&alias.approval.collection);
        assert_eq!(db.b.knowledge_collection_at_root(&allowed.tenant, &root, &approvals, &scopes).await.unwrap(), Some(allowed.clone()));
        approvals.insert(alias.approval.collection.clone(), alias.approval.clone());
        assert!(db.b.knowledge_collection_at_root(&allowed.tenant, &root, &approvals, &BTreeSet::new()).await.unwrap().is_none());
    }).await.expect("root SQL qualification exceeded 120 seconds");
}

async fn complete(
    store: &PlatformStore,
    lease: &veoveo_platform_store::knowledge::CoordinatorLease,
    registration: &CollectionRegistration,
    generation: GenerationId,
) -> Result<(), veoveo_platform_store::StoreError> {
    let ticket = store
        .knowledge_collection_sync(lease, registration, generation)
        .await?;
    store.complete_knowledge_collection(&ticket).await
}

#[tokio::test]
async fn controlled_observation_storage_rejects_unknown_and_missing_fields_atomically() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("observation-shape");
        let lease = db.a.claim_knowledge_coordinator(&registration.tenant, veoveo_platform_store::knowledge::CoordinatorId::new()).await.unwrap().unwrap();
        let specification = spec(&registration, "shape");
        let generation = GenerationId::new();
        db.a.register_knowledge_collection(&registration, None).await.unwrap();
        db.a.create_knowledge_generation(&lease, &registration.tenant, generation, &specification).await.unwrap();
        let member = member_with_access(&registration, &specification, "visible", AccessDescriptor {
            tenant: registration.tenant.clone(), work_context: "operations".parse().unwrap(),
            read_policy: source::ReadPolicy::Tenant {}, owner: AccessSubject::Principal("author".parse().unwrap()),
            grants: vec![], data_labels: vec![], expires_at: None,
        });
        insert(&db.a, &lease, &registration, generation, &specification, &member).await;
        complete(&db.a, &lease, &registration, generation).await.unwrap();
        db.a.activate_knowledge_generation(&lease, &registration.tenant, generation, None).await.unwrap();
        let original = serde_json::to_value(member.observation()).unwrap();
        for which in 0..8 {
            let mut invalid = original.clone();
            match which {
                0 => { invalid.as_object_mut().unwrap().remove("collection"); }
                1 => { invalid["unknown"] = true.into(); }
                2 => { invalid["access"]["unknown"] = true.into(); }
                3 => { invalid["access"]["owner"]["unknown"] = true.into(); }
                4 => { invalid["access"]["readPolicy"]["unknown"] = true.into(); }
                5 => { invalid["access"]["grants"] = serde_json::json!([{ "subject": {"kind":"principal","id":"author"}, "unknown":true }]); }
                6 => { invalid["modifiedBy"] = serde_json::json!({"kind":"principal","id":"author","unknown":true}); }
                7 => { invalid["external"] = serde_json::json!({"system":"crm","nativeId":"entry","unknown":true}); }
                _ => unreachable!(),
            }
            assert!(db.a.client().query(include_str!("queries/knowledge/observation_write.surql"))
                .bind(("table", format!("knowledge_chunk_{}", generation.as_uuid().simple())))
                .bind(("generation", veoveo_platform_store::RecordId::new("knowledge_generation", surrealdb::types::Uuid::from(generation.as_uuid()))))
                .bind(("uri", member.uri().to_string()))
                .bind(("observation", veoveo_platform_store::native_json_into_value(invalid)))
                .await.unwrap().check().is_err(), "closed Observation admitted mutation {which}");
            let rows = db.b.knowledge_candidates_page(&scope(&registration), generation, None, 100).await.unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(&rows[0].observation, member.observation());
        }
    }).await.expect("Observation shape qualification exceeded 120 seconds");
}

#[tokio::test]
async fn generation_requirements_admit_only_closed_native_collection_rows() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("knowledge-requirement-shape");
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let lease =
            db.a.claim_knowledge_coordinator(
                &registration.tenant,
                veoveo_platform_store::knowledge::CoordinatorId::new(),
            )
            .await
            .unwrap()
            .unwrap();
        let generation = GenerationId::new();
        db.a.create_knowledge_generation(
            &lease,
            &registration.tenant,
            generation,
            &spec(&registration, "requirements"),
        )
        .await
        .unwrap();
        use surrealdb::types::{Array, Object, SurrealValue, Value};
        let collection = veoveo_platform_store::RecordId::new(
            "knowledge_collection",
            Array::from(vec![
                registration.tenant.to_string(),
                registration.descriptor.collection().to_string(),
            ]),
        );
        let mut admitted = Object::new();
        admitted.insert("record", collection.into_value());
        admitted.insert("revision", registration.revision().to_string().into_value());
        for (field, replacement) in [
            ("record", None),
            ("revision", None),
            ("record", Some("not-a-record".into_value())),
            (
                "record",
                Some(veoveo_platform_store::RecordId::new("task", "foreign").into_value()),
            ),
            ("revision", Some(42_i64.into_value())),
            ("extra", Some(true.into_value())),
        ] {
            let mut invalid = admitted.clone();
            if let Some(value) = replacement {
                invalid.insert(field, value);
            } else {
                invalid.remove(field);
            }
            let result =
                db.a.client()
                    .query(include_str!(
                        "queries/knowledge/generation_requirement_shape.surql"
                    ))
                    .bind((
                        "generation",
                        veoveo_platform_store::RecordId::new(
                            "knowledge_generation",
                            surrealdb::types::Uuid::from(generation.as_uuid()),
                        ),
                    ))
                    .bind((
                        "probe",
                        veoveo_platform_store::RecordId::new(
                            "knowledge_generation",
                            surrealdb::types::Uuid::from(GenerationId::new().as_uuid()),
                        ),
                    ))
                    .bind(("requirements", vec![Value::Object(invalid)]))
                    .await
                    .unwrap()
                    .check();
            assert!(
                result.is_err(),
                "invalid requirement field {field} admitted"
            );
        }
        assert!(
            db.b.knowledge_generation(&registration.tenant, generation)
                .await
                .unwrap()
                .is_some()
        );
    })
    .await
    .expect("requirement shape qualification exceeded 120 seconds");
}
