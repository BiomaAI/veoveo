use anyhow::ensure;
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::Scalars;
use url::Url;
use veoveo_recording_forwarder::{
    batch::RecordingAccumulator, client::RecordingIngestClient, config::ClientAssertionAlgorithm,
    oauth::OAuthTokenProvider,
};
use veoveo_recording_hub::{collect_segments, inspect_segment};
use veoveo_recording_protocol::v1::{OpenRecordingStreamRequest, RecordingStreamState};

use super::*;

pub(crate) async fn recording_ingest(
    conformance: &Path,
    gateway: &Path,
    hub: &Path,
    base_control_plane: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(gateway)?;
    assert_executable(hub)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("recording ingest smoke workspace: {}", tmpdir.display());

    let gateway_port = reserve_local_port()?;
    let hub_grpc_port = reserve_local_port()?;
    let hub_api_port = reserve_local_port()?;
    let gateway_base = format!("http://localhost:{gateway_port}");
    let gateway_transport_base = format!("http://127.0.0.1:{gateway_port}");
    let hub_base = format!("http://127.0.0.1:{hub_api_port}");
    let protected_resource = format!("{gateway_base}/ingest/recordings");
    let control_plane = tmpdir.join("gateway.recording-ingest.json");
    let producer_key = tmpdir.join("producer-key.pem");
    let spool_dir = tmpdir.join("recordings");
    let journal_dir = tmpdir.join("journal");
    let hub_ready = tmpdir.join("hub.ready");
    let hub_log = tmpdir.join("hub.log");
    let gateway_log = tmpdir.join("gateway.log");
    fs::create_dir_all(&spool_dir)?;
    fs::create_dir_all(&journal_dir)?;

    let source = fs::read_to_string(base_control_plane)?;
    let source = source
        .replace(PUBLIC_BASE_URL, &gateway_base)
        .replace("http://recording-hub:9878", &hub_base);
    fs::write(&control_plane, source)?;

    let private_key_der_b64 = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    fs::write(
        &producer_key,
        rsa_private_key_pem(private_key_der_b64.trim()).as_bytes(),
    )?;

    let platform = spawn_gateway_platform_store(gateway, &control_plane).await?;
    let mut hub_env = platform.runtime_env();
    hub_env.extend([
        ("RECORDING_TENANT_KEY", "tenant-a".into()),
        ("RECORDING_CLASSIFICATION", "internal".into()),
        (
            "RECORDING_INGEST_PROTECTED_RESOURCE",
            protected_resource.clone().into(),
        ),
        ("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()),
    ]);
    let mut hub_child = ChildGuard::spawn(
        hub,
        [
            "--bind".into(),
            format!("127.0.0.1:{hub_grpc_port}").into(),
            "--internal-ingest-bind".into(),
            format!("127.0.0.1:{hub_api_port}").into(),
            "--spool-dir".into(),
            spool_dir.as_os_str().to_os_string(),
            "--journal-dir".into(),
            journal_dir.as_os_str().to_os_string(),
            "--route".into(),
            "raw=smoke-sensor".into(),
            "--ready-file".into(),
            hub_ready.as_os_str().to_os_string(),
            "--counters-interval-s".into(),
            "60".into(),
        ],
        hub_env,
        &hub_log,
    )?;
    wait_for_file(&hub_ready).await?;

    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args_for_base(gateway_port, &platform, &gateway_base),
        [
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                private_key_der_b64.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::HOST,
        reqwest::header::HeaderValue::from_str(&format!("localhost:{gateway_port}"))?,
    );
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;
    let gateway_url = Url::parse(&format!("{gateway_base}/"))?;
    let gateway_transport_url = Url::parse(&format!("{gateway_transport_base}/"))?;
    let protected_resource_url = Url::parse(&protected_resource)?;
    let token_http = http.clone();
    let token_resource = protected_resource_url.clone();
    let token_key = producer_key.clone();
    let client = RecordingIngestClient::discover(
        http,
        &gateway_url,
        &gateway_transport_url,
        &protected_resource_url,
        move |token_endpoint, token_transport_endpoint| {
            OAuthTokenProvider::new(
                token_http,
                token_endpoint,
                token_transport_endpoint,
                token_resource,
                "smoke-recording-producer".to_owned(),
                "test-key".to_owned(),
                ClientAssertionAlgorithm::Rs256,
                &token_key,
            )
        },
    )
    .await?;

    let request = OpenRecordingStreamRequest {
        source_stream_id: uuid::Uuid::now_v7().to_string(),
        application_id: "smoke-sensor".to_owned(),
        recording_id: "external-smoke".to_owned(),
    };
    let opened = client.open(&request).await?;
    ensure!(
        opened.next_sequence == 1 && opened.state == i32::from(RecordingStreamState::Open),
        "new recording stream did not start at sequence one: {opened:?}"
    );

    let (recording, storage) = RecordingStreamBuilder::new(request.application_id.as_str())
        .recording_id(request.recording_id.clone())
        .memory()?;
    recording.log("sensor/value", &Scalars::single(42.0))?;
    let messages = storage.take();
    let store_id = messages
        .first()
        .context("Rerun memory sink emitted no store information")?
        .store_id()
        .clone();
    let mut accumulator = RecordingAccumulator::new(store_id)?;
    for message in messages {
        accumulator.push(message)?;
    }
    let mut batches = accumulator.drain_encoded(client.maximum_batch_bytes())?;
    ensure!(batches.len() == 1, "smoke recording unexpectedly split");
    let mut batch = batches.remove(0);
    batch.sequence = 1;

    let appended = client.append(&opened.stream_id, &batch).await?;
    ensure!(
        appended.durable_through_sequence == 1
            && appended.materialized_through_sequence == 1
            && !appended.duplicate,
        "first recording batch was not durably materialized: {appended:?}"
    );
    let duplicate = client.append(&opened.stream_id, &batch).await?;
    ensure!(
        duplicate.durable_through_sequence == 1
            && duplicate.materialized_through_sequence == 1
            && duplicate.duplicate,
        "idempotent recording retry was not acknowledged: {duplicate:?}"
    );

    let resumed = client.open(&request).await?;
    ensure!(
        resumed.stream_id == opened.stream_id && resumed.next_sequence == 2,
        "recording stream did not resume from its durable checkpoint: {resumed:?}"
    );
    let finished = client.finish(&opened.stream_id).await?;
    let finished = finished.stream.context("finish response omitted stream")?;
    ensure!(
        finished.state == i32::from(RecordingStreamState::Finished) && finished.next_sequence == 2,
        "recording stream did not finish at its durable checkpoint: {finished:?}"
    );

    let segments = collect_segments(&spool_dir)?;
    ensure!(
        segments.len() == 1,
        "expected one immutable segment, found {segments:?}"
    );
    let inspection = inspect_segment(&segments[0])?;
    ensure!(
        inspection.application_id == request.application_id
            && inspection.recording_key == request.recording_id
            && inspection.sha256 == hex::encode(&batch.sha256),
        "materialized segment changed recording identity or digest: {inspection:?}"
    );

    gateway_child.stop();
    hub_child.stop();
    cleanup.remove_on_drop();
    println!("recording ingest smoke ok: OAuth retry checkpoint materialized and resumed");
    Ok(())
}

fn rsa_private_key_pem(der_base64: &str) -> String {
    let mut pem = String::from("-----BEGIN RSA PRIVATE KEY-----\n");
    for chunk in der_base64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).expect("base64 is UTF-8"));
        pem.push('\n');
    }
    pem.push_str("-----END RSA PRIVATE KEY-----\n");
    pem
}
