//! Public current-format recovery over real RRD files and an isolated HTTP publisher.
use super::*;
#[path = "seal_recovery/intent.rs"]
mod intent;
#[path = "seal_recovery/race.rs"]
mod race;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path as HttpPath, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
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
    caller_authority: veoveo_types::InvocationAuthority,
    refuse: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<StreamArtifactRequest>>>,
    objects: Arc<Mutex<BTreeMap<String, (ArtifactMetadata, Vec<u8>)>>>,
    revoke_read: Arc<AtomicBool>,
    refuse_manifest: Arc<AtomicBool>,
    lose_manifest_reply: Arc<AtomicBool>,
    corrupt_manifest_reply: Arc<AtomicBool>,
    manifest_attempts: Arc<Mutex<Vec<(StreamArtifactRequest, Vec<u8>)>>>,
}
struct HttpFixture {
    task: tokio::task::JoinHandle<()>,
    origin: url::Url,
    key: PathBuf,
}

/// Reuse the recovery fixture's Artifact admission and HTTP lifecycle for playback.
#[cfg(feature = "redap")]
pub(super) struct PlaybackFixture {
    http: HttpFixture,
    state: PublisherState,
}

#[cfg(feature = "redap")]
impl PlaybackFixture {
    pub(super) async fn new(
        db: &fixture::TestDb,
        caller: &GatewayInternalIdentity,
        root: &Path,
    ) -> Self {
        let service = RecordingService::new(
            db.a.clone(),
            HttpArtifactPlane::new("http://127.0.0.1:1"),
            root.into(),
        )
        .unwrap();
        let state = PublisherState {
            store: db.a.clone(),
            identity: service.platform_identity(caller).await.unwrap(),
            authority: veoveo_recording_hub::invocation_authority_record(&caller.authority),
            caller_authority: caller.authority.clone(),
            refuse: Arc::new(AtomicBool::new(false)),
            requests: Arc::new(Mutex::new(Vec::new())),
            objects: Arc::new(Mutex::new(BTreeMap::new())),
            revoke_read: Arc::new(AtomicBool::new(false)),
            refuse_manifest: Arc::new(AtomicBool::new(false)),
            lose_manifest_reply: Arc::new(AtomicBool::new(false)),
            corrupt_manifest_reply: Arc::new(AtomicBool::new(false)),
            manifest_attempts: Arc::new(Mutex::new(Vec::new())),
        };
        let http = HttpFixture::new(root, state.clone()).await;
        Self { http, state }
    }

    pub(super) fn service(
        &self,
        store: PlatformStore,
        root: &Path,
        cache: &Path,
    ) -> RecordingService {
        self.http.service(store, root, cache)
    }

    pub(super) fn deny_reads(&self, denied: bool) {
        self.state.revoke_read.store(denied, Ordering::SeqCst);
    }

