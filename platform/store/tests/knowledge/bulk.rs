//! Largest supported vectors and member replacement over the default remote wire.
use super::*;
use surrealdb::types::SurrealValue;

#[derive(SurrealValue)]
struct Count {
    total: u64,
}

#[tokio::test]
async fn maximum_member_vectors_fit_the_wire_and_failed_replacement_rolls_back() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("knowledge-bulk");
        let mut embedding = space("largest-member");
        embedding.dimension = EmbeddingDimension::new(8192).unwrap();
        let specification = GenerationSpec::new(
            embedding.clone(),
            "Find relevant passages",
            ChunkSettings::new("fixture-v1", 1000, 100).unwrap(),
            [(registration.descriptor.collection().clone(), registration.revision())].into(),
        ).unwrap();
        let lease = db.a.claim_knowledge_coordinator(&registration.tenant, Default::default())
            .await.unwrap().unwrap();
        db.a.register_knowledge_collection(&registration, None).await.unwrap();
        let generation = GenerationId::new();
        db.a.create_knowledge_generation(&lease,
&registration.tenant,
generation,
&specification,
&embedding_fixture::runtime(specification.space().clone()))
            .await.unwrap();
        let text = "Maximum-size vector replacement fixture";
        let observation = Observation::builder(
            registration.descriptor.collection().clone(),
            Revision::parse("large-member-one").unwrap(),
            source::content_digest(text), Utc::now(),
        ).access(AccessDescriptor {
            tenant: registration.tenant.clone(),
            work_context: "operations".parse().unwrap(),
            read_policy: source::ReadPolicy::Tenant {},
            owner: AccessSubject::Principal("author".parse().unwrap()),
            grants: vec![], data_labels: vec![], expires_at: None,
        }).build(&registration.descriptor).unwrap();
        let vector = EmbeddingVector::new(&embedding_fixture::runtime(embedding), vec![1.0 / 8192_f32.sqrt(); 8192]).unwrap();
        let chunk = IndexedChunk::from_range(text,
0..text.len(),
vector,
&specification,
&embedding_fixture::runtime(specification.space().clone())).unwrap();
        let member = |title| IndexedMember::new(
            &registration, &specification, ResourceUri::new("fixture://records/large-member").unwrap(),
            observation.clone(), text, MemberTitle::new(title).unwrap(), vec![chunk.clone(); 256],
        ).unwrap();
        let initial = member("Original bulk member");
        insert(&db.a, &lease, &registration, generation, &specification, &initial).await;
        complete(&db.a, &lease, &registration, generation).await.unwrap();
        db.a.activate_knowledge_generation(&lease,
&registration.tenant,
generation,
None,
&embedding_fixture::runtime(db.a.knowledge_generation(&registration.tenant, generation).await.unwrap().unwrap().space().clone()))
            .await.unwrap();
        let table = format!("knowledge_chunk_{}", generation.as_uuid().simple());
        let mut response = db.b.client().query(include_str!("../queries/knowledge/bulk/maximum_member_vectors_fit_the_wire_and_failed_replacement_rolls_back.surql"))
            .bind(("table", table.clone())).await.unwrap().check().unwrap();
        assert_eq!(response.take::<Option<Count>>(0).unwrap().unwrap().total, 256);
        let vectors: Vec<Vec<f32>> = response.take(1).unwrap();
        assert_eq!(vectors[0].len(), 8192);
        assert_eq!(vectors[0], chunk.vector().values());

        let provenance_before = provenance::publication_state(&db.b,generation).await;
        assert_eq!(provenance_before.0.len(),1);
        assert_eq!(provenance_before.1.len(),1);

        // Failure after earlier rows in the INSERT must preserve every old row.
        db.a.client().query(include_str!("../queries/knowledge/bulk/maximum_member_vectors_fit_the_wire_and_failed_replacement_rolls_back_2.surql"))
            .bind(("table", table.clone())).await.unwrap().check().unwrap();
        db.a.renew_knowledge_coordinator(&lease).await.unwrap();
        let replacement = member("Replacement bulk member");
        let ticket = db.a.begin_knowledge_member_read(&lease,
&registration,
generation,
&specification,
replacement.uri(),
&embedding_fixture::runtime(specification.space().clone())).await.unwrap();
        let error = db.a.replace_knowledge_member(&ticket, &replacement).await.unwrap_err();
        assert!(error.to_string().contains("fixture_replacement_rejected"),
            "replacement failed before the injected transactional rejection");
        let mut response = db.b.client().query(include_str!("../queries/knowledge/bulk/maximum_member_vectors_fit_the_wire_and_failed_replacement_rolls_back_3.surql"))
            .bind(("table", table.clone())).bind(("title", initial.title().as_str().to_owned())).await.unwrap().check().unwrap();
        assert_eq!(response.take::<Option<Count>>(0).unwrap().unwrap().total, 256);
        assert_eq!(provenance::publication_state(&db.b,generation).await, provenance_before, "failed INSERT must roll back receipt publication, retained producers and publication epoch");
        assert!(db.b.knowledge_candidates_page(&scope(&registration), generation, None, 100)
            .await.unwrap().is_empty(), "failed replacement must leave the reserved member stale");
    }).await.expect("bulk member qualification exceeded three minutes");
}
