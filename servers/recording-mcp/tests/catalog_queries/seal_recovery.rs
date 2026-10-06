//! Public current-format recovery over real RRD files and an isolated HTTP publisher.
use super::*;
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use sha2::{Digest as _, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use veoveo_artifact_contract::{ArtifactMetadata, StreamArtifactRequest};
use veoveo_platform_store::{ArtifactOccurrenceDraft, PlatformIdentity, PlatformStore};
use veoveo_recording_hub::{
    ClientAssertionAlgorithm, GatewayLayerPublisher, GatewayLayerPublisherConfig,
};
use veoveo_recording_reader::cache::LayerCacheLimits;
use veoveo_recording_store::{RecordingLayerId, RecordingLayerState, RecordingState};
use veoveo_types::{ScopeDefinition, Sha256Digest};

#[derive(Clone)]
struct PublisherState {
    store: PlatformStore,
    identity: PlatformIdentity,
    authority: veoveo_platform_store::InvocationAuthorityRecord,
    refuse: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<StreamArtifactRequest>>>,
}
struct HttpFixture {
    task: tokio::task::JoinHandle<()>,
    origin: url::Url,
    key: PathBuf,
}
impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl HttpFixture {
    async fn new(root: &Path, state: PublisherState) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin =
            url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let app = Router::new().route("/oauth/token", post(|| async { Json(serde_json::json!({"access_token":"isolated-recording-fixture","token_type":"Bearer","expires_in":300})) }))
            .route("/recordings/recovery/layers", post(publish)).with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let key = root.join("fixture-key.pem");
        std::fs::write(&key, "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEB\n-----END PRIVATE KEY-----\n").unwrap();
        Self { task, origin, key }
    }
    fn service(&self, store: PlatformStore, spool: &Path, cache: &Path) -> RecordingService {
        RecordingService::new(
            store,
            HttpArtifactPlane::new(self.origin.as_str()),
            spool.to_owned(),
        )
        .unwrap()
        .with_layer_cache(
            cache.to_owned(),
            LayerCacheLimits {
                managed_bytes: 16 * 1024 * 1024,
                minimum_free_bytes: 1,
            },
        )
        .unwrap()
        .with_layer_publisher(
            GatewayLayerPublisher::new(GatewayLayerPublisherConfig {
                gateway_url: self.origin.clone(),
                gateway_transport_url: None,
                protected_resource: self.origin.clone(),
                profile: "recovery".into(),
                client_id: "recording-recovery".into(),
                private_key_pem_file: self.key.clone(),
                key_id: "recording-recovery".into(),
                algorithm: ClientAssertionAlgorithm::EdDsa,
            })
            .unwrap(),
        )
    }
}
async fn publish(
    State(state): State<PublisherState>,
    headers: HeaderMap,
    bytes: Bytes,
) -> axum::response::Response {
    let request: StreamArtifactRequest = serde_json::from_str(
        headers
            .get("x-artifact-stream-put")
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .unwrap();
    state.requests.lock().unwrap().push(request.clone());
    if state.refuse.load(Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "definitively refused before persistence",
        )
            .into_response();
    }
    assert_eq!(bytes.len() as u64, request.expected_byte_len);
    assert_eq!(
        Sha256Digest::from_bytes(Sha256::digest(&bytes).into()).hex(),
        request.expected_sha256.as_str()
    );
    persist(&state, &request).await;
    Json(ArtifactMetadata {
        byte_len: request.expected_byte_len,
        artifact_uri: request.artifact_id.plane_uri(),
        mime_type: request.artifact.mime_type,
        filename: request.artifact.filename,
        download_url: None,
        created_at: Utc::now(),
        release_state: Default::default(),
        compliance: Default::default(),
        metadata: request.artifact.metadata,
    })
    .into_response()
}
async fn persist(state: &PublisherState, request: &StreamArtifactRequest) {
    let id = veoveo_platform_store::ArtifactId::from_uuid(request.artifact_id.as_uuid());
    if let Some(existing) = state.store.artifact_aggregate(id).await.unwrap() {
        assert_eq!(existing.blob.byte_len as u64, request.expected_byte_len);
        assert_eq!(existing.blob.sha256, request.expected_sha256.as_str());
        return;
    }
    state
        .store
        .create_artifact_occurrence(ArtifactOccurrenceDraft {
            artifact_id: id,
            identity: state.identity.clone(),
            authority: state.authority.clone(),
            owner: state.identity.principal_id.record_id(),
            initial_grants: Vec::new(),
            sha256: request.expected_sha256.as_str().into(),
            byte_len: request.expected_byte_len as i64,
            object_key: format!("recording-recovery/{id}"),
            media_type: request.artifact.mime_type.clone().unwrap(),
            filename: request.artifact.filename.clone(),
            classification: "restricted".into(),
            labels: vec!["restricted".into()],
            metadata: request
                .artifact
                .metadata
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            retention_expires_at: None,
        })
        .await
        .unwrap();
}
async fn recording(
    state: &PublisherState,
    caller: &GatewayInternalIdentity,
    dataset: RecordingDatasetId,
    root: &Path,
    key: &str,
) -> RecordingId {
    let repo = RecordingRepository::new(state.store.clone());
    let row = repo
        .create_recording(RecordingDraft {
            identity: state.identity.clone(),
            authority: veoveo_recording_hub::invocation_authority_record(&caller.authority),
            dataset_id: dataset,
            application_id: dataset.to_string(),
            recording_key: key.into(),
            classification: "restricted".into(),
            labels: vec!["restricted".into()],
            metadata: BTreeMap::new(),
            started_at: Utc::now() - TimeDelta::minutes(1),
        })
        .await
        .unwrap();
    let id = RecordingId::from_uuid(record_uuid(&row.id, "recording").unwrap());
    let layer = repo
        .open_recording_layer(
            RecordingLayerDraft::capture(state.identity.clone(), id, 0, format!("{key}.rrd"), None)
                .unwrap(),
        )
        .await
        .unwrap();
    let layer_id = RecordingLayerId::from_uuid(record_uuid(&layer.id, "recording_layer").unwrap());
    let path = root.join(format!("{key}.rrd"));
    let (stream, storage) = re_sdk::RecordingStreamBuilder::new("open upstream name")
        .recording_id("upstream recording")
        .memory()
        .unwrap();
    stream
        .log(
            "sensor/value",
            &re_sdk_types::archetypes::Scalars::single(42.0),
        )
        .unwrap();
    drop(stream);
    let mut encoder = re_log_encoding::rrd::Encoder::new_eager(
        re_log_encoding::rrd::CrateVersion::LOCAL,
        re_log_encoding::rrd::EncodingOptions::PROTOBUF_COMPRESSED,
        std::fs::File::create(&path).unwrap(),
    )
    .unwrap();
    for message in storage.take() {
        encoder.append(&message).unwrap();
    }
    encoder.finish().unwrap();
    drop(encoder);
    let inspected = veoveo_rrd::recording_layer::normalize_recording_layer(
        &path,
        crate_contract_dataset(dataset),
        veoveo_recording_contract::RecordingId::try_from(id.as_uuid()).unwrap(),
    )
    .unwrap();
    repo.stage_recording_layer(
        &state.identity,
        layer_id,
        inspected.byte_len as i64,
        inspected.message_count as i64,
        &inspected.sha256,
        Some(&inspected.rrd_version),
        Some(&inspected.schema_digest),
        Some(Utc::now()),
    )
    .await
    .unwrap();
    let request = StreamArtifactRequest {
        artifact_id: veoveo_artifact_contract::ArtifactId::try_from(layer_id.as_uuid()).unwrap(),
        artifact: veoveo_artifact_contract::PutArtifactRequest {
            mime_type: Some("application/vnd.rerun.rrd".into()),
            filename: Some(format!("{key}.rrd")),
            classification: None,
            data_labels: BTreeSet::new(),
            retention_expires_at: None,
            metadata: serde_json::json!({}),
        },
        expected_byte_len: inspected.byte_len,
        expected_sha256: veoveo_artifact_contract::UploadSha256::parse(inspected.sha256.hex())
            .unwrap(),
    };
    persist(state, &request).await;
    repo.commit_recording_layer(
        &state.identity,
        layer_id,
        veoveo_platform_store::ArtifactId::from_uuid(request.artifact_id.as_uuid()),
    )
    .await
    .unwrap();
    repo.finish_recording(&state.identity, id, Utc::now())
        .await
        .unwrap();
    id
}
fn crate_contract_dataset(id: RecordingDatasetId) -> veoveo_recording_contract::RecordingDatasetId {
    veoveo_recording_contract::RecordingDatasetId::try_from(id.as_uuid()).unwrap()
}

