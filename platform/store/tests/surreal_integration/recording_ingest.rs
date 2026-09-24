use super::*;
use chrono::DateTime;
use veoveo_platform_store::{
    RecordingDatasetId, RecordingIngestBatchDraft, RecordingIngestBatchState, RecordingIngestQuota,
    RecordingIngestStreamDraft, RecordingIngestStreamId, RecordingIngestStreamState,
};

#[tokio::test]
async fn finished_stream_replays_materialized_batch_without_reopening_or_appending() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let store = PlatformStore::connect(
        StoreConfig::builder(
            endpoint,
            "veoveo_integration",
            format!("ingest_recovery_{}", Uuid::now_v7().simple()),
            StoreCredentials::root(username, SecretString::from(password)),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    let identity = store
        .ensure_identity(
            "ingest-recovery",
            "recording-hub",
            "https://veoveo.local/services",
            "recording-hub",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let dataset = store
        .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
            identity.clone(),
            "world",
        ))
        .await
        .unwrap();
    let recording = store
        .create_recording(RecordingDraft {
            identity: identity.clone(),
            authority: artifact_authority(&identity),
            dataset_id: RecordingDatasetId::from_uuid(record_uuid(&dataset.id)),
            application_id: "sensor-suite".into(),
            recording_key: "recovery-run".into(),
            classification: "internal".into(),
            labels: Vec::new(),
            metadata: BTreeMap::new(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    let stream = store
        .open_recording_ingest_stream(RecordingIngestStreamDraft {
            identity: identity.clone(),
            recording_id: RecordingId::from_uuid(record_uuid(&recording.id)),
            producer_id: "sensor".into(),
            oauth_client_id: "sensor-client".into(),
            source_stream_id: "sensor-stream".into(),
            application_id: recording.application_id.clone(),
            recording_key: recording.recording_key.clone(),
            dataset: "world".into(),
            maximum_concurrent_streams: 1,
        })
        .await
        .unwrap();
    let stream_id = RecordingIngestStreamId::from_uuid(record_uuid(&stream.id));
    let draft = RecordingIngestBatchDraft {
        identity: identity.clone(),
        stream_id,
        sequence: 1,
        payload_format: "rerun_rrd".into(),
        sha256: "a".repeat(64),
        relative_path: format!("{}/{stream_id}/00000000000000000001.pb", identity.tenant_id),
        byte_len: 128,
        message_count: 2,
        producer_id: "sensor".into(),
        maximum_batches_per_minute: 1,
        maximum_bytes_per_day: 128,
    };
    let accepted = store
        .commit_recording_ingest_batch(draft.clone())
        .await
        .unwrap();
    assert!(!accepted.duplicate);
    store
        .mark_recording_ingest_materialized(identity.tenant_id, stream_id, 1)
        .await
        .unwrap();
    let finished = store
        .finish_recording_ingest_stream(identity.tenant_id, stream_id)
        .await
        .unwrap();
    assert_eq!(finished.state, RecordingIngestStreamState::Finished);

    // Hub startup replays a journal left after materialization committed but
    // before the file was removed. The stream may already have been finished.
    let replay = store
        .commit_recording_ingest_batch(draft.clone())
        .await
        .unwrap();
    assert!(replay.duplicate);
    assert_eq!(replay.stream, finished);
    assert_eq!(replay.batch.id, accepted.batch.id);
    assert_eq!(replay.batch.state, RecordingIngestBatchState::Materialized);

    let mut conflict = draft.clone();
    conflict.sha256 = "b".repeat(64);
    assert!(matches!(
        store.commit_recording_ingest_batch(conflict).await,
        Err(StoreError::RecordingIngestDigestConflict { sequence: 1 })
    ));
    let mut next = draft;
    next.sequence = 2;
    next.relative_path = format!("{}/{stream_id}/00000000000000000002.pb", identity.tenant_id);
    assert!(matches!(
        store.commit_recording_ingest_batch(next).await,
        Err(StoreError::RecordingIngestStreamStateConflict { state, .. }) if state == "finished"
    ));
    assert_eq!(
        store
            .recording_ingest_stream(identity.tenant_id, stream_id)
            .await
            .unwrap(),
        Some(finished)
    );
    assert!(
        store
            .recording_ingest_batch(identity.tenant_id, stream_id, 2)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn batches_commit_once_across_minute_and_day_boundaries() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let store = PlatformStore::connect(
        StoreConfig::builder(
            endpoint,
            "veoveo_integration",
            format!("ingest_quota_boundary_{}", Uuid::now_v7().simple()),
            StoreCredentials::root(username, SecretString::from(password)),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    let identity = store
        .ensure_identity(
            "ingest-quota-boundary",
            "recording-hub",
            "https://veoveo.local/services",
            "recording-hub",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let dataset = store
        .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
            identity.clone(),
            "world",
        ))
        .await
        .unwrap();
    let recording = store
        .create_recording(RecordingDraft {
            identity: identity.clone(),
            authority: artifact_authority(&identity),
            dataset_id: RecordingDatasetId::from_uuid(record_uuid(&dataset.id)),
            application_id: "sensor-suite".into(),
            recording_key: "boundary-run".into(),
            classification: "internal".into(),
            labels: Vec::new(),
            metadata: BTreeMap::new(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    let mut stream = store
        .open_recording_ingest_stream(RecordingIngestStreamDraft {
            identity: identity.clone(),
            recording_id: RecordingId::from_uuid(record_uuid(&recording.id)),
            producer_id: "sensor".into(),
            oauth_client_id: "sensor-client".into(),
            source_stream_id: "boundary-stream".into(),
            application_id: recording.application_id.clone(),
            recording_key: recording.recording_key.clone(),
            dataset: "world".into(),
            maximum_concurrent_streams: 1,
        })
        .await
        .unwrap();
    let stream_id = RecordingIngestStreamId::from_uuid(record_uuid(&stream.id));
    let draft = |sequence: u64| RecordingIngestBatchDraft {
        identity: identity.clone(),
        stream_id,
        sequence,
        payload_format: "rerun_rrd".into(),
        sha256: format!("{sequence:064x}"),
        relative_path: format!("{}/{stream_id}/{sequence:020}.pb", identity.tenant_id),
        byte_len: 128,
        message_count: 2,
        producer_id: "sensor".into(),
        maximum_batches_per_minute: 1,
        maximum_bytes_per_day: 1_000_000,
    };
    let instant = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    };

    // Each append chooses its checkpoint and commits at one acceptance time,
    // as the Hub materializer does. The last nanosecond of a day and the
    // first nanosecond of the next day both commit on the first attempt.
    for (sequence, accepted_at) in [
        (1, instant("2026-01-01T23:59:59.999999999Z")),
        (2, instant("2026-01-02T00:00:00Z")),
        (3, instant("2026-01-02T00:01:00Z")),
    ] {
        let quota = store
            .recording_ingest_quota_checkpoint(identity.tenant_id, "sensor", accepted_at)
            .await
            .unwrap();
        let outcome = store
            .commit_recording_ingest_batch_at_checkpoints(
                stream.clone(),
                quota,
                accepted_at,
                draft(sequence),
            )
            .await
            .unwrap();
        assert!(!outcome.duplicate);
        assert_eq!(outcome.batch.created_at, accepted_at);
        stream = outcome.stream;
    }

    // Same-window quota enforcement still applies: the minute window that
    // accepted sequence 3 allows one batch.
    let same_minute = instant("2026-01-02T00:01:30Z");
    let quota = store
        .recording_ingest_quota_checkpoint(identity.tenant_id, "sensor", same_minute)
        .await
        .unwrap();
    assert!(matches!(
        store
            .commit_recording_ingest_batch_at_checkpoints(
                stream.clone(),
                quota.clone(),
                same_minute,
                draft(4),
            )
            .await,
        Err(StoreError::RecordingIngestQuotaExceeded {
            quota: RecordingIngestQuota::MaximumBatchesPerMinute
        })
    ));

    // A checkpoint is still refused for an acceptance time outside its windows.
    assert!(matches!(
        store
            .commit_recording_ingest_batch_at_checkpoints(
                stream,
                quota,
                instant("2026-01-02T00:02:00Z"),
                draft(4),
            )
            .await,
        Err(StoreError::InvalidRecordingIngestField {
            field: "quota_checkpoint",
            ..
        })
    ));
}
