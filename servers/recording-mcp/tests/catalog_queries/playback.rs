//! Playback admission across capture-layer publication; no renderer or Artifact bytes.
use super::*;
use base64::Engine as _;
use futures::StreamExt as _;
use veoveo_recording_mcp::{
    contract::PlaybackManifest, playback::PlaybackManager, service::PlaybackArchiveSelection,
};
use veoveo_recording_store::RecordingRepository;
use veoveo_recording_store::{RecordingLayerId, RecordingReadGrantClass};

async fn manifest(
    service: &RecordingService,
    manager: &PlaybackManager,
    caller: &GatewayInternalIdentity,
    recording: RecordingId,
) -> PlaybackManifest {
    let plan = service
        .playback_plan(
            caller,
            None,
            recording,
            PlaybackArchiveSelection::SealedViewer,
        )
        .await
        .unwrap()
        .unwrap();
    let grant = service
        .issue_read_grant(
            caller,
            plan.dataset_id,
            RecordingReadGrantClass::ViewerSegment,
            vec![recording],
            plan.catalog_revision.clone(),
            None,
        )
        .await
        .unwrap();
    manager.prepare_manifest(plan, grant).await.unwrap()
}

#[tokio::test]
async fn live_receiver_survives_the_gap_between_capture_layers() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = fixture::TestDb::with_modules(vec![
        veoveo_recording_store::schema::module_setup(
            fixture::module_lanes::execution("recordings").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    tokio::time::timeout(Duration::from_secs(30), async {
        let spool = tempfile::tempdir().unwrap();
        let service = RecordingService::new(
            db.b.clone(),
            HttpArtifactPlane::new("http://127.0.0.1:1"),
            spool.path().to_owned(),
        )
        .unwrap();
        let caller = identity("playback-rollover", "reader", &["operations"]);
        let producer = service.platform_identity(&caller).await.unwrap();
        let dataset = RecordingRepository::new(db.a.clone())
            .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
                producer.clone(),
                "rollover",
            ))
            .await
            .unwrap();
        let dataset_id =
            RecordingDatasetId::from_uuid(record_uuid(&dataset.id, "recording_dataset").unwrap());
        let draft = RecordingDraft {
            identity: producer.clone(),
            authority: veoveo_recording_hub::invocation_authority_record(&caller.authority),
            dataset_id,
            application_id: dataset_id.to_string(),
            recording_key: "live-rollover".into(),
            classification: "unclassified".into(),
            labels: vec!["operations".into()],
            metadata: BTreeMap::new(),
            started_at: Utc::now(),
        };
        let row = RecordingRepository::new(db.a.clone())
            .create_recording(draft.clone())
            .await
            .unwrap();
        let recording = RecordingId::from_uuid(record_uuid(&row.id, "recording").unwrap());
        let key = base64::engine::general_purpose::STANDARD.encode([7_u8; 32]);
        let manager = PlaybackManager::new(&key, "https://archive.example", db.b.clone()).unwrap();

        // The channel exists before capture starts, including reconnects in the
        // publication gap after a previous layer leaves Writing.
        let first = manifest(&service, &manager, &caller, recording).await;
        assert!(first.archive.is_none());
        assert!(
            first.live.is_some(),
            "a live recording must advertise its channel before a layer exists"
        );
        let receiver = serde_json::to_value(&first.live).unwrap();
        for ordinal in 0..2 {
            let relative = format!("capture-{ordinal}.rrd");
            std::fs::write(spool.path().join(&relative), []).unwrap();
            let layer = RecordingRepository::new(db.a.clone())
                .open_recording_layer(
                    RecordingLayerDraft::capture(
                        producer.clone(),
                        recording,
                        ordinal,
                        relative,
                        Some(Utc::now()),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
            let writing = manifest(&service, &manager, &caller, recording).await;
            assert_eq!(serde_json::to_value(&writing.live).unwrap(), receiver);
            let layer_id =
                RecordingLayerId::from_uuid(record_uuid(&layer.id, "recording_layer").unwrap());
            RecordingRepository::new(db.a.clone())
                .stage_recording_layer(
                    &producer,
                    layer_id,
                    128,
                    1,
                    &"a".repeat(64),
                    Some("0.38.1"),
                    None,
                    Some(Utc::now()),
                )
                .await
                .unwrap();
            let gap = manifest(&service, &manager, &caller, recording).await;
            assert_ne!(gap.catalog_revision, writing.catalog_revision);
            assert!(gap.archive.is_none());
            assert_eq!(serde_json::to_value(&gap.live).unwrap(), receiver);
            // The channel declaration conveys no authority of its own.
            let mut denied = caller.clone();
            denied.actor.data_labels.clear();
            assert!(
                service
                    .playback_plan(&denied, None, recording, PlaybackArchiveSelection::Omit,)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        let row = RecordingRepository::new(db.a.clone())
            .create_recording(RecordingDraft {
                recording_key: "idle-start".into(),
                ..draft
            })
            .await
            .unwrap();
        let idle = RecordingId::from_uuid(record_uuid(&row.id, "recording").unwrap());
        let mut channel = veoveo_recording_mcp::live_stream::authorized_live_rrd_stream(
            service.clone(),
            caller.clone(),
            idle,
            Duration::from_secs(1),
            veoveo_recording_mcp::playback::playback_store_id(dataset_id, idle).unwrap(),
            veoveo_recording_mcp::live_stream::LiveRrdStart::Bootstrap,
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(200), channel.next())
                .await
                .is_err()
        );
        RecordingRepository::new(db.a.clone())
            .interrupt_recording(
                &producer,
                idle,
                Utc::now(),
                "producer closed before capture",
            )
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(5), channel.next())
                .await
                .expect("an idle channel must close when the recording leaves Live")
                .is_none()
        );
        assert!(
            manifest(&service, &manager, &caller, idle)
                .await
                .live
                .is_none()
        );
    })
    .await
    .expect("live rollover admission exceeded 30 seconds");
}
