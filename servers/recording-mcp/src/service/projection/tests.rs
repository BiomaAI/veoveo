use super::*;
use chrono::TimeDelta;
use serde_json::json;
use veoveo_platform_store::{PrincipalId, TenantId, WorkContextId};

fn request() -> CreateRecordingProjectionRequest {
    serde_json::from_value(json!({
        "dataset_id": crate::contract::RecordingDatasetId::new(),
        "recording_id": crate::contract::RecordingId::new(),
        "entity_paths": ["/sensor"], "component_ids": ["Scalars:scalars"], "timeline": "tick",
        "sampling": {"kind": "range", "start": 0, "end": 2}, "sparse_fill": "none",
        "maximum_entities": 1, "maximum_columns": 1, "maximum_samples": 3,
        "maximum_rows": 3, "maximum_bytes": 1048576, "deadline_ms": 1000,
        "idempotency_key": "result", "units": {}, "coordinate_frame_refs": []
    }))
    .unwrap()
}

fn handle(
    request: &CreateRecordingProjectionRequest,
    id: RecordingProjectionReceiptId,
    bytes: &[u8],
    expires_at: DateTime<Utc>,
) -> RecordingProjectionHandle {
    projection_handle(
        request,
        id,
        "catalog-1".into(),
        projection_query_digest(request).unwrap(),
        expires_at,
        ArrowProjectionSummary {
            row_count: 1,
            omitted_sample_count: 0,
            schema_sha256: Sha256Digest::from_bytes([1; 32]),
            byte_len: (bytes.len() as u64).try_into().unwrap(),
            sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
        },
    )
    .unwrap()
}

fn receipt(handle: &RecordingProjectionHandle) -> RecordingProjectionReceiptRecord {
    RecordingProjectionReceiptRecord {
        id: RecordingProjectionReceiptId::from_uuid(handle.projection_id.as_uuid()).record_id(),
        tenant: TenantId::new().record_id(),
        actor: PrincipalId::new().record_id(),
        work_context: WorkContextId::new().record_id(),
        grant: RecordingReadGrantId::new().record_id(),
        dataset: RecordingDatasetId::from_uuid(handle.dataset_id.as_uuid()).record_id(),
        recordings: vec![RecordingId::from_uuid(handle.recording_id.as_uuid()).record_id()],
        policy_revision: "p1".into(),
        catalog_revision: handle.result.catalog_revision.clone(),
        caller_idempotency_key: "result".into(),
        manifest_digest: "a".repeat(64),
        query_digest: handle.result.query_digest.hex().into(),
        state: RecordingProjectionState::Ready,
        result_byte_len: Some(handle.result.byte_len.get() as i64),
        result_sha256: Some(handle.result.payload_sha256.hex().into()),
        failure_reason: None,
        expires_at: handle.expires_at,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn runtime(root: PathBuf, bytes: u64) -> ProjectionRuntime {
    ProjectionRuntime::new(
        root,
        ProjectionRuntimeLimits {
            aggregate_scratch_bytes: bytes,
            minimum_free_bytes: 1,
            concurrent_projections: 1,
            maximum_deadline_ms: 1_000,
        },
    )
    .unwrap()
}

#[test]
fn reservations_and_concurrency_fail_before_work_starts() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = runtime(directory.path().to_path_buf(), 100);
    let first_id = RecordingProjectionReceiptId::new();
    let first = runtime.reserve(first_id, 80).unwrap();
    assert!(
        runtime
            .reserve(RecordingProjectionReceiptId::new(), 21)
            .is_err()
    );
    assert_eq!(runtime.stats().unwrap().headroom_rejections, 1);
    drop(first);
    assert_eq!(runtime.stats().unwrap().reserved_bytes, 0);

    let permit = runtime.try_acquire().unwrap();
    assert!(runtime.try_acquire().is_err());
    assert_eq!(runtime.stats().unwrap().concurrency_rejections, 1);
    drop(permit);
    let _permit = runtime.try_acquire().unwrap();
}

#[test]
fn startup_removes_partial_and_invalid_projection_pairs() {
    let directory = tempfile::tempdir().unwrap();
    let partial = directory.path().join("orphan.arrow.partial");
    std::fs::write(&partial, b"partial").unwrap();
    let projection_id = RecordingProjectionReceiptId::new();
    let arrow = directory.path().join(format!("{projection_id}.arrow"));
    let metadata = directory.path().join(format!("{projection_id}.json"));
    std::fs::write(&arrow, b"arrow").unwrap();
    std::fs::write(&metadata, b"not-json").unwrap();

    let runtime = runtime(directory.path().to_path_buf(), 1024);
    assert!(!partial.exists());
    assert!(!arrow.exists());
    assert!(!metadata.exists());
    assert_eq!(runtime.stats().unwrap().files, 0);
    runtime.readiness().unwrap();
}

#[test]
fn repeated_startup_preserves_valid_pairs_and_charges_metadata_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let initial = runtime(directory.path().to_path_buf(), 8192);
    let id = RecordingProjectionReceiptId::new();
    let paths = initial.paths(id);
    let bytes = b"scratch integrity fixture";
    let handle = handle(&request(), id, bytes, Utc::now() + TimeDelta::minutes(5));
    std::fs::write(&paths.final_arrow, bytes).unwrap();
    write_metadata(&paths.final_metadata, &handle).unwrap();
    let expected = bytes.len() as u64 + paths.final_metadata.metadata().unwrap().len();
    drop(initial);
    for _ in 0..2 {
        let restarted = runtime(directory.path().to_path_buf(), 8192);
        assert_eq!(restarted.stats().unwrap().files, 1);
        assert_eq!(restarted.stats().unwrap().committed_bytes, expected);
        assert!(paths.final_arrow.is_file());
        assert_eq!(
            read_metadata(&paths.final_metadata).unwrap().projection_id,
            handle.projection_id
        );
        restarted.readiness().unwrap();
    }
    assert!(
        ProjectionRuntime::new(
            directory.path().to_path_buf(),
            ProjectionRuntimeLimits {
                aggregate_scratch_bytes: expected - 1,
                minimum_free_bytes: 1,
                concurrent_projections: 1,
                maximum_deadline_ms: 1000,
            }
        )
        .is_err()
    );
    assert!(paths.final_metadata.is_file());
}

