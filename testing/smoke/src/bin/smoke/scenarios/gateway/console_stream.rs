use std::collections::BTreeMap;

use chrono::Utc;
use futures::StreamExt;
use secrecy::SecretString;
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, GrantPermission, InvocationAuthorityRecord, InvocationMode,
    PlatformIdentity, PlatformStore, PrincipalKind, RecordingDraft, StoreConfig, StoreCredentials,
    WorkContextInitialGrantRecord, WorkContextMembershipLevel,
};

use super::*;

const STREAM_FRAME_TIMEOUT: Duration = Duration::from_secs(30);

fn recording_authority(identity: &PlatformIdentity) -> InvocationAuthorityRecord {
    InvocationAuthorityRecord {
        context_key: "operations".to_owned(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: "r1".to_owned(),
        owner_kind: ArtifactGrantSubjectKind::Principal,
        owner_key: identity.principal_key.clone(),
        initial_grants: vec![WorkContextInitialGrantRecord {
            subject_kind: ArtifactGrantSubjectKind::Principal,
            subject_key: identity.principal_key.clone(),
            permission: GrantPermission::Admin,
        }],
        classification: None,
        data_labels: Vec::new(),
        invocation_mode: InvocationMode::Automated,
        initiator_key: None,
        delegation_id: None,
    }
}

pub(crate) async fn gateway_console_stream(
    conformance: &Path,
    gateway: &Path,
    base_control_plane: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let gateway_port = 18831u16;
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let gateway_log = tmpdir.join("gateway.log");

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let platform_store = spawn_gateway_platform_store(gateway, base_control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &platform_store),
        [
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;

    let admin_token = gateway_token_for_profile(
        conformance,
        &gateway_base,
        "admin",
        &["--scope", "operator:use", "--scope", "admin:manage"],
    )?;
    let admin_token = admin_token.trim();
    let http = reqwest::Client::new();

    // 1. The snapshot must hand out a versionstamp stream cursor.
    let snapshot = get_json(
        &http,
        &format!("{gateway_base}/admin/admin/console/snapshot"),
        Some(admin_token),
    )
    .await?;
    let cursor = snapshot
        .pointer("/stream/cursor")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("snapshot carried no stream cursor: {snapshot}"))?;
    if cursor.parse::<i64>().is_err() {
        bail!("stream cursor is not a versionstamp: {cursor}");
    }
    let tenant_key = snapshot
        .pointer("/session/tenantId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("snapshot carried no session tenant"))?
        .to_owned();

    // 2. Open the SSE stream with the snapshot cursor.
    let stream_url = format!("{gateway_base}/admin/admin/console/stream");
    let response = http
        .get(&stream_url)
        .query(&[("cursor", cursor)])
        .bearer_auth(admin_token)
        .send()
        .await?;
    if response.status() != reqwest::StatusCode::OK {
        bail!("console stream returned {}", response.status());
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    if !content_type.starts_with("text/event-stream") {
        bail!("console stream content type was {content_type}");
    }
    let mut frames = response.bytes_stream();

    // The initial server-health frames arrive before any row events.
    let opening = read_stream_until(&mut frames, "event: server").await?;
    contains(&opening, "event: server")?;

    // 3. A live store mutation must surface as a recording upsert frame.
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &platform_store.endpoint,
            &platform_store.namespace,
            &platform_store.database,
            StoreCredentials::database(
                SURREAL_RUNTIME_USER,
                SecretString::from(SURREAL_RUNTIME_PASSWORD),
            ),
        )
        .build()?,
    )
    .await?;
    let identity = store
        .ensure_identity(
            &tenant_key,
            "console-stream-smoke",
            "https://veoveo.local/smoke",
            "console-stream-smoke",
            PrincipalKind::Service,
        )
        .await?;
    let first_key = format!("console-stream-{}", uuid::Uuid::now_v7().simple());
    store
        .create_recording(RecordingDraft {
            identity: identity.clone(),
            authority: recording_authority(&identity),
            dataset: "smoke".to_owned(),
            application_id: "console-stream-smoke".to_owned(),
            recording_key: first_key.clone(),
            classification: "internal".to_owned(),
            labels: Vec::new(),
            metadata: BTreeMap::new(),
            started_at: Utc::now(),
        })
        .await?;
    let live_frames = read_stream_until(&mut frames, &first_key).await?;
    contains(&live_frames, "event: recording")?;
    contains(&live_frames, r#""op":"upsert""#)?;
    let last_event_id = last_sse_id(&format!("{opening}{live_frames}"))
        .ok_or_else(|| anyhow!("no SSE id observed before disconnect"))?;
    drop(frames);

    // 4. A mutation made while disconnected must replay on reconnect with
    //    Last-Event-ID.
    let second_key = format!("console-stream-{}", uuid::Uuid::now_v7().simple());
    store
        .create_recording(RecordingDraft {
            identity: identity.clone(),
            authority: recording_authority(&identity),
            dataset: "smoke".to_owned(),
            application_id: "console-stream-smoke".to_owned(),
            recording_key: second_key.clone(),
            classification: "internal".to_owned(),
            labels: Vec::new(),
            metadata: BTreeMap::new(),
            started_at: Utc::now(),
        })
        .await?;
    let reconnect = http
        .get(&stream_url)
        .header("last-event-id", last_event_id.clone())
        .bearer_auth(admin_token)
        .send()
        .await?;
    if reconnect.status() != reqwest::StatusCode::OK {
        bail!("console stream reconnect returned {}", reconnect.status());
    }
    let mut reconnect_frames = reconnect.bytes_stream();
    let replayed = read_stream_until(&mut reconnect_frames, &second_key).await?;
    contains(&replayed, "event: recording")?;
    drop(reconnect_frames);

    // 5. The per-principal connection cap returns 429, not a queued stream.
    let mut held = Vec::new();
    let mut limited = false;
    for _ in 0..4 {
        let response = http
            .get(&stream_url)
            .query(&[("cursor", cursor)])
            .bearer_auth(admin_token)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            limited = true;
            break;
        }
        if response.status() != reqwest::StatusCode::OK {
            bail!("console stream limit probe returned {}", response.status());
        }
        held.push(response);
    }
    if !limited {
        bail!("per-principal stream cap did not trigger 429");
    }
    drop(held);

    gateway_child.stop();
    cleanup.remove_on_drop();
    println!("gateway console stream smoke ok");
    Ok(())
}

async fn read_stream_until(
    frames: &mut (impl futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Unpin),
    needle: &str,
) -> Result<String> {
    let mut collected = String::new();
    let deadline = tokio::time::Instant::now() + STREAM_FRAME_TIMEOUT;
    loop {
        let chunk = tokio::select! {
            () = tokio::time::sleep_until(deadline) => {
                bail!("timed out waiting for `{needle}` in console stream; saw:\n{collected}");
            }
            chunk = frames.next() => chunk,
        };
        match chunk {
            Some(Ok(bytes)) => {
                collected.push_str(&String::from_utf8_lossy(&bytes));
                if collected.contains(needle) {
                    return Ok(collected);
                }
            }
            Some(Err(error)) => bail!("console stream errored: {error}"),
            None => bail!("console stream ended before `{needle}`; saw:\n{collected}"),
        }
    }
}

fn last_sse_id(frames: &str) -> Option<String> {
    frames
        .lines()
        .filter_map(|line| line.strip_prefix("id:"))
        .map(|id| id.trim().to_owned())
        .next_back()
}
