#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
use fixture::TestDb;
use veoveo_platform_store::{
    GatewayControlRevisionContent, GatewayControlRevisionSource, OpenObject, RecordId, StoreError,
};

#[tokio::test]
async fn active_revision_changes_are_visible_and_corrupt_pointers_fail_closed() {
    let db = TestDb::new().await;
    assert!(
        db.a.active_gateway_control_revision()
            .await
            .unwrap()
            .is_none()
    );
    for name in ["revision-a", "revision-b"] {
        let revision = RecordId::new("gateway_control_revision", name);
        let content = GatewayControlRevisionContent {
            revision_id: name.into(),
            sha256: "a".repeat(64),
            source: GatewayControlRevisionSource::SeedFile,
            applied_at: chrono::Utc::now(),
            applied_by: "fixture".into(),
            tenant: None,
            control_plane: OpenObject::default(),
        };
        db.a.client().query("BEGIN TRANSACTION; CREATE ONLY $revision CONTENT $content; UPSERT gateway_control_active:current SET revision = $revision, revision_id = $name, updated_at = time::now(); COMMIT TRANSACTION;")
            .bind(("revision", revision.clone())).bind(("content", content)).bind(("name", name))
            .await.unwrap().check().unwrap();
        let current =
            db.b.active_gateway_control_revision()
                .await
                .unwrap()
                .unwrap();
        assert_eq!(current.id, revision);
        assert_eq!(current.revision_id, name);
    }
    let writer = db.a.clone();
    let updates = tokio::spawn(async move {
        for i in 0..24 {
            let name = if i % 2 == 0 {
                "revision-a"
            } else {
                "revision-b"
            };
            writer.client().query("UPDATE gateway_control_active:current SET revision = $revision, revision_id = $name, updated_at = time::now();")
                .bind(("revision", RecordId::new("gateway_control_revision", name))).bind(("name", name))
                .await.unwrap().check().unwrap();
        }
    });
    for _ in 0..24 {
        let current =
            db.b.active_gateway_control_revision()
                .await
                .unwrap()
                .unwrap();
        assert!(matches!(
            current.revision_id.as_str(),
            "revision-a" | "revision-b"
        ));
        assert_eq!(
            current.id,
            RecordId::new("gateway_control_revision", current.revision_id)
        );
    }
    updates.await.unwrap();
    db.a.client()
        .query("UPDATE gateway_control_active:current SET revision_id = 'wrong-name';")
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        db.b.active_gateway_control_revision().await,
        Err(StoreError::InvalidGatewayControlRevision)
    ));
    db.a.client().query("UPDATE gateway_control_active:current SET revision = gateway_control_revision:missing;").await.unwrap().check().unwrap();
    assert!(matches!(
        db.b.active_gateway_control_revision().await,
        Err(StoreError::InvalidGatewayControlRevision)
    ));
}
