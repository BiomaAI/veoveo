//! Finish explicitly selected smoke recordings through the producer's normal API.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use veoveo_platform_store::{RecordIdKey, deterministic_tenant_id};
use veoveo_recording_forwarder::{
    client::RecordingIngestClient,
    config::ClientAssertionAlgorithm,
    oauth::{OAuthTokenProvider, OAuthTokenProviderConfig},
};
use veoveo_recording_protocol::v1::RecordingStreamFinishMode;

use super::stream::{
    PortForwardGuard, kubernetes_namespace, load_environment, optional_environment,
    recording_producer_key, recording_store, required_environment,
};

pub(crate) async fn recording_fixture_finish(
    env_file: &Path,
    producer_key_secret: &str,
    stream_ids: &[uuid::Uuid],
) -> Result<()> {
    ensure!(
        !stream_ids.is_empty(),
        "select at least one smoke ingest stream ID"
    );
    let environment = load_environment(env_file)?;
    let namespace = kubernetes_namespace(&environment);
    let temporary = tempfile::tempdir()?;
    let key = recording_producer_key(namespace, producer_key_secret, temporary.path())?;
    let gateway = url::Url::parse(required_environment(&environment, "PUBLIC_BASE_URL")?)?;
    let resource = gateway.join("/ingest/recordings")?;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let client_id = optional_environment(
        &environment,
        "VEOVEO_RECORDING_PRODUCER_CLIENT_ID",
        "recording-producer",
    )
    .to_owned();
    let key_id = required_environment(&environment, "VEOVEO_RECORDING_PRODUCER_KEY_ID")?.to_owned();
    let client = RecordingIngestClient::discover(
        http.clone(),
        &gateway,
        &gateway,
        &resource,
        |token_endpoint, token_transport_endpoint| {
            OAuthTokenProvider::new(OAuthTokenProviderConfig {
                http,
                token_endpoint,
                token_transport_endpoint,
                protected_resource: resource.clone(),
                client_id,
                scope: "recording:ingest".to_owned(),
                key_id,
                algorithm: ClientAssertionAlgorithm::Rs256,
                private_key_pem_file: key.path().to_owned(),
            })
        },
    )
    .await?;
    let _forward = PortForwardGuard::spawn(namespace, "surrealdb", 8000, 8000)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::net::TcpStream::connect("127.0.0.1:8000")
        .await
        .is_err()
    {
        ensure!(
            tokio::time::Instant::now() < deadline,
            "catalog port-forward did not start"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let store = recording_store(&environment).await?;
    let tenant =
        deterministic_tenant_id(required_environment(&environment, "RECORDING_TENANT_KEY")?)?;
    for stream_id in stream_ids {
        let stream_id = veoveo_platform_store::RecordingIngestStreamId::from_uuid(*stream_id);
        let stream = store
            .recording_ingest_stream(tenant, stream_id)
            .await?
            .context("selected ingest stream does not exist")?;
        let recording_id = match &stream.recording.key {
            RecordIdKey::Uuid(id) => **id,
            RecordIdKey::String(id) => uuid::Uuid::parse_str(id)?,
            _ => anyhow::bail!("ingest stream has no recording UUID"),
        };
        let recording = store
            .recording(
                tenant,
                veoveo_platform_store::RecordingId::from_uuid(recording_id),
            )
            .await?
            .context("selected stream has no recording")?;
        ensure!(
            recording.application_id == "veoveo-video-test",
            "selected stream is not a video-test fixture"
        );
        let id = match &stream.id.key {
            RecordIdKey::Uuid(id) => id.to_string(),
            RecordIdKey::String(id) => id.clone(),
            _ => anyhow::bail!("ingest stream has no UUID identity"),
        };
        client
            .finish(&id, RecordingStreamFinishMode::CompleteRecording)
            .await?;
        println!("finished smoke recording {recording_id}, stream {id}");
    }
    println!(
        "finished {} selected smoke ingest streams",
        stream_ids.len()
    );
    Ok(())
}
