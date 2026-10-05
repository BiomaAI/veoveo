use super::*;
use veoveo_platform_store::{
    GatewayControlRevisionContent, GatewayControlRevisionSource, RecordId,
};

async fn control(store: &PlatformStore, digest: &Sha256Digest) {
    let id = uuid::Uuid::now_v7().to_string();
    let content = GatewayControlRevisionContent {
        revision_id: id.clone(),
        sha256: digest.hex().to_owned(),
        source: GatewayControlRevisionSource::SeedFile,
        applied_at: Utc::now(),
        applied_by: "catalog-test".into(),
        tenant: None,
        control_plane: serde_json::from_value(serde_json::json!({})).unwrap(),
    };
    store
        .client()
        .query(include_str!("../queries/knowledge/catalog/control.surql"))
        .bind((
            "revision",
            RecordId::new("gateway_control_revision", id.clone()),
        ))
        .bind(("content", content))
        .bind(("id", id))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn complete_catalog_replacement_fences_racing_discovery_and_removed_approvals() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let mut record = registration("catalog-sync");
        control(&db.a, &record.control_revision).await;
        let first =
            db.a.begin_knowledge_catalog(&record.tenant, &record.control_revision)
                .await
                .unwrap();
        let second =
            db.b.begin_knowledge_catalog(&record.tenant, &record.control_revision)
                .await
                .unwrap();
        let records = [record.clone()];
        let (a, b) = tokio::join!(
            db.a.replace_knowledge_catalog(first, &records),
            db.b.replace_knowledge_catalog(second, &records)
        );
        assert_ne!(
            a.is_ok(),
            b.is_ok(),
            "only one discovery can replace the captured empty catalog"
        );
        assert_eq!(
            db.a.knowledge_collection(&record.tenant, record.descriptor.collection())
                .await
                .unwrap(),
            Some(record.clone())
        );
        let old_control =
            db.a.begin_knowledge_catalog(&record.tenant, &record.control_revision)
                .await
                .unwrap();
        let next = Sha256Digest::from_bytes([2; 32]);
        control(&db.a, &next).await;
        assert!(
            db.b.replace_knowledge_catalog(old_control, &records)
                .await
                .is_err(),
            "old control authority cannot restore registrations"
        );
        assert!(
            db.a.begin_knowledge_catalog(&record.tenant, &record.control_revision)
                .await
                .is_err()
        );
        record.control_revision = next;
        let current =
            db.b.begin_knowledge_catalog(&record.tenant, &record.control_revision)
                .await
                .unwrap();
        db.b.replace_knowledge_catalog(current, &[record.clone()])
            .await
            .unwrap();
        let other = registration("another-tenant");
        db.a.register_knowledge_collection(&other, None)
            .await
            .unwrap();
        let clear =
            db.a.begin_knowledge_catalog(&record.tenant, &record.control_revision)
                .await
                .unwrap();
        db.a.replace_knowledge_catalog(clear, &[]).await.unwrap();
        assert!(
            db.b.knowledge_collection(&record.tenant, record.descriptor.collection())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            db.b.knowledge_collection(&other.tenant, other.descriptor.collection())
                .await
                .unwrap(),
            Some(other)
        );
    })
    .await
    .expect("catalog replacement exceeded 120 seconds");
}