pub(super) async fn qualify(db: &fixture::TestDb) {
    let spool = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let mut caller = identity("recording-recovery", "producer", &["restricted"]);
    caller
        .actor
        .scopes
        .insert(RecordingScope::Seal.name().clone());
    let base = RecordingService::new(
        db.a.clone(),
        HttpArtifactPlane::new("http://127.0.0.1:1"),
        spool.path().into(),
    )
    .unwrap();
    let principal = base.platform_identity(&caller).await.unwrap();
    let repo = RecordingRepository::new(db.a.clone());
    let dataset = repo
        .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
            principal.clone(),
            "recovery",
        ))
        .await
        .unwrap();
    let dataset_id =
        RecordingDatasetId::from_uuid(record_uuid(&dataset.id, "recording_dataset").unwrap());
    let state = PublisherState {
        store: db.a.clone(),
        identity: principal.clone(),
        authority: veoveo_recording_hub::invocation_authority_record(&caller.authority),
        refuse: Arc::new(AtomicBool::new(true)),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let http = HttpFixture::new(spool.path(), state.clone()).await;
    let first = recording(&state, &caller, dataset_id, spool.path(), "staged-recovery").await;
    let service = http.service(db.b.clone(), spool.path(), cache.path());
    let source = repo
        .recording_layers(principal.tenant_id, first, 8)
        .await
        .unwrap()
        .remove(0);
    let ready = repo
        .recording(principal.tenant_id, first)
        .await
        .unwrap()
        .unwrap();
    for state_name in ["writing", "staged", "failed"] {
        db.a.client()
            .query(include_str!(
                "../queries/catalog_queries/seal_recovery/source_state.surql"
            ))
            .bind(("layer", source.id.clone()))
            .bind(("state", state_name.to_owned()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let before = repo
            .recording_layers(principal.tenant_id, first, 8)
            .await
            .unwrap();
        assert!(service.seal(&caller, first).await.is_err());
        assert_eq!(
            before,
            repo.recording_layers(principal.tenant_id, first, 8)
                .await
                .unwrap()
        );
        assert_eq!(
            ready,
            repo.recording(principal.tenant_id, first)
                .await
                .unwrap()
                .unwrap()
        );
        assert!(state.requests.lock().unwrap().is_empty());
    }
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/source_state.surql"
        ))
        .bind(("layer", source.id.clone()))
        .bind(("state", "committed".to_owned()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let error = service.seal(&caller, first).await.unwrap_err();
    assert!(
        error.to_string().contains("503"),
        "unexpected seal refusal: {error}"
    );
    drop(service);
    let first_record = repo
        .recording(principal.tenant_id, first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_record.state, RecordingState::Sealing);
    let properties = repo
        .recording_layer_by_name(principal.tenant_id, first, "properties")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(properties.state, RecordingLayerState::Staged);
    let first_request = state.requests.lock().unwrap()[0].clone();
    assert_eq!(
        first_request.artifact_id.as_uuid(),
        record_uuid(&properties.id, "recording_layer").unwrap()
    );
    let path = cache.path().join(properties.staging_path.as_ref().unwrap());
    let original = std::fs::read(&path).unwrap();
    let restarted = http.service(db.b.clone(), spool.path(), cache.path());
    // Both byte and schema-admitted native-stage contradictions fail locally;
    // no new publication, row rewrite or occurrence allocation follows.
    for corrupt_stage in [false, true] {
        if corrupt_stage {
            db.a.client()
                .query(include_str!(
                    "../queries/catalog_queries/seal_recovery/corrupt_stage.surql"
                ))
                .bind(("layer", properties.id.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
        } else {
            std::fs::write(&path, b"mismatched retained RRD").unwrap();
        }
        let before = repo
            .recording_layer(
                principal.tenant_id,
                RecordingLayerId::from_uuid(
                    record_uuid(&properties.id, "recording_layer").unwrap(),
                ),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(restarted.seal(&caller, first).await.is_err());
        assert_eq!(state.requests.lock().unwrap().len(), 1);
        assert_eq!(
            before,
            repo.recording_layer(
                principal.tenant_id,
                RecordingLayerId::from_uuid(
                    record_uuid(&properties.id, "recording_layer").unwrap()
                )
            )
            .await
            .unwrap()
            .unwrap()
        );
        assert_eq!(
            first_record,
            repo.recording(principal.tenant_id, first)
                .await
                .unwrap()
                .unwrap()
        );
        if corrupt_stage {
            db.a.client()
                .query(include_str!(
                    "../queries/catalog_queries/seal_recovery/restore_stage.surql"
                ))
                .bind(("layer", properties.id.clone()))
                .bind(("digest", properties.sha256.clone().unwrap()))
                .await
                .unwrap()
                .check()
                .unwrap();
        } else {
            assert_eq!(std::fs::read(&path).unwrap(), b"mismatched retained RRD");
            std::fs::write(&path, &original).unwrap();
        }
    }
    // Prepared facts must be admitted even when there is no final file to inspect.
    // Each corruption is storage-admitted and leaves the destination absent.
    std::fs::remove_file(&path).unwrap();
    for field in 0..5 {
        db.a.client()
            .query(include_str!(
                "../queries/catalog_queries/seal_recovery/stage_facts.surql"
            ))
            .bind(("layer", properties.id.clone()))
            .bind(("bytes", properties.byte_len + i64::from(field == 0)))
            .bind(("messages", properties.message_count + i64::from(field == 1)))
            .bind((
                "digest",
                if field == 2 {
                    "a".repeat(64)
                } else {
                    properties.sha256.clone().unwrap()
                },
            ))
            .bind((
                "schema",
                if field == 3 {
                    "b".repeat(64)
                } else {
                    properties.schema_digest.clone().unwrap()
                },
            ))
            .bind((
                "version",
                if field == 4 {
                    "contradictory-version".to_owned()
                } else {
                    properties.rrd_version.clone().unwrap()
                },
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        let before = repo
            .recording_layer_by_name(principal.tenant_id, first, "properties")
            .await
            .unwrap()
            .unwrap();
        assert!(restarted.seal(&caller, first).await.is_err());
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            0
        );
        assert_eq!(state.requests.lock().unwrap().len(), 1);
        assert_eq!(
            before,
            repo.recording_layer_by_name(principal.tenant_id, first, "properties")
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(
            first_record,
            repo.recording(principal.tenant_id, first)
                .await
                .unwrap()
                .unwrap()
        );
    }
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/stage_facts.surql"
        ))
        .bind(("layer", properties.id.clone()))
        .bind(("bytes", properties.byte_len))
        .bind(("messages", properties.message_count))
        .bind(("digest", properties.sha256.clone().unwrap()))
        .bind(("schema", properties.schema_digest.clone().unwrap()))
        .bind(("version", properties.rrd_version.clone().unwrap()))
        .await
        .unwrap()
        .check()
        .unwrap();
    // A valid missing file is rebuilt deterministically, then the definitive HTTP
    // refusal keeps the same stage and reserved occurrence for the next retry.
    assert!(restarted.seal(&caller, first).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(
        properties,
        repo.recording_layer_by_name(principal.tenant_id, first, "properties")
            .await
            .unwrap()
            .unwrap()
    );
    assert_eq!(
        first_record,
        repo.recording(principal.tenant_id, first)
            .await
            .unwrap()
            .unwrap()
    );
    {
        let requests = state.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            serde_json::to_value(&requests[0]).unwrap(),
            serde_json::to_value(&requests[1]).unwrap()
        );
    }
    for wrong_path in [false, true] {
        let end = if wrong_path {
            properties.end_time
        } else {
            properties.end_time.map(|time| time + TimeDelta::seconds(1))
        };
        let retained_path = if wrong_path {
            "properties/another-recording.rrd".to_owned()
        } else {
            properties.staging_path.clone().unwrap()
        };
        db.a.client()
            .query(include_str!(
                "../queries/catalog_queries/seal_recovery/stage_binding.surql"
            ))
            .bind(("layer", properties.id.clone()))
            .bind(("end", end))
            .bind(("path", retained_path))
            .await
            .unwrap()
            .check()
            .unwrap();
        let before = repo
            .recording_layer_by_name(principal.tenant_id, first, "properties")
            .await
            .unwrap()
            .unwrap();
        assert!(restarted.seal(&caller, first).await.is_err());
        assert_eq!(
            before,
            repo.recording_layer_by_name(principal.tenant_id, first, "properties")
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(state.requests.lock().unwrap().len(), 2);
        assert_eq!(
            first_record,
            repo.recording(principal.tenant_id, first)
                .await
                .unwrap()
                .unwrap()
        );
    }
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/stage_binding.surql"
        ))
        .bind(("layer", properties.id.clone()))
        .bind(("end", properties.end_time))
        .bind(("path", properties.staging_path.clone().unwrap()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let mut foreign = identity("foreign-recovery", "producer", &["restricted"]);
    foreign
        .actor
        .scopes
        .insert(RecordingScope::Seal.name().clone());
    assert!(restarted.seal(&foreign, first).await.is_err());
    assert_eq!(state.requests.lock().unwrap().len(), 2);
    state.refuse.store(false, Ordering::SeqCst);
    restarted.seal(&caller, first).await.unwrap();
    {
        let requests = state.requests.lock().unwrap();
        assert_eq!(
            serde_json::to_value(&requests[0]).unwrap(),
            serde_json::to_value(&requests[2]).unwrap()
        );
        assert_eq!(requests.len(), 4); // repeated properties, then reserved manifest
    }
    assert_eq!(
        repo.recording(principal.tenant_id, first)
            .await
            .unwrap()
            .unwrap()
            .state,
        RecordingState::Sealed
    );

    // A fixture-owned transactional fault interrupts the actual public producer
    // after writing the file but before staging. No manual row manufacture.
    let writing = recording(
        &state,
        &caller,
        dataset_id,
        spool.path(),
        "writing-recovery",
    )
    .await;
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/refuse_stage.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(restarted.seal(&caller, writing).await.is_err());
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/remove_refusal.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    let row = repo
        .recording(principal.tenant_id, writing)
        .await
        .unwrap()
        .unwrap();
    let layer = repo
        .recording_layer_by_name(principal.tenant_id, writing, "properties")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, RecordingState::Sealing);
    assert_eq!(layer.state, RecordingLayerState::Writing);
    let path = cache.path().join(layer.staging_path.as_ref().unwrap());
    let original = std::fs::read(&path).unwrap();
    let count = state.requests.lock().unwrap().len();
    std::fs::write(&path, b"wrong Writing bytes").unwrap();
    let resumed = http.service(db.b.clone(), spool.path(), cache.path());
    assert!(resumed.seal(&caller, writing).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"wrong Writing bytes");
    assert_eq!(
        repo.recording(principal.tenant_id, writing)
            .await
            .unwrap()
            .unwrap(),
        row
    );
    assert_eq!(
        repo.recording_layer_by_name(principal.tenant_id, writing, "properties")
            .await
            .unwrap()
            .unwrap(),
        layer
    );
    assert_eq!(state.requests.lock().unwrap().len(), count);
    std::fs::write(&path, &original).unwrap();
    resumed.seal(&caller, writing).await.unwrap();
    {
        let requests = state.requests.lock().unwrap();
        assert_eq!(
            requests[count].artifact_id.as_uuid(),
            record_uuid(&layer.id, "recording_layer").unwrap()
        );
        assert_eq!(
            requests[count].expected_sha256.as_str(),
            Sha256Digest::from_bytes(Sha256::digest(&original).into()).hex()
        );
    }
    assert_eq!(
        repo.recording(principal.tenant_id, writing)
            .await
            .unwrap()
            .unwrap()
            .state,
        RecordingState::Sealed
    );
}