#[test]
fn reservations_reclaim_expired_pairs_without_touching_live_results_or_partial_work() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = runtime(directory.path().to_path_buf(), 4096);
    let active_id = RecordingProjectionReceiptId::new();
    let active = runtime.reserve(active_id, 128).unwrap();
    let active_paths = runtime.paths(active_id);
    std::fs::write(&active_paths.partial_arrow, b"active").unwrap();
    let expired_id = RecordingProjectionReceiptId::new();
    let retained_id = RecordingProjectionReceiptId::new();
    let bytes = b"scratch integrity fixture";
    let mut retained_bytes = 0;
    for (id, expires_at) in [
        (retained_id, Utc::now() + TimeDelta::minutes(5)),
        (expired_id, Utc::now() - TimeDelta::minutes(1)),
    ] {
        let reservation = runtime.reserve(id, 1024).unwrap();
        let paths = runtime.paths(id);
        let handle = handle(&request(), id, bytes, expires_at);
        std::fs::write(&paths.final_arrow, bytes).unwrap();
        write_metadata(&paths.final_metadata, &handle).unwrap();
        let total = bytes.len() as u64 + paths.final_metadata.metadata().unwrap().len();
        reservation.commit(total, expires_at).unwrap();
        if id == retained_id {
            retained_bytes = total;
        }
    }
    assert_eq!(runtime.stats().unwrap().files, 2);
    let next = runtime
        .reserve(
            RecordingProjectionReceiptId::new(),
            4096 - 128 - retained_bytes,
        )
        .unwrap();
    assert_eq!(runtime.stats().unwrap().files, 1);
    assert_eq!(runtime.stats().unwrap().committed_bytes, retained_bytes);
    assert!(!runtime.paths(expired_id).final_arrow.exists());
    assert!(!runtime.paths(expired_id).final_metadata.exists());
    assert!(runtime.paths(retained_id).final_arrow.is_file());
    assert!(runtime.paths(retained_id).final_metadata.is_file());
    assert_eq!(
        std::fs::read(active_paths.partial_arrow).unwrap(),
        b"active"
    );
    drop(next);
    assert_eq!(runtime.stats().unwrap().reserved_bytes, 128);
    drop(active);
    assert_eq!(runtime.stats().unwrap().reserved_bytes, 0);
}