    pub(super) async fn dataset(&self) -> RecordingDatasetId {
        let row = RecordingRepository::new(self.state.store.clone())
            .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
                self.state.identity.clone(),
                "redap-wire",
            ))
            .await
            .unwrap();
        RecordingDatasetId::from_uuid(record_uuid(&row.id, "recording_dataset").unwrap())
    }

    pub(super) async fn recording(
        &self,
        caller: &GatewayInternalIdentity,
        dataset: RecordingDatasetId,
        root: &Path,
        key: &str,
    ) -> (RecordingId, Vec<re_log_types::LogMsg>) {
        let id = recording(&self.state, caller, dataset, root, key).await;
        let bytes = std::fs::read(root.join(format!("{key}.rrd"))).unwrap();
        let layers = RecordingRepository::new(self.state.store.clone())
            .recording_layers(self.state.identity.tenant_id, id, 1)
            .await
            .unwrap();
        let layer_id =
            record_uuid(layers[0].artifact.as_ref().unwrap(), "artifact_occurrence").unwrap();
        let request = self
            .state
            .store
            .artifact_aggregate(veoveo_platform_store::ArtifactId::from_uuid(layer_id))
            .await
            .unwrap()
            .unwrap();
        let put = StreamArtifactRequest {
            artifact_id: veoveo_artifact_contract::ArtifactId::try_from(layer_id).unwrap(),
            artifact: veoveo_artifact_contract::PutArtifactRequest {
                mime_type: Some("application/vnd.rerun.rrd".into()),
                filename: Some(format!("{key}.rrd")),
                classification: None,
                data_labels: BTreeSet::new(),
                retention_expires_at: None,
                metadata: serde_json::json!({}),
            },
            expected_byte_len: bytes.len().try_into().unwrap(),
            expected_sha256: veoveo_artifact_contract::UploadSha256::parse(request.blob.sha256)
                .unwrap(),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-artifact-stream-put",
            serde_json::to_string(&put).unwrap().parse().unwrap(),
        );
        assert_eq!(
            publish(
                State(self.state.clone()),
                headers,
                Bytes::from(bytes.clone())
            )
            .await
            .status(),
            StatusCode::OK
        );
        let messages = re_log_encoding::Decoder::<re_log_types::LogMsg>::decode_eager(
            std::io::Cursor::new(bytes),
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
        (id, messages)
    }
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
            .route("/recordings/recovery/layers", post(publish))
            .route("/artifacts/{id}/meta", get(read_metadata))
            .route("/artifacts/{id}/download", get(read_body)).with_state(state.clone());
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
    let manifest = request.artifact.mime_type.as_deref()
        == Some("application/vnd.veoveo.recording-manifest+json");
    if manifest {
        state
            .manifest_attempts
            .lock()
            .unwrap()
            .push((request.clone(), bytes.to_vec()));
    }
    if state.refuse.load(Ordering::SeqCst)
        || (manifest && state.refuse_manifest.load(Ordering::SeqCst))
    {
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
    let mut metadata = ArtifactMetadata {
        byte_len: request.expected_byte_len,
        artifact_uri: request.artifact_id.plane_uri(),
        mime_type: request.artifact.mime_type.clone(),
        filename: request.artifact.filename.clone(),
        download_url: None,
        created_at: Utc::now(),
        release_state: Default::default(),
        compliance: veoveo_artifact_contract::ComplianceMetadata {
            classification: request.artifact.classification.clone(),
            data_labels: request.artifact.data_labels.clone(),
            retention_expires_at: request.artifact.retention_expires_at,
            tenant_id: Some(veoveo_types::TenantId::parse(&state.identity.tenant_key).unwrap()),
            owner: Some(state.caller_authority.output_policy.owner.clone()),
            work_context: Some(state.caller_authority.work_context.clone()),
            provenance: Some(veoveo_artifact_contract::ArtifactProvenance::new(
                veoveo_types::PrincipalId::parse(&state.identity.principal_key).unwrap(),
                state.caller_authority.provenance.clone(),
                state.caller_authority.policy_revision.clone(),
            )),
        },
        metadata: request.artifact.metadata,
    };
    state.objects.lock().unwrap().insert(
        request.artifact_id.to_string(),
        (metadata.clone(), bytes.to_vec()),
    );
    if manifest && state.lose_manifest_reply.load(Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "fixture interrupted after confirmed publication",
        )
            .into_response();
    }
    if manifest && state.corrupt_manifest_reply.load(Ordering::SeqCst) {
        metadata.compliance.classification =
            Some(veoveo_types::DataLabelId::parse("wrong-classification").unwrap());
    }
    Json(metadata).into_response()
}
fn authorized_read(state: &PublisherState, headers: &HeaderMap) -> bool {
    !state.revoke_read.load(Ordering::SeqCst)
        && headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            == Some("Bearer recording-fixture")
}
async fn read_metadata(
    State(state): State<PublisherState>,
    HttpPath(id): HttpPath<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !authorized_read(&state, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let objects = state.objects.lock().unwrap();
    match objects.get(&id) {
        Some((metadata, _)) => Json(metadata.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn read_body(
    State(state): State<PublisherState>,
    HttpPath(id): HttpPath<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !authorized_read(&state, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let objects = state.objects.lock().unwrap();
    match objects.get(&id) {
        Some((_, bytes)) => bytes.clone().into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
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
        caller_authority: caller.authority.clone(),
        refuse: Arc::new(AtomicBool::new(true)),
        requests: Arc::new(Mutex::new(Vec::new())),
        objects: Arc::new(Mutex::new(BTreeMap::new())),
        revoke_read: Arc::new(AtomicBool::new(false)),
        refuse_manifest: Arc::new(AtomicBool::new(false)),
        lose_manifest_reply: Arc::new(AtomicBool::new(false)),
        corrupt_manifest_reply: Arc::new(AtomicBool::new(false)),
        manifest_attempts: Arc::new(Mutex::new(Vec::new())),
    };
    let http = HttpFixture::new(spool.path(), state.clone()).await;
    let (first, restarted) = Box::pin(qualify_staged_properties_recovery(
        db,
        &state,
        &http,
        &caller,
        dataset_id,
        spool.path(),
        cache.path(),
    ))
    .await;
    Box::pin(intent::qualify(
        db,
        &state,
        &http,
        &caller,
        dataset_id,
        spool.path(),
        cache.path(),
    ))
    .await;

    Box::pin(race::qualify(
        db,
        &state,
        &http,
        &caller,
        dataset_id,
        spool.path(),
        cache.path(),
    ))
    .await;

    Box::pin(qualify_writing_properties_recovery(
        db,
        &state,
        &http,
        &caller,
        dataset_id,
        spool.path(),
        cache.path(),
        first,
        restarted,
    ))
    .await;
}

async fn qualify_staged_properties_recovery(
    db: &fixture::TestDb,
    state: &PublisherState,
    http: &HttpFixture,
    caller: &GatewayInternalIdentity,
    dataset_id: RecordingDatasetId,
    spool: &Path,
    cache: &Path,
) -> (RecordingId, RecordingService) {
    let principal = &state.identity;
    let repo = RecordingRepository::new(db.a.clone());
    let first = recording(&state, &caller, dataset_id, spool, "staged-recovery").await;
    let service = http.service(db.b.clone(), spool, cache);
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
        assert!(
            service
                .seal(&caller, &artifact_reader(&caller), first)
                .await
                .is_err()
        );
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
    let error = service
        .seal(&caller, &artifact_reader(&caller), first)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("503"),
        "unexpected seal refusal: {error}"
    );
    drop(service);
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/source_epoch.surql"
        ))
        .bind(("recording", first.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
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
    let preparation = properties.properties_preparation.clone().unwrap();
    assert_eq!(
        properties.end_time,
        Some(
            chrono::DateTime::parse_from_rfc3339(&preparation.body.sealed_at)
                .unwrap()
                .with_timezone(&Utc)
        )
    );
    assert!(preparation.body.source_revision < first_record.revision);
    for version in [0, 2] {
        let mut value = serde_json::to_value(&preparation).unwrap();
        value["version"] = serde_json::json!(version);
        assert!(<veoveo_recording_store::RecordingPropertiesPreparation as surrealdb::types::SurrealValue>::from_value(
            veoveo_platform_store::native_json_into_value(value)).is_err());
    }

    let count = state.requests.lock().unwrap().len();
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/properties_snapshot_digest.surql"
        ))
        .bind(("layer", properties.id.clone()))
        .bind(("digest", "f".repeat(64)))
        .await
        .unwrap()
        .check()
        .unwrap();
    let error = http
        .service(db.b.clone(), spool, cache)
        .seal(&caller, &artifact_reader(&caller), first)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("properties preparation differs from selected immutable source facts"),
        "{error}"
    );
    assert_eq!(count, state.requests.lock().unwrap().len());
    assert_eq!(
        first_record,
        repo.recording(principal.tenant_id, first)
            .await
            .unwrap()
            .unwrap()
    );
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/properties_snapshot_digest.surql"
        ))
        .bind(("layer", properties.id.clone()))
        .bind((
            "digest",
            preparation.body.immutable_manifest_digest.hex().to_owned(),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    let first_request = state.requests.lock().unwrap()[0].clone();
    assert_eq!(
        first_request.artifact_id.as_uuid(),
        record_uuid(&properties.id, "recording_layer").unwrap()
    );
    let path = cache.join(properties.staging_path.as_ref().unwrap());
    let original = std::fs::read(&path).unwrap();
    let restarted = http.service(db.b.clone(), spool, cache);
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
        assert!(
            restarted
                .seal(&caller, &artifact_reader(&caller), first)
                .await
                .is_err()
        );
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
        assert!(
            restarted
                .seal(&caller, &artifact_reader(&caller), first)
                .await
                .is_err()
        );
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
    assert!(
        restarted
            .seal(&caller, &artifact_reader(&caller), first)
            .await
            .is_err()
    );
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
        assert!(
            restarted
                .seal(&caller, &artifact_reader(&caller), first)
                .await
                .is_err()
        );
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
    assert!(
        restarted
            .seal(&foreign, &artifact_reader(&foreign), first)
            .await
            .is_err()
    );
    assert_eq!(state.requests.lock().unwrap().len(), 2);
    state.refuse.store(false, Ordering::SeqCst);
    restarted
        .seal(&caller, &artifact_reader(&caller), first)
        .await
        .unwrap();
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

    (first, restarted)
}

async fn qualify_writing_properties_recovery(
    db: &fixture::TestDb,
    state: &PublisherState,
    http: &HttpFixture,
    caller: &GatewayInternalIdentity,
    dataset_id: RecordingDatasetId,
    spool: &Path,
    cache: &Path,
    first: RecordingId,
    restarted: RecordingService,
) {
    let principal = &state.identity;
    let repo = RecordingRepository::new(db.a.clone());
    // A fixture-owned transactional fault interrupts the actual public producer
    // after writing the file but before staging. No manual row manufacture.
    let writing = recording(&state, &caller, dataset_id, spool, "writing-recovery").await;
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/refuse_stage.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        restarted
            .seal(&caller, &artifact_reader(&caller), writing)
            .await
            .is_err()
    );
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/remove_refusal.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/source_epoch.surql"
        ))
        .bind(("recording", writing.record_id()))
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
    let original_preparation = layer.properties_preparation.clone().unwrap();
    assert!(original_preparation.body.source_revision < row.revision);

    let path = cache.join(layer.staging_path.as_ref().unwrap());
    let original = std::fs::read(&path).unwrap();
    let count = state.requests.lock().unwrap().len();
    std::fs::write(&path, b"wrong Writing bytes").unwrap();
    let resumed = http.service(db.b.clone(), spool, cache);
    assert!(
        resumed
            .seal(&caller, &artifact_reader(&caller), writing)
            .await
            .is_err()
    );
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
    resumed
        .seal(&caller, &artifact_reader(&caller), writing)
        .await
        .unwrap();
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
    // Repeated sealing reads the stored public body under a fresh caller. Refusal
    // and contradictory bytes preserve the sealed row and issue no publication.
    let sealed = repo
        .recording(principal.tenant_id, writing)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        sealed.sealed_at,
        Some(
            chrono::DateTime::parse_from_rfc3339(&original_preparation.body.sealed_at)
                .unwrap()
                .with_timezone(&Utc)
        )
    );
    let publications = state.requests.lock().unwrap().len();
    let context = spool
        .join("recovery")
        .join(format!(".recording-{writing}.static-context"));
    std::fs::create_dir_all(context.parent().unwrap()).unwrap();
    std::fs::write(&context, b"owned cleanup sentinel").unwrap();
    resumed
        .seal(&caller, &artifact_reader(&caller), writing)
        .await
        .unwrap();
    assert!(!context.exists());
    std::fs::write(&context, b"owned cleanup sentinel").unwrap();
    state.revoke_read.store(true, Ordering::SeqCst);
    let denied = resumed
        .seal(&caller, &artifact_reader(&caller), writing)
        .await
        .unwrap_err();
    assert!(context.exists());
    assert!(
        denied.to_string().contains("denied or unavailable"),
        "{denied}"
    );
    state.revoke_read.store(false, Ordering::SeqCst);
    let manifest_id = writing.to_string();
    let original_manifest = state.objects.lock().unwrap()[&manifest_id].clone();
    for field in [
        "classification",
        "labels",
        "retention",
        "filename",
        "mime",
        "tenant",
        "context",
        "owner",
        "producer",
    ] {
        let mut changed = original_manifest.0.clone();
        match field {
            "classification" => {
                changed.compliance.classification = Some(DataLabelId::parse("wrong").unwrap())
            }
            "labels" => {
                changed
                    .compliance
                    .data_labels
                    .insert(DataLabelId::parse("wrong").unwrap());
            }
            "retention" => {
                changed.compliance.retention_expires_at = Some(Utc::now() + TimeDelta::hours(1))
            }
            "filename" => changed.filename = Some("wrong.recording-v10.json".into()),
            "mime" => changed.mime_type = Some("application/json".into()),
            "tenant" => changed.compliance.tenant_id = Some(TenantId::parse("wrong").unwrap()),
            "context" => {
                changed.compliance.work_context = Some(WorkContextId::parse("wrong").unwrap())
            }
            "owner" => {
                changed.compliance.owner = Some(AccessSubject::Principal(
                    PrincipalId::parse("wrong").unwrap(),
                ))
            }
            _ => {
                changed.compliance.provenance.as_mut().unwrap().producer =
                    PrincipalId::parse("wrong").unwrap()
            }
        }
        state
            .objects
            .lock()
            .unwrap()
            .insert(manifest_id.clone(), (changed, original_manifest.1.clone()));
        let error = resumed
            .seal(&caller, &artifact_reader(&caller), writing)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("metadata differs")
                || error
                    .to_string()
                    .contains("explicit publication descriptor")
                || error
                    .to_string()
                    .contains("selected native Artifact occurrence"),
            "{field}: {error}"
        );
        assert_eq!(
            sealed,
            repo.recording(principal.tenant_id, writing)
                .await
                .unwrap()
                .unwrap()
        );
        assert!(context.exists());
        assert_eq!(publications, state.requests.lock().unwrap().len());
    }
    state
        .objects
        .lock()
        .unwrap()
        .insert(manifest_id.clone(), original_manifest.clone());
    for mode in ["retired", "mixed", "context"] {
        let mut value: serde_json::Value = serde_json::from_slice(&original_manifest.1).unwrap();
        match mode {
            "retired" => value["schema"] = serde_json::json!("veoveo.ai/recording-manifest/v9"),
            "mixed" => value["recording_segment_id"] = value["recordingSegmentId"].clone(),
            _ => value["recordingSegmentId"] = serde_json::json!(first.to_string()),
        }
        // Immutable native digest is checked before any body vocabulary is trusted.
        state
            .objects
            .lock()
            .unwrap()
            .get_mut(&manifest_id)
            .unwrap()
            .1 = serde_json::to_vec_pretty(&value).unwrap();
        let error = resumed
            .seal(&caller, &artifact_reader(&caller), writing)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("length differs")
                || error.to_string().contains("immutable digest")
                || error.to_string().contains("selected length"),
            "{mode}: {error}"
        );
        assert_eq!(
            sealed,
            repo.recording(principal.tenant_id, writing)
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(publications, state.requests.lock().unwrap().len());
    }
    // Corrupting both remote bytes and native Artifact declarations cannot replace
    // the independently persisted Recording publication intent.
    let occurrence = veoveo_platform_store::ArtifactId::from_uuid(writing.as_uuid());
    let aggregate = db.a.artifact_aggregate(occurrence).await.unwrap().unwrap();
    for (mode, diagnostic) in [
        ("retired", "immutable publication intent"),
        ("mixed", "immutable publication intent"),
        ("context", "immutable publication intent"),
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(&original_manifest.1).unwrap();
        match mode {
            "retired" => value["schema"] = serde_json::json!("veoveo.ai/recording-manifest/v9"),
            "mixed" => value["recording_segment_id"] = value["recordingSegmentId"].clone(),
            _ => {
                *value.pointer_mut("/recordingSegmentId").unwrap() =
                    serde_json::json!(first.to_string())
            }
        }
        let body = serde_json::to_vec_pretty(&value).unwrap();
        let digest = Sha256Digest::from_bytes(Sha256::digest(&body).into())
            .hex()
            .to_owned();
        let mut metadata = original_manifest.0.clone();
        metadata.byte_len = body.len() as u64;
        *metadata.metadata.pointer_mut("/provenance/sha256").unwrap() = serde_json::json!(digest);
        let native_metadata: BTreeMap<String, serde_json::Value> =
            serde_json::from_value(metadata.metadata.clone()).unwrap();
        db.a.client()
            .query(include_str!(
                "../queries/catalog_queries/seal_recovery/manifest_facts.surql"
            ))
            .bind(("blob", aggregate.blob.id.clone()))
            .bind(("occurrence", aggregate.occurrence.id.clone()))
            .bind(("bytes", body.len() as i64))
            .bind(("digest", digest))
            .bind(("metadata", native_metadata))
            .await
            .unwrap()
            .check()
            .unwrap();
        state
            .objects
            .lock()
            .unwrap()
            .insert(manifest_id.clone(), (metadata, body));
        let error = resumed
            .seal(&caller, &artifact_reader(&caller), writing)
            .await
            .unwrap_err();
        assert!(error.to_string().contains(diagnostic), "{mode}: {error}");
        assert_eq!(
            sealed,
            repo.recording(principal.tenant_id, writing)
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(publications, state.requests.lock().unwrap().len());
    }
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/seal_recovery/manifest_facts.surql"
        ))
        .bind(("blob", aggregate.blob.id.clone()))
        .bind(("occurrence", aggregate.occurrence.id.clone()))
        .bind(("bytes", aggregate.blob.byte_len))
        .bind(("digest", aggregate.blob.sha256))
        .bind((
            "metadata",
            original_manifest
                .0
                .metadata
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>(),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    state
        .objects
        .lock()
        .unwrap()
        .insert(manifest_id, original_manifest);
    resumed
        .seal(&caller, &artifact_reader(&caller), writing)
        .await
        .unwrap();
    assert_eq!(
        repo.recording(principal.tenant_id, writing)
            .await
            .unwrap()
            .unwrap()
            .state,
        RecordingState::Sealed
    );
}
