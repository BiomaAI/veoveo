//! Synthetic profiles exercise storage fencing; they are never deployment qualification.
use super::*;
use surrealdb::types::RecordId;
use veoveo_platform_store::knowledge::SearchWindow;

fn batch(
    registration: &CollectionRegistration,
    spec: &GenerationSpec,
    runtime: &QualifiedEmbeddingRuntime,
) -> IndexedMember {
    let baseline = member(registration, spec, "visible", "operations", &[]);
    let text = baseline.chunks()[0].text();
    IndexedMember::new(
        registration,
        spec,
        baseline.uri().clone(),
        baseline.observation().clone(),
        text,
        baseline.title().clone(),
        vec![
            IndexedChunk::from_range(
                text,
                0..text.len(),
                EmbeddingVector::new(runtime, vec![1., 0., 0.]).unwrap(),
                spec,
                runtime,
            )
            .unwrap(),
        ],
    )
    .unwrap()
}

async fn receipts(
    store: &PlatformStore,
    generation: GenerationId,
) -> (Vec<RecordId>, Vec<RecordId>, i64) {
    let mut response = store
        .client()
        .query(include_str!(
            "../queries/knowledge/provenance/receipts.surql"
        ))
        .bind((
            "chunk_table",
            format!("knowledge_chunk_{}", generation.as_uuid().simple()),
        ))
        .bind((
            "generation",
            RecordId::new(
                "knowledge_generation",
                surrealdb::types::Uuid::from(generation.as_uuid()),
            ),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    (
        response.take(0).unwrap(),
        response.take(1).unwrap(),
        response.take::<Option<i64>>(2).unwrap().unwrap(),
    )
}

pub(super) async fn publication_state(
    store: &PlatformStore,
    generation: GenerationId,
) -> (Vec<RecordId>, Vec<RecordId>, i64) {
    let mut response = store
        .client()
        .query(include_str!(
            "../queries/knowledge/provenance/publication_state.surql"
        ))
        .bind((
            "generation",
            RecordId::new(
                "knowledge_generation",
                surrealdb::types::Uuid::from(generation.as_uuid()),
            ),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    (
        response.take(0).unwrap(),
        response.take::<Vec<RecordId>>(1).unwrap(),
        response.take::<Option<i64>>(2).unwrap().unwrap(),
    )
}

#[tokio::test]
async fn retained_producers_survive_revalidation_and_fence_new_profile_publication() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("knowledge-provenance");
        let spec = spec(&registration, "provenance");
        let a = embedding_fixture::runtime(spec.space().clone());
        let profile_b = embedding_fixture::profile(spec.space().clone(), 2);
        let b = QualifiedEmbeddingRuntime::new(
            profile_b.clone(),
            vec![a.profile().clone(), profile_b.clone()],
            vec![
                embedding_fixture::qualification(&profile_b, &profile_b),
                embedding_fixture::qualification(&profile_b, a.profile()),
            ],
        )
        .unwrap();
        let unknown = embedding_fixture::runtime({
            let mut space = spec.space().clone();
            space.precision = EmbeddingPrecision::Float16;
            space
        });
        let lease =
            db.a.claim_knowledge_coordinator(&registration.tenant, Default::default())
                .await
                .unwrap()
                .unwrap();
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let generation = GenerationId::new();
        db.a.create_knowledge_generation(&lease, &registration.tenant, generation, &spec, &a)
            .await
            .unwrap();
        let member_a = batch(&registration, &spec, &a);
        let ticket =
            db.a.begin_knowledge_member_read(
                &lease,
                &registration,
                generation,
                &spec,
                member_a.uri(),
                &a,
            )
            .await
            .unwrap();
        db.a.replace_knowledge_member(&ticket, &member_a)
            .await
            .unwrap();
        complete(&db.a, &lease, &registration, generation)
            .await
            .unwrap();
        db.a.activate_knowledge_generation(&lease, &registration.tenant, generation, None, &a)
            .await
            .unwrap();
        let before = receipts(&db.b, generation).await;
        assert_eq!(
            before.0,
            vec![RecordId::new(
                "knowledge_embedding_profile",
                a.profile().id().as_ref()
            )]
        );
        let mut forged = serde_json::to_value(a.profile()).unwrap();
        forged["contents"]["runtimeImage"] =
            serde_json::to_value(Sha256Digest::from_bytes([9; 32])).unwrap();
        assert!(
            db.a.client()
                .query(include_str!(
                    "../queries/knowledge/provenance/immutable_profile.surql"
                ))
                .bind((
                    "profile",
                    RecordId::new("knowledge_embedding_profile", a.profile().id().as_ref())
                ))
                .bind((
                    "document",
                    veoveo_platform_store::native_json_into_value(forged)
                ))
                .await
                .unwrap()
                .check()
                .is_err()
        );
        let admission_a =
            db.b.admit_knowledge_embeddings(&registration.tenant, generation, &a)
                .await
                .unwrap();
        assert!(
            db.b.admit_knowledge_embeddings(&registration.tenant, generation, &unknown)
                .await
                .is_err()
        );
        // Rewriting with the same producer changes data/receipt identity, not the producer-set fence.
        let again =
            db.a.begin_knowledge_member_read(
                &lease,
                &registration,
                generation,
                &spec,
                member_a.uri(),
                &a,
            )
            .await
            .unwrap();
        db.a.replace_knowledge_member(&again, &member_a)
            .await
            .unwrap();
        let repeated = receipts(&db.b, generation).await;
        assert_eq!(repeated.0, before.0);
        assert_eq!(repeated.2, before.2);
        assert_eq!(repeated.1.len(), before.1.len() + 1);
        let query = EmbeddingText::new("Fixture").unwrap();
        let vector_a = EmbeddingVector::new(&a, vec![1., 0., 0.]).unwrap();
        let scope = scope(&registration);
        db.b.search_knowledge(
            &scope,
            generation,
            &query,
            &vector_a,
            &admission_a,
            &BTreeSet::new(),
            1,
            SearchWindow::Initial,
        )
        .await
        .unwrap();
        // Conditional same-space reuse preserves the original producer and batch receipts.
        let unqualified_b = QualifiedEmbeddingRuntime::new(
            profile_b.clone(),
            vec![a.profile().clone(), profile_b.clone()],
            vec![embedding_fixture::qualification(&profile_b, &profile_b)],
        )
        .unwrap();
        assert!(
            db.a.begin_knowledge_member_read(
                &lease,
                &registration,
                generation,
                &spec,
                member_a.uri(),
                &unqualified_b
            )
            .await
            .is_err()
        );
        assert_eq!(receipts(&db.b, generation).await, repeated);
        let reuse =
            db.a.begin_knowledge_member_read(
                &lease,
                &registration,
                generation,
                &spec,
                member_a.uri(),
                &b,
            )
            .await
            .unwrap();
        let mut conditional = serde_json::to_value(member_a.observation()).unwrap();
        conditional["notModified"] = true.into();
        let conditional: Observation = serde_json::from_value(conditional).unwrap();
        db.a.revalidate_knowledge_member(&reuse, &conditional)
            .await
            .unwrap();
        assert_eq!(receipts(&db.b, generation).await, repeated);
        let old_admission =
            db.b.admit_knowledge_embeddings(&registration.tenant, generation, &b)
                .await
                .unwrap();
        let member_b = batch(&registration, &spec, &b);
        let ticket_b =
            db.a.begin_knowledge_member_read(
                &lease,
                &registration,
                generation,
                &spec,
                member_b.uri(),
                &b,
            )
            .await
            .unwrap();
        db.a.replace_knowledge_member(&ticket_b, &member_b)
            .await
            .unwrap();
        let produced = receipts(&db.b, generation).await;
        assert_eq!(produced.2, repeated.2 + 1);
        assert_eq!(produced.1.len(), repeated.1.len() + 1);
        assert_eq!(
            produced.0,
            vec![RecordId::new(
                "knowledge_embedding_profile",
                b.profile().id().as_ref()
            )]
        );
        assert!(
            db.b.admit_knowledge_embeddings(&registration.tenant, generation, &a)
                .await
                .is_err(),
            "every retained producer requires directional qualification"
        );
        let vector_b = EmbeddingVector::new(&b, vec![1., 0., 0.]).unwrap();
        for window in [
            SearchWindow::Initial,
            SearchWindow::Expanded,
            SearchWindow::Deep,
            SearchWindow::Maximum,
        ] {
            assert!(
                matches!(
                    db.b.search_knowledge(
                        &scope,
                        generation,
                        &query,
                        &vector_b,
                        &old_admission,
                        &BTreeSet::new(),
                        1,
                        window
                    )
                    .await
                    .unwrap_err(),
                    veoveo_platform_store::StoreError::KnowledgeEmbeddingAdmissionChanged
                ),
                "{window:?}"
            );
        }
        let fresh =
            db.b.admit_knowledge_embeddings(&registration.tenant, generation, &b)
                .await
                .unwrap();
        let page =
            db.b.search_knowledge(
                &scope,
                generation,
                &query,
                &vector_b,
                &fresh,
                &BTreeSet::new(),
                1,
                SearchWindow::Initial,
            )
            .await
            .unwrap();
        assert_eq!(page.results.len(), 1);
        assert_eq!(page.results[0].candidate.uri, *member_b.uri());
        // Failed old-runtime admission must leave all receipt and producer facts untouched.
        assert_eq!(receipts(&db.b, generation).await, produced);
        // Publication changes between source reservation and replacement/activation are fenced.
        let next = GenerationId::new();
        db.a.create_knowledge_generation(&lease, &registration.tenant, next, &spec, &b)
            .await
            .unwrap();
        let late_a = member(&registration, &spec, "late-a", "operations", &[]);
        let late_ticket =
            db.a.begin_knowledge_member_read(&lease, &registration, next, &spec, late_a.uri(), &a)
                .await
                .unwrap();
        let b_ticket = db
            .a
            .begin_knowledge_member_read(&lease, &registration, next, &spec, member_b.uri(), &b)
            .await
            .unwrap();
        db.a.replace_knowledge_member(&b_ticket, &member_b)
            .await
            .unwrap();
        let next_receipts = receipts(&db.b, next).await;
        assert!(
            db.a.replace_knowledge_member(&late_ticket, &late_a)
                .await
                .is_err()
        );
        assert_eq!(receipts(&db.b, next).await, next_receipts);
        assert!(
            db.a.activate_knowledge_generation(
                &lease,
                &registration.tenant,
                next,
                Some(generation),
                &a
            )
            .await
            .is_err()
        );
        assert_eq!(
            db.b.active_knowledge_generation(&registration.tenant)
                .await
                .unwrap(),
            Some(generation)
        );
        // A new Store connection loads the original producer identities from durable rows.
        let restarted_reader = PlatformStore::connect(db.b.config().clone()).await.unwrap();
        assert_eq!(receipts(&restarted_reader, generation).await, produced);
        restarted_reader
            .admit_knowledge_embeddings(&registration.tenant, generation, &b)
            .await
            .unwrap();
    })
    .await
    .expect("producer provenance qualification exceeded 180 seconds");
}