#[test]
fn startup_discards_expired_corrupt_oversized_and_orphan_pairs() {
    #[derive(Clone, Copy, Debug)]
    enum Fault {
        Expired,
        Identity,
        Length,
        Digest,
        MetadataSize,
        MissingMetadata,
        MissingArrow,
    }
    for fault in [
        Fault::Expired,
        Fault::Identity,
        Fault::Length,
        Fault::Digest,
        Fault::MetadataSize,
        Fault::MissingMetadata,
        Fault::MissingArrow,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let initial = runtime(directory.path().to_path_buf(), 8192);
        let id = RecordingProjectionReceiptId::new();
        let paths = initial.paths(id);
        let bytes = b"scratch integrity fixture";
        let handle_id = if matches!(fault, Fault::Identity) {
            RecordingProjectionReceiptId::new()
        } else {
            id
        };
        let expires_at = Utc::now()
            + if matches!(fault, Fault::Expired) {
                -TimeDelta::minutes(1)
            } else {
                TimeDelta::minutes(5)
            };
        let handle = handle(&request(), handle_id, bytes, expires_at);
        std::fs::write(&paths.final_arrow, bytes).unwrap();
        write_metadata(&paths.final_metadata, &handle).unwrap();
        match fault {
            Fault::Expired | Fault::Identity => {}
            Fault::Length => std::fs::write(&paths.final_arrow, b"short").unwrap(),
            Fault::Digest => std::fs::write(&paths.final_arrow, vec![b'x'; bytes.len()]).unwrap(),
            Fault::MetadataSize => std::fs::write(
                &paths.final_metadata,
                vec![b' '; MAX_METADATA_BYTES as usize + 1],
            )
            .unwrap(),
            Fault::MissingMetadata => std::fs::remove_file(&paths.final_metadata).unwrap(),
            Fault::MissingArrow => std::fs::remove_file(&paths.final_arrow).unwrap(),
        }
        if matches!(fault, Fault::MetadataSize) {
            assert_eq!(
                read_metadata(&paths.final_metadata)
                    .unwrap_err()
                    .to_string(),
                "projection metadata exceeds its byte limit"
            );
        }
        drop(initial);
        let restarted = runtime(directory.path().to_path_buf(), 8192);
        assert_eq!(restarted.stats().unwrap().files, 0, "{fault:?}");
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            0,
            "{fault:?}"
        );
    }
}

