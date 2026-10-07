//! Store-authorized playback bytes and capture-layer publication; no renderer.
use super::*;
use base64::Engine as _;
use futures::StreamExt as _;
use re_build_info::CrateVersion;
use re_chunk::{Chunk, RowId};
use re_log_encoding::{Decoder, EncodingOptions, rrd::Encoder};
use re_log_types::{ApplicationId, LogMsg, StoreId};
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::Scalars;
use sha2::Digest as _;
use std::{io::Cursor, path::Path};
use veoveo_recording_mcp::{
    contract::PlaybackManifest,
    live_stream::{LiveRrdStart, LiveRrdStream, authorized_live_rrd_stream},
    playback::PlaybackManager,
    service::PlaybackArchiveSelection,
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

fn rrd_bytes(messages: &[LogMsg]) -> Vec<u8> {
    let mut encoder = Encoder::new_eager(
        CrateVersion::LOCAL,
        EncodingOptions::PROTOBUF_COMPRESSED,
        Vec::new(),
    )
    .unwrap();
    for message in messages {
        encoder.append(message).unwrap();
    }
    encoder.finish().unwrap();
    encoder.into_inner().unwrap()
}

fn publish_part(directory: &Path, sequence: u64, messages: &[LogMsg]) {
    // Match ingest's atomic publication: an incomplete staging file is never a part.
    let staging = directory.join(format!("{sequence:020}.rrd.{}.tmp", uuid::Uuid::now_v7()));
    std::fs::write(&staging, rrd_bytes(messages)).unwrap();
    std::fs::rename(staging, directory.join(format!("{sequence:020}.rrd"))).unwrap();
}

fn sensor_rows(messages: &[LogMsg]) -> BTreeMap<String, BTreeSet<RowId>> {
    let mut rows = BTreeMap::new();
    for message in messages {
        if let LogMsg::ArrowMsg(_, arrow) = message {
            let chunk = Chunk::from_arrow_msg(arrow).unwrap();
            let entity = chunk.entity_path().to_string();
            if entity == "/sensor/calibration" || entity == "/sensor/value" {
                let selected: &mut BTreeSet<RowId> = rows.entry(entity).or_default();
                for row in chunk.row_ids() {
                    assert!(selected.insert(row), "replayed sensor row {row}");
                }
            }
        }
    }
    rows
}

async fn next_frame(channel: &mut LiveRrdStream, store: &StoreId) -> Vec<LogMsg> {
    let bytes = tokio::time::timeout(Duration::from_secs(5), channel.next())
        .await
        .expect("Store-authorized live frame did not arrive within five seconds")
        .expect("live channel ended before its frame")
        .expect("live channel failed");
    assert!(bytes.len() > 4);
    assert_eq!(
        u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize,
        bytes.len() - 4
    );
    let messages = Decoder::<LogMsg>::decode_eager(Cursor::new(&bytes[4..]))
        .expect("frame is not a complete RRD")
        .collect::<Result<Vec<_>, _>>()
        .expect("frame RRD decoding failed");
    assert!(!messages.is_empty());
    assert!(messages.iter().all(|message| message.store_id() == store));
    messages
}

#[tokio::test]
async fn store_authorized_rrd_bootstrap_reconnect_rollover_and_revocation() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = fixture::TestDb::with_modules(vec![
        veoveo_recording_store::schema::module_setup(
            fixture::module_lanes::execution("recordings").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    tokio::time::timeout(Duration::from_secs(60), async {
        let spool = tempfile::tempdir().unwrap();
        let service = RecordingService::new(
            db.b.clone(),
            HttpArtifactPlane::new("http://127.0.0.1:1"),
            spool.path().to_owned(),
        )
        .unwrap();
        let caller = identity("playback-bytes", "reader", &["operations"]);
        let producer = service.platform_identity(&caller).await.unwrap();
        let repository = RecordingRepository::new(db.a.clone());
        let dataset = repository
            .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
                producer.clone(), "playback-bytes",
            ))
            .await.unwrap();
        let dataset_id = RecordingDatasetId::from_uuid(record_uuid(&dataset.id, "recording_dataset").unwrap());
        let row = repository.create_recording(RecordingDraft {
            identity: producer.clone(),
            authority: veoveo_recording_hub::invocation_authority_record(&caller.authority),
            dataset_id,
            application_id: dataset_id.to_string(),
            recording_key: "live-bytes".into(),
            classification: "unclassified".into(),
            labels: vec!["operations".into()],
            metadata: BTreeMap::new(),
            started_at: Utc::now(),
        }).await.unwrap();
        let recording = RecordingId::from_uuid(record_uuid(&row.id, "recording").unwrap());
        let store = veoveo_recording_mcp::playback::playback_store_id(dataset_id, recording).unwrap();
        let day = spool.path().join("dataset/2026-10-07");
        std::fs::create_dir_all(&day).unwrap();
        let stream_id = uuid::Uuid::now_v7();
        let first_path = day.join(format!("recording.ingest-{stream_id}-s0.rrd"));
        let first_parts = veoveo_rrd::ingest_parts::ingest_segment_parts_directory(&first_path);
        std::fs::create_dir(&first_parts).unwrap();
        let (source, storage) = RecordingStreamBuilder::new(ApplicationId::try_new(dataset_id.to_string()).unwrap())
            .recording_id(recording.to_string()).memory().unwrap();
        source.log_static("sensor/calibration", &Scalars::single(1.0)).unwrap();
        source.log("sensor/value", &Scalars::single(10.0)).unwrap();
        let initial = storage.take();
        let expected_rows = sensor_rows(&initial);
        assert_eq!(expected_rows.len(), 2);
        assert_eq!(expected_rows["/sensor/calibration"].len(), 1);
        assert_eq!(expected_rows["/sensor/value"].len(), 1);
        let store_info = initial.iter().find(|message| matches!(message, LogMsg::SetStoreInfo(_))).unwrap().clone();
        let static_messages = initial.iter().filter(|message| match message {
            LogMsg::SetStoreInfo(_) => true,
            LogMsg::ArrowMsg(_, arrow) => Chunk::from_arrow_msg(arrow).unwrap().is_static(),
            _ => false,
        }).cloned().collect::<Vec<_>>();
        let mut first_messages = vec![store_info.clone()];
        first_messages.extend(initial.iter().filter(|message| matches!(message, LogMsg::ArrowMsg(_, arrow) if !Chunk::from_arrow_msg(arrow).unwrap().is_static())).cloned());
        let context = veoveo_recording_hub::ingest_recording_static_context_path(&first_path, recording).unwrap();
        std::fs::write(context, rrd_bytes(&static_messages)).unwrap();
        publish_part(&first_parts, 0, &first_messages);
        let first_layer = repository.open_recording_layer(RecordingLayerDraft::capture(
            producer.clone(), recording, 0,
            first_path.strip_prefix(spool.path()).unwrap().to_str().unwrap().into(), Some(Utc::now()),
        ).unwrap()).await.unwrap();

        let channel = |start| authorized_live_rrd_stream(
            service.clone(), caller.clone(), recording, Duration::from_secs(60), store.clone(), start,
        );
        let mut bootstrap = channel(LiveRrdStart::Bootstrap);
        let emitted = next_frame(&mut bootstrap, &store).await;
        assert_eq!(sensor_rows(&emitted), sensor_rows(&initial));
        assert_eq!(emitted.iter().filter(|message| matches!(message, LogMsg::SetStoreInfo(_))).count(), 1);
        drop(bootstrap);

        let mut resumed = channel(LiveRrdStart::ResumeHead);
        assert!(tokio::time::timeout(Duration::from_millis(200), resumed.next()).await.is_err(),
            "same-channel reconnect replayed the bootstrap");
        source.log("sensor/value", &Scalars::single(20.0)).unwrap();
        let update = storage.take();
        let mut update_part = vec![store_info.clone()];
        update_part.extend(update.clone());
        publish_part(&first_parts, 1, &update_part);
        let emitted_update = next_frame(&mut resumed, &store).await;
        assert_eq!(sensor_rows(&emitted_update), sensor_rows(&update));
        assert!(!emitted_update.iter().any(|message| matches!(message, LogMsg::SetStoreInfo(_))));
        assert!(!sensor_rows(&emitted_update).contains_key("/sensor/calibration"));

        let mut fresh = channel(LiveRrdStart::Bootstrap);
        let emitted_fresh = next_frame(&mut fresh, &store).await;
        let mut all_messages = initial.clone();
        all_messages.extend(update);
        assert_eq!(sensor_rows(&emitted_fresh), sensor_rows(&all_messages));
        assert_eq!(emitted_fresh.iter().filter(|message| matches!(message, LogMsg::SetStoreInfo(_))).count(), 1);
        drop(fresh);

        let archived = rrd_bytes(&all_messages);
        let first_layer_id = RecordingLayerId::from_uuid(record_uuid(&first_layer.id, "recording_layer").unwrap());
        repository.stage_recording_layer(&producer, first_layer_id,
            archived.len().try_into().unwrap(), all_messages.len().try_into().unwrap(),
            &veoveo_types::Sha256Digest::from_bytes(sha2::Sha256::digest(&archived).into()),
            None, None, Some(Utc::now()),
        ).await.unwrap();
        std::fs::remove_dir_all(&first_parts).unwrap();
        let second_path = day.join(format!("recording.ingest-{stream_id}-s1.rrd"));
        let second_parts = veoveo_rrd::ingest_parts::ingest_segment_parts_directory(&second_path);
        std::fs::create_dir(&second_parts).unwrap();
        source.log("sensor/value", &Scalars::single(30.0)).unwrap();
        let rollover = storage.take();
        let mut rollover_part = vec![store_info];
        rollover_part.extend(rollover.clone());
        publish_part(&second_parts, 0, &rollover_part);
        let second_layer = repository.open_recording_layer(RecordingLayerDraft::capture(
            producer.clone(), recording, 1,
            second_path.strip_prefix(spool.path()).unwrap().to_str().unwrap().into(), Some(Utc::now()),
        ).unwrap()).await.unwrap();
        let emitted_rollover = next_frame(&mut resumed, &store).await;
        assert_eq!(sensor_rows(&emitted_rollover), sensor_rows(&rollover));
        assert!(!emitted_rollover.iter().any(|message| matches!(message, LogMsg::SetStoreInfo(_))));
        assert!(tokio::time::timeout(Duration::from_millis(200), resumed.next()).await.is_err(),
            "layer rollover duplicated rows");

        // Change visibility in Store while the second layer is being followed.
        // The next layer contains invalid bytes: refusal must precede its decoder.
        db.a.client().query(include_str!(
            "../queries/catalog_queries/projections/assert_download_admission_21.surql"
        )).bind(("recording", recording.record_id())).await.unwrap().check().unwrap();
        let second_bytes = rrd_bytes(&rollover_part);
        let second_layer_id = RecordingLayerId::from_uuid(record_uuid(&second_layer.id, "recording_layer").unwrap());
        repository.stage_recording_layer(&producer, second_layer_id,
            second_bytes.len().try_into().unwrap(), rollover_part.len().try_into().unwrap(),
            &veoveo_types::Sha256Digest::from_bytes(sha2::Sha256::digest(&second_bytes).into()),
            None, None, Some(Utc::now()),
        ).await.unwrap();
        let denied_path = day.join(format!("recording.ingest-{stream_id}-s2.rrd"));
        std::fs::write(&denied_path, b"invalid RRD: layer admission must precede decoding").unwrap();
        repository.open_recording_layer(RecordingLayerDraft::capture(
            producer.clone(), recording, 2,
            denied_path.strip_prefix(spool.path()).unwrap().to_str().unwrap().into(), Some(Utc::now()),
        ).unwrap()).await.unwrap();
        std::fs::remove_dir_all(&second_parts).unwrap();
        let error = tokio::time::timeout(Duration::from_secs(5), resumed.next()).await.unwrap()
            .expect("layer-boundary revocation must return an admission error").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(error.to_string(), "recording is not visible");
        drop(resumed);

        // Fresh policy assertions govern admission, independently of an existing
        // Redap viewer grant. SQL refuses before invalid local bytes are examined.
        let mut revoked = caller.clone();
        revoked.actor.data_labels.clear();
        revoked.authority.policy_revision = PolicyVersion::parse("r2").unwrap();
        assert!(repository.visible_recording(&RecordingReadScope {
            tenant_id: producer.tenant_id, data_labels: Vec::new(),
        }, recording).await.unwrap().is_none());
        assert!(service.playback_plan(&revoked, None, recording, PlaybackArchiveSelection::Omit)
            .await.unwrap().is_none());
        for start in [LiveRrdStart::Bootstrap, LiveRrdStart::ResumeHead] {
            let mut denied = authorized_live_rrd_stream(service.clone(), revoked.clone(), recording,
                Duration::from_secs(60), store.clone(), start);
            let error = tokio::time::timeout(Duration::from_secs(5), denied.next()).await.unwrap()
                .expect("denied channel must return an admission error").unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            assert_eq!(error.to_string(), "recording is not visible");
        }
    }).await.expect("Store-backed live playback exceeded 60 seconds");
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
            let (source, storage) = RecordingStreamBuilder::new(
                ApplicationId::try_new(dataset_id.to_string()).unwrap(),
            )
            .recording_id(recording.to_string())
            .memory()
            .unwrap();
            source
                .log("sensor/value", &Scalars::single(ordinal as f64))
                .unwrap();
            let messages = storage.take();
            let bytes = rrd_bytes(&messages);
            std::fs::write(spool.path().join(&relative), &bytes).unwrap();
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
                    bytes.len() as i64,
                    messages.len().try_into().unwrap(),
                    &veoveo_types::Sha256Digest::from_bytes(sha2::Sha256::digest(&bytes).into()),
                    None,
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
