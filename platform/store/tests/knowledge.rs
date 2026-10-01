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

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

fn space(revision: &str) -> EmbeddingSpace {
    EmbeddingSpace {
        model: EmbeddingModelId::new("synthetic-fixture").unwrap(),
        revision: EmbeddingModelRevision::new(revision).unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        runtime_image: Sha256Digest::from_bytes([7; 32]),
    }
}
fn registration(tenant: &str) -> CollectionRegistration {
    CollectionRegistration {
        tenant: TenantId::new(tenant).unwrap(),
        descriptor: CollectionDescriptor::new(
            CollectionId::try_from("fixture.records".to_owned()).unwrap(),
            EntityKind::new("record").unwrap(),
            ResourceTemplateUri::new("fixture://records{?cursor}").unwrap(),
            Freshness::max_age(30),
            ChangeSignal::Listen,
            AccessModel::WorkContext,
            IndexingMode::Content,
        )
        .unwrap(),
        approval: CollectionApproval::Index,
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
    let text = "Fixture knowledge content";
    let observation = Observation::builder(
        registration.descriptor.collection().clone(),
        Revision::new("r1").unwrap(),
        source::content_digest(text),
        Utc::now(),
    )
    .access(AccessDescriptor {
        read_policy,
        tenant: registration.tenant.clone(),
        work_context: WorkContextId::new(context).unwrap(),
        owner: AccessSubject::Principal(PrincipalId::new("author").unwrap()),
        grants,
        data_labels: labels
            .iter()
            .map(|v| DataLabelId::new(*v).unwrap())
            .collect(),
    })
    .build(&registration.descriptor)
    .unwrap();
    let vector = EmbeddingVector::new(spec.space().clone(), vec![1.0, 0.0, 0.0]).unwrap();
    let chunk = IndexedChunk::from_range(text, 0..text.len(), vector, spec).unwrap();
    let uri = veoveo_types::ResourceUriBuilder::new("fixture://records")
        .unwrap()
        .segment(veoveo_types::UriSegment::new(id).unwrap())
        .build()
        .unwrap();
    IndexedMember::new(registration, spec, uri, observation, text, vec![chunk]).unwrap()
}
async fn insert(
    store: &PlatformStore,
    registration: &CollectionRegistration,
    gen_id: GenerationId,
    spec: &GenerationSpec,
    member: &IndexedMember,
) {
    let ticket = store
        .begin_knowledge_member_read(registration, gen_id, spec, member.uri())
        .await
        .unwrap();
    store
        .replace_knowledge_member(&ticket, member)
        .await
        .unwrap();
}
fn scope(registration: &CollectionRegistration) -> CandidateScope {
    CandidateScope {
        profile: "operations".parse().unwrap(),
        active_work_context: "operations".parse().unwrap(),
        tenant: registration.tenant.clone(),
        collections: BTreeSet::from([registration.descriptor.collection().clone()]),
        work_contexts: BTreeSet::from([WorkContextId::new("operations").unwrap()]),
        subjects: BTreeSet::from([AccessSubject::Principal(
            PrincipalId::new("reader").unwrap(),
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
async fn source_read_policies_preserve_subject_context_and_profile_restrictions() {
    tokio::time::timeout(Duration::from_secs(120), qualify_read_policies())
        .await
        .expect("source policy qualification exceeded 120 seconds");
}

async fn qualify_read_policies() {
    use source::ReadPolicy;
    let db = fixture::TestDb::new().await;
    let registration = registration("knowledge-policies");
    let specification = spec(&registration, "policies");
    let generation = GenerationId::new();
    db.a.register_knowledge_collection(&registration, None)
        .await
        .unwrap();
    db.a.create_knowledge_generation(&registration.tenant, generation, &specification)
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
        insert(&db.a, &registration, generation, &specification, &member).await;
    }
    db.a.complete_knowledge_collection(
        &registration.tenant,
        generation,
        registration.descriptor.collection(),
        &registration.revision(),
    )
    .await
    .unwrap();
    db.a.activate_knowledge_generation(&registration.tenant, generation, None)
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
    db.a.client().query(format!(
        "UPDATE {table} SET observation = {{malformed: true}} WHERE admission.tenant_read = false AND admission.context_read = false;"
    )).await.unwrap().check().unwrap();
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
    db.a.create_knowledge_generation(tenant, first, &first_spec)
        .await
        .unwrap();
    assert!(
        db.b.activate_knowledge_generation(tenant, first, None)
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
        insert(&db.a, &registration, first, &first_spec, m).await;
    }
    db.a.complete_knowledge_collection(tenant, first, collection, &registration.revision())
        .await
        .unwrap();
    db.a.activate_knowledge_generation(tenant, first, None)
        .await
        .unwrap();
    assert_eq!(
        db.b.active_knowledge_generation(tenant).await.unwrap(),
        Some(first)
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
    denied_scope.tenant = TenantId::new("foreign").unwrap();
    assert!(
        db.b.knowledge_candidates_page(&denied_scope, first, None, 100)
            .await
            .unwrap()
            .is_empty()
    );

    // Malformed hidden observations must be excluded before typed decoding and LIMIT.
    let table = format!("knowledge_chunk_{}", first.as_uuid().simple());
    db.a.client()
        .query(format!(
            "UPDATE {table} SET observation = {{malformed: true}} WHERE admission.labels CONTAINS 'secret';"
        ))
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
        db.a.begin_knowledge_member_read(&registration, first, &first_spec, visible.uri())
            .await
            .unwrap();
    let current =
        db.b.begin_knowledge_member_read(&registration, first, &first_spec, visible.uri())
            .await
            .unwrap();
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
    assert!(
        db.a.complete_knowledge_collection(tenant, first, collection, &registration.revision())
            .await
            .is_err()
    );
    db.a.replace_knowledge_member(&current, &visible)
        .await
        .unwrap();
    assert_eq!(
        db.b.knowledge_candidates_page(&scope, first, None, 1)
            .await
            .unwrap()
            .len(),
        1
    );

    let deletion =
        db.b.begin_knowledge_member_read(&registration, first, &first_spec, visible.uri())
            .await
            .unwrap();
    db.a.confirm_knowledge_member_deleted(&deletion)
        .await
        .unwrap();
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
    db.a.complete_knowledge_collection(tenant, first, collection, &registration.revision())
        .await
        .unwrap();

    let second = GenerationId::new();
    let second_spec = spec(&registration, "space-two");
    db.a.create_knowledge_generation(tenant, second, &second_spec)
        .await
        .unwrap();
    let ticket =
        db.a.begin_knowledge_member_read(&registration, second, &second_spec, visible.uri())
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
    db.a.complete_knowledge_collection(tenant, second, collection, &registration.revision())
        .await
        .unwrap();
    assert!(
        db.b.activate_knowledge_generation(tenant, second, None)
            .await
            .is_err()
    );
    db.b.activate_knowledge_generation(tenant, second, Some(first))
        .await
        .unwrap();
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
    revoked.approval = CollectionApproval::CatalogOnly;
    db.a.register_knowledge_collection(&revoked, Some(&registration.revision()))
        .await
        .unwrap();
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
            &registration,
            second,
            &second_spec,
            &ResourceUri::new("fixture://records/visible").unwrap()
        )
        .await
        .is_err()
    );
    assert!(
        db.a.remove_knowledge_generation(tenant, second)
            .await
            .is_err(),
        "active generations are fenced from cleanup"
    );
    db.a.remove_knowledge_generation(tenant, first)
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
            .query("SELECT VALUE id FROM knowledge_member WHERE generation = $generation;")
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