#[test]
fn maximum_grid_and_metadata_fit_the_reserved_envelope() {
    let directory = tempfile::tempdir().unwrap();
    let initial = runtime(directory.path().to_path_buf(), 2 * 1024 * 1024);
    let mut wire = serde_json::to_value(request()).unwrap();
    let components: Vec<_> = (0..64)
        .map(|index| format!("{}:{index}", "s".repeat(250)))
        .collect();
    let units: std::collections::BTreeMap<_, _> = components
        .iter()
        .map(|name| (name.clone(), "\"".repeat(256)))
        .collect();
    wire["component_ids"] = json!(components);
    wire["maximum_columns"] = json!(64);
    wire["maximum_samples"] = json!(10000);
    wire["maximum_rows"] = json!(10000);
    wire["timeline"] = json!("x".repeat(1024));
    wire["sampling"] = json!({"kind": "sample_grid", "values": (1..=10000).map(|n| i64::MIN + n).collect::<Vec<_>>()});
    wire["units"] = json!(units);
    wire["coordinate_frame_refs"] = json!(vec!["\\".repeat(256); 64]);
    let request = serde_json::from_value::<CreateRecordingProjectionRequest>(wire).unwrap();
    let id = RecordingProjectionReceiptId::new();
    let paths = initial.paths(id);
    let bytes = b"integrity fixture";
    let handle = projection_handle(
        &request,
        id,
        "r".repeat(128),
        projection_query_digest(&request).unwrap(),
        Utc::now() + TimeDelta::minutes(5),
        ArrowProjectionSummary {
            row_count: 0,
            omitted_sample_count: 10000,
            schema_sha256: Sha256Digest::from_bytes([1; 32]),
            byte_len: (bytes.len() as u64).try_into().unwrap(),
            sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
        },
    )
    .unwrap();
    std::fs::write(&paths.final_arrow, bytes).unwrap();
    write_metadata(&paths.final_metadata, &handle).unwrap();
    assert!(paths.final_metadata.metadata().unwrap().len() <= MAX_METADATA_BYTES);
    let decoded = read_metadata(&paths.final_metadata).unwrap();
    decoded.validate_request(&request).unwrap();
    assert_eq!(decoded.result.sample_grid.len(), 10000);
    drop(initial);
    let restarted = runtime(directory.path().to_path_buf(), 2 * 1024 * 1024);
    assert_eq!(
        restarted.stats().unwrap().committed_bytes,
        bytes.len() as u64 + paths.final_metadata.metadata().unwrap().len()
    );
}

#[test]
fn arrow_result_survives_receipt_readback_and_rejects_changed_context() {
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;
    let directory = tempfile::tempdir().unwrap();
    let runtime = runtime(directory.path().join("scratch"), 2 * 1024 * 1024);
    let layer = directory.path().join("source.rrd");
    let source = RecordingStreamBuilder::new("projection-result")
        .recording_id("source")
        .save(&layer)
        .unwrap();
    for tick in 0..3 {
        source.set_time_sequence("tick", tick);
        source
            .log("/sensor", &Scalars::single(tick as f64))
            .unwrap();
    }
    source.flush_blocking().unwrap();
    drop(source);
    let request = request();
    let id = RecordingProjectionReceiptId::new();
    let paths = runtime.paths(id);
    let query = ArrowProjectionQuery::new(request.query.clone()).unwrap();
    let summary =
        veoveo_rrd::projection::write_arrow_projection(&[layer], &query, &paths.final_arrow)
            .unwrap();
    let handle = projection_handle(
        &request,
        id,
        "catalog-1".into(),
        projection_query_digest(&request).unwrap(),
        Utc::now() + TimeDelta::minutes(5),
        summary,
    )
    .unwrap();
    assert_eq!(handle.result.row_count, 3);
    write_metadata(&paths.final_metadata, &handle).unwrap();
    let mut receipt = receipt(&handle);
    read_handle(&paths, &receipt, &request, HandleReadMode::Ready).unwrap();
    receipt.state = RecordingProjectionState::Materializing;
    receipt.result_byte_len = None;
    receipt.result_sha256 = None;
    read_handle(&paths, &receipt, &request, HandleReadMode::Recovering).unwrap();
    assert!(read_handle(&paths, &receipt, &request, HandleReadMode::Ready).is_err());
    let wire = serde_json::to_value(&handle).unwrap();
    for (pointer, value) in [
        (
            "/expires_at",
            json!((handle.expires_at + TimeDelta::minutes(1)).to_rfc3339()),
        ),
        ("/result/catalog_revision", json!("another-catalog")),
        ("/result/query_digest", json!("b".repeat(64))),
        ("/result/timeline", json!("other-timeline")),
        ("/result/payload_sha256", json!("c".repeat(64))),
    ] {
        let mut changed = wire.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        std::fs::write(&paths.final_metadata, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(
            read_handle(&paths, &receipt, &request, HandleReadMode::Recovering).is_err(),
            "{pointer}"
        );
    }
    std::fs::write(&paths.final_metadata, serde_json::to_vec(&wire).unwrap()).unwrap();
    receipt.query_digest = "b".repeat(64);
    assert!(read_handle(&paths, &receipt, &request, HandleReadMode::Recovering).is_err());
}
