//! Metadata admission only: synthetic references never establish playback or GPU behavior.
use super::{fixture::TestDb, *};
use std::path::Path;
use veoveo_platform_store::{ArtifactId, RecordId};
use veoveo_recording_store::RecordingRepository;
use veoveo_types::ScopeDefinition;

pub(super) async fn qualify(
    db: &TestDb,
    service: &RecordingService,
    producer: &GatewayInternalIdentity,
    denied: &GatewayInternalIdentity,
    dataset_id: RecordingDatasetId,
    spool: &Path,
) {
    let platform = service.platform_identity(producer).await.unwrap();
    let tenant_id = platform.tenant_id;
    let row = RecordingRepository::new(db.a.clone())
        .create_recording(RecordingDraft {
            identity: platform.clone(),
            authority: veoveo_recording_hub::invocation_authority_record(&producer.authority),
            dataset_id,
            application_id: dataset_id.to_string(),
            recording_key: "metadata-admission".into(),
            classification: "restricted".into(),
            labels: vec!["restricted".into()],
            metadata: BTreeMap::new(),
            started_at: Utc::now() - TimeDelta::minutes(1),
        })
        .await
        .unwrap();
    let recording = RecordingId::from_uuid(record_uuid(&row.id, "recording").unwrap());
    let layer = RecordingRepository::new(db.a.clone())
        .open_recording_layer(
            RecordingLayerDraft::capture(platform, recording, 0, "metadata/first.rrd".into(), None)
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .layer_views(producer, recording)
            .await
            .unwrap()
            .unwrap()[0]
            .byte_len,
        0
    );
    let occurrence = ArtifactId::new();
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/metadata/qualify.surql"
        ))
        .bind(("layer", layer.id.clone()))
        .bind(("artifact", occurrence.record_id()))
        .bind(("digest", "a".repeat(64)))
        .await
        .unwrap()
        .check()
        .unwrap();
    let admitted = service
        .layer_views(producer, recording)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        admitted[0]
            .artifact_uri
            .as_ref()
            .unwrap()
            .artifact_id()
            .as_uuid(),
        occurrence.as_uuid()
    );

    for digest in ["invalid".to_owned(), "A".repeat(64)] {
        db.a.client()
            .query(include_str!(
                "../queries/catalog_queries/metadata/qualify_2.surql"
            ))
            .bind(("layer", layer.id.clone()))
            .bind(("digest", digest))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(service.layer_views(producer, recording).await.is_err());
        assert!(
            service
                .layer_views(denied, recording)
                .await
                .unwrap()
                .is_none()
        );
    }
    let mut sealer = producer.clone();
    sealer
        .actor
        .scopes
        .insert(RecordingScope::Seal.name().clone());
    // Rejected committed metadata must not advance a fresh seal to `sealing`.
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/metadata/qualify_3.surql"
        ))
        .bind(("recording", row.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(service.seal(&sealer, recording).await.is_err());
    assert_eq!(
        RecordingRepository::new(db.a.clone())
            .recording(tenant_id, recording)
            .await
            .unwrap()
            .unwrap()
            .state,
        veoveo_recording_store::RecordingState::Ready
    );
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/metadata/qualify_4.surql"
        ))
        .bind(("recording", row.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/metadata/qualify_5.surql"
        ))
        .bind(("layer", layer.id.clone()))
        .bind(("digest", "a".repeat(64)))
        .await
        .unwrap()
        .check()
        .unwrap();
    // Storage admits these native timestamps; owner reads and seal admission
    // reject the contradiction without repairing or advancing the row.
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/metadata/out_of_order_times.surql"
        ))
        .bind(("recording", row.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let before = RecordingRepository::new(db.a.clone())
        .recording(tenant_id, recording)
        .await
        .unwrap()
        .unwrap();
    assert!(service.recording_view(producer, recording).await.is_err());
    assert!(
        service
            .recording_view(denied, recording)
            .await
            .unwrap()
            .is_none()
    );
    assert!(service.seal(&sealer, recording).await.is_err());
    let after = RecordingRepository::new(db.a.clone())
        .recording(tenant_id, recording)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(after).unwrap()
    );
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/metadata/qualify_4.surql"
        ))
        .bind(("recording", row.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    for malformed in [
        ArtifactId::from_uuid(uuid::Uuid::new_v4()).record_id(),
        RecordId::new("artifact_occurrence", "untyped-private-reference"),
    ] {
        db.a.client()
            .query(include_str!(
                "../queries/catalog_queries/metadata/qualify_6.surql"
            ))
            .bind(("layer", layer.id.clone()))
            .bind(("recording", row.id.clone()))
            .bind(("artifact", malformed))
            .await
            .unwrap()
            .check()
            .unwrap();
        let error = service.layer_views(producer, recording).await.unwrap_err();
        assert!(!format!("{error:#}").contains("untyped-private-reference"));
        assert!(service.recording_view(producer, recording).await.is_err());
        assert!(
            service
                .recording_view(denied, recording)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            service
                .layer_views(denied, recording)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            service
                .recording_view(denied, recording)
                .await
                .unwrap()
                .is_none()
        );
        // The denied malformed record sorts first. SQL excludes it before public mapping.
        assert!(service.catalog_page(denied, None).await.is_ok());
    }
    let manifest = ArtifactId::new();
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/metadata/qualify_7.surql"
        ))
        .bind(("layer", layer.id.clone()))
        .bind(("recording", row.id.clone()))
        .bind(("layer_artifact", occurrence.record_id()))
        .bind(("manifest", manifest.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let admitted = service
        .recording_view(producer, recording)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        admitted
            .manifest_artifact_uri
            .as_ref()
            .unwrap()
            .artifact_id()
            .as_uuid(),
        manifest.as_uuid()
    );

    let context = spool
        .join("native-catalog")
        .join(format!(".recording-{recording}.static-context"));
    std::fs::create_dir_all(context.parent().unwrap()).unwrap();
    std::fs::write(&context, b"owned cleanup sentinel").unwrap();
    // A contradictory retry must not remove its owned local context.
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/metadata/qualify_8.surql"
        ))
        .bind(("recording", row.id.clone()))
        .bind(("manifest", occurrence.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(service.seal(&sealer, recording).await.is_err());
    assert!(context.exists());
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/metadata/qualify_9.surql"
        ))
        .bind(("recording", row.id))
        .bind(("manifest", manifest.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let output = service.seal(&sealer, recording).await.unwrap();
    assert_eq!(
        output.manifest_artifact_uri.artifact_id().as_uuid(),
        manifest.as_uuid()
    );
    assert_eq!(
        output.layer_artifact_uris[0].artifact_id().as_uuid(),
        occurrence.as_uuid()
    );
    assert!(!context.exists());
}
