use super::*;
use veoveo_platform_store::knowledge::CoordinatorId;

#[tokio::test]
async fn takeover_and_source_epochs_fence_late_members_and_late_coverage() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("knowledge-coordinator");
        let tenant = &registration.tenant;
        let first_owner = CoordinatorId::new();
        let lease =
            db.a.claim_knowledge_coordinator(tenant, first_owner)
                .await
                .unwrap()
                .unwrap();
        assert!(
            db.b.claim_knowledge_coordinator(tenant, CoordinatorId::new())
                .await
                .unwrap()
                .is_none()
        );
        let specification = spec(&registration, "coordinator");
        let generation = GenerationId::new();
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        db.a.create_knowledge_generation(&lease,
tenant,
generation,
&specification,
&embedding_fixture::runtime(specification.space().clone()))
            .await
            .unwrap();
        let visible = member(&registration, &specification, "visible", "operations", &[]);
        insert(
            &db.a,
            &lease,
            &registration,
            generation,
            &specification,
            &visible,
        )
        .await;
        let late_coverage =
            db.a.knowledge_collection_sync(&lease, &registration, generation)
                .await
                .unwrap();
        db.a.complete_knowledge_collection(&late_coverage)
            .await
            .unwrap();
        db.a.activate_knowledge_generation(&lease,
tenant,
generation,
None,
&embedding_fixture::runtime(db.a.knowledge_generation(tenant, generation).await.unwrap().unwrap().space().clone()))
            .await
            .unwrap();
        let caller = scope(&registration);
        assert_candidates(&db.b, &caller, generation, &["visible"]).await;
        let late_read =
            db.a.begin_knowledge_member_read(&lease,
&registration,
generation,
&specification,
visible.uri(),
&embedding_fixture::runtime(specification.space().clone()))
            .await
            .unwrap();
        let sync =
            db.b.invalidate_knowledge_collection(&lease, &registration, generation)
                .await
                .unwrap();
        assert!(
            db.a.replace_knowledge_member(&late_read, &visible)
                .await
                .is_err()
        );
        assert!(
            db.a.complete_knowledge_collection(&late_coverage)
                .await
                .is_err()
        );
        insert(
            &db.a,
            &lease,
            &registration,
            generation,
            &specification,
            &visible,
        )
        .await;
        db.a.complete_knowledge_collection(&sync).await.unwrap();
        assert_candidates(&db.b, &caller, generation, &["visible"]).await;

        // Expiry is exercised in the isolated database without a 30-second sleep.
        let late_read =
            db.a.begin_knowledge_member_read(&lease,
&registration,
generation,
&specification,
visible.uri(),
&embedding_fixture::runtime(specification.space().clone()))
            .await
            .unwrap();
        db.a.client()
            .query(include_str!("../queries/knowledge/coordinator/takeover_and_source_epochs_fence_late_members_and_late_coverage.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(db.a.renew_knowledge_coordinator(&lease).await.is_err());
        assert_candidates(&db.b, &caller, generation, &[]).await;
        let successor =
            db.b.claim_knowledge_coordinator(tenant, CoordinatorId::new())
                .await
                .unwrap()
                .unwrap();
        assert!(
            db.a.replace_knowledge_member(&late_read, &visible)
                .await
                .is_err()
        );
        assert!(db.a.complete_knowledge_collection(&sync).await.is_err());
        assert!(
            db.a.remove_knowledge_generation(&lease, tenant, generation)
                .await
                .is_err()
        );
        // The old process cannot release or publish through the successor.
        db.a.release_knowledge_coordinator(&lease).await.unwrap();
        db.b.renew_knowledge_coordinator(&successor).await.unwrap();
        let recovered =
            db.b.invalidate_knowledge_collection(&successor, &registration, generation)
                .await
                .unwrap();
        insert(
            &db.b,
            &successor,
            &registration,
            generation,
            &specification,
            &visible,
        )
        .await;
        db.b.complete_knowledge_collection(&recovered)
            .await
            .unwrap();
        assert_candidates(&db.a, &caller, generation, &["visible"]).await;
        db.b.release_knowledge_coordinator(&successor)
            .await
            .unwrap();
        assert_candidates(&db.a, &caller, generation, &[]).await;
    })
    .await
    .expect("coordinator takeover qualification exceeded 120 seconds");
}

#[tokio::test]
async fn source_loss_and_freshness_exclude_malformed_cached_rows_before_decode() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("knowledge-freshness");
        let lease = db.a.claim_knowledge_coordinator(&registration.tenant, CoordinatorId::new()).await.unwrap().unwrap();
        let specification = spec(&registration, "freshness");
        let generation = GenerationId::new();
        db.a.register_knowledge_collection(&registration, None).await.unwrap();
        db.a.create_knowledge_generation(&lease,
&registration.tenant,
generation,
&specification,
&embedding_fixture::runtime(specification.space().clone())).await.unwrap();
        let visible = member(&registration, &specification, "visible", "operations", &[]);
        insert(&db.a, &lease, &registration, generation, &specification, &visible).await;
        complete(&db.a, &lease, &registration, generation).await.unwrap();
        db.a.activate_knowledge_generation(&lease,
&registration.tenant,
generation,
None,
&embedding_fixture::runtime(db.a.knowledge_generation(&registration.tenant, generation).await.unwrap().unwrap().space().clone())).await.unwrap();
        let table = format!("knowledge_chunk_{}", generation.as_uuid().simple());
        db.a.client().query(include_str!("../queries/knowledge/coordinator/source_loss_and_freshness_exclude_malformed_cached_rows_before_decode.surql"))
            .bind(("table", table.clone()))
            .await.unwrap().check().unwrap();
        assert_candidates(&db.b, &scope(&registration), generation, &[]).await;
        assert!(!db.b.knowledge_member_observed(&registration, visible.uri()).await.unwrap());
        db.a.client().query(include_str!("../queries/knowledge/coordinator/source_loss_and_freshness_exclude_malformed_cached_rows_before_decode_2.surql")).await.unwrap().check().unwrap();
        db.a.invalidate_knowledge_collection(&lease, &registration, generation).await.unwrap();
        assert_candidates(&db.b, &scope(&registration), generation, &[]).await;
    }).await.expect("source freshness qualification exceeded 120 seconds");
}
