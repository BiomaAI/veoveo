use super::*;
pub(crate) async fn frames_mcp(
    conformance: &Path,
    frames: &Path,
    artifact_service: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(frames)?;
    assert_executable(artifact_service)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let port = 18809u16;
    let base = format!("http://127.0.0.1:{port}");
    let log = tmpdir.join("frames.log");
    let output_dir = tmpdir.join("outputs");

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut frames_child =
        spawn_frames_smoke(frames, port, &base, &plane.url, &plane.platform, &log)?;
    wait_for_http(&format!("{base}/frames/healthz")).await?;
    let health = reqwest::get(format!("{base}/frames/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;
    let untrusted_host_status = reqwest::Client::new()
        .get(format!("{base}/frames/healthz"))
        .header(HOST, "evil.example.com")
        .send()
        .await?
        .status();
    if untrusted_host_status != StatusCode::MISDIRECTED_REQUEST {
        bail!("frames untrusted Host status was {untrusted_host_status}, expected 421");
    }
    assert_json_log(
        &log,
        &[
            ("message", "listening"),
            ("service", "veoveo-frames-mcp"),
            ("mcp_path", "/frames/mcp"),
        ],
    )?;
    assert_http_status(&format!("{base}/frames/mcp"), StatusCode::UNAUTHORIZED).await?;
    assert_http_status(
        &format!(
            "{base}/frames/artifacts/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        StatusCode::NOT_FOUND,
    )
    .await?;

    let mcp_url = format!("{base}/frames/mcp");
    {
        let identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("media")?)?;

        let bearer = fixture_bearer(identity)?;
        assert_direct_mcp_denied(
            conformance,
            &mcp_url,
            ["--scheme".into(), "frames".into(), "info".into()],
            [("MCP_BEARER_TOKEN", bearer.into())],
            veoveo_mcp_conformance::client::failure::ObservedFailure::Http { status: 401 },
        )
    }?;

    let info = run_frames_mcp(conformance, &mcp_url, ["info".into()])?;
    for expected in [
        "server: frames",
        "tool `batch_transform`",
        "tool `convert_frame`",
        "tool `create_world`",
        "tool `publish_world`",
        "prompt `frames_frame_audit`",
        "template: frames://world/{world_id}",
        "template: frames://world/{world_id}/revision/{revision_id}/frame/{frame_id}",
        "template: frames://artifact/{artifact_id}",
    ] {
        contains(&info, expected)?;
    }

    let resources = run_frames_mcp(conformance, &mcp_url, ["resources".into()])?;
    for expected in ["frames://worlds", "frames://usage"] {
        contains(&resources, expected)?;
    }
    not_contains(&resources, "frames://world/smoke-world")?;

    let worlds = run_frames_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), "frames://worlds".into()],
    )?;
    let worlds: veoveo_frames_mcp::contract::FrameWorldPage = serde_json::from_str(&worlds)?;
    if !worlds.items.is_empty() || worlds.limit != 100 || worlds.next_cursor.is_some() {
        bail!("fresh Frames world catalog must return an empty bounded page");
    }

    let prompt = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "prompt".into(),
            "frames_world_design".into(),
            "--arguments".into(),
            r#"{"workflow":"UAV waypoint mission around a small survey site","earth_anchor_hint":"mission launch point"}"#
                .into(),
        ],
    )?;
    contains(&prompt, "complete rooted frame tree")?;

    let create = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "create_world".into(),
            "--arguments".into(),
            r#"{"worldId":"smoke-world","displayName":"Smoke world","description":"Frames smoke world tree."}"#.into(),
        ],
    )?;
    contains(&create, "created frame world smoke-world")?;
    let resources = run_frames_mcp(conformance, &mcp_url, ["resources".into()])?;
    not_contains(&resources, "frames://world/smoke-world")?;
    let worlds = run_frames_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), "frames://worlds".into()],
    )?;
    let worlds: veoveo_frames_mcp::contract::FrameWorldPage = serde_json::from_str(&worlds)?;
    if worlds.items.len() != 1 || worlds.items[0].world_id().as_str() != "smoke-world" {
        bail!("Frames world catalog must contain the authored world");
    }

    let publish = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "publish_world".into(),
            "--arguments".into(),
            serde_json::to_string(&veoveo_frames_mcp::contract::PublishWorldRequest {
                world_id: veoveo_frames_mcp::contract::FrameWorldId::parse("smoke-world")?,
                expected_head_revision_id: None,
                tree: installed_frames_tree()?,
            })?
            .into(),
        ],
    )?;
    contains(&publish, "published frame world revision")?;
    let published: Value = structured_from_output(&publish)?;
    let revision_uri = published
        .pointer("/revision/revisionUri")
        .and_then(Value::as_str)
        .context("published frame world omitted revision_uri")?;
    let robot_frame_uri = format!("{revision_uri}/frame/robot-world");

    let frame_completion = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "complete-resource".into(),
            "--uri".into(),
            "frames://world/{world_id}".into(),
            "--argument".into(),
            "world_id".into(),
            "smoke".into(),
        ],
    )?;
    contains(&frame_completion, "smoke-world")?;

    let frame = run_frames_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), robot_frame_uri.clone().into()],
    )?;
    contains(&frame, "\"frameId\": \"robot-world\"")?;
    contains(&frame, "\"parentFrameId\": \"launch-enu\"")?;

    let convert = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "convert_frame".into(),
            "--arguments".into(),
            serde_json::to_string(&serde_json::json!({
                "target": {
                    "kind": "world_frame",
                    "frameUri": robot_frame_uri,
                },
                "points": [{
                    "kind": "wgs84",
                    "latitudeDegrees": 37.4220999,
                    "longitudeDegrees": -122.0840575,
                    "ellipsoidHeightM": 12.0,
                }],
            }))?
            .into(),
        ],
    )?;
    contains(&convert, "converted 1 point(s)")?;
    let converted: Value = structured_from_output(&convert)?;
    assert_json_pointer_str(&converted, "/points/0/kind", "world_frame")?;
    assert_json_pointer_str(&converted, "/points/0/frameUri", &robot_frame_uri)?;
    let operation_uri = veoveo_frames_mcp::contract::FrameOperationUri::new(
        &veoveo_frames_mcp::contract::CoordinateOperationId::parse(operation_id(
            &converted,
            "/provenance/operation/operationId",
        )?)?,
    );
    let operation = run_frames_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), operation_uri.to_string().into()],
    )?;
    contains(&operation, "\"kind\": \"frame_conversion\"")?;
    contains(&operation, &robot_frame_uri)?;

    let batch = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "batch_transform".into(),
            "--arguments".into(),
            r#"{"artifact":true,"convert":{"target":{"kind":"ecef_wgs84"},"points":[{"kind":"wgs84","latitudeDegrees":37.4219999,"longitudeDegrees":-122.0840575,"ellipsoidHeightM":10.0}]}}"#.into(),
            "--task".into(),
        ],
    )?;
    let task_id = task_id_from_output(&batch)?;
    contains(&batch, "batch transform completed with 1 point(s)")?;
    contains(&batch, "output: frames://artifact/")?;
    let batch_output: SmokeFramesBatchOutput = structured_from_output(&batch)?;
    assert_json_pointer_str(&batch_output.result, "/points/0/kind", "ecef_wgs84")?;
    let batch_operation_uri = veoveo_frames_mcp::contract::FrameOperationUri::new(
        &veoveo_frames_mcp::contract::CoordinateOperationId::parse(operation_id(
            &batch_output.result,
            "/provenance/operation/operationId",
        )?)?,
    );
    let batch_operation = run_frames_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), batch_operation_uri.to_string().into()],
    )?;
    contains(&batch_operation, batch_operation_uri.as_str())?;

    let artifact = batch_output
        .artifact
        .ok_or_else(|| anyhow!("batch output had no artifact metadata"))?;
    if artifact.artifact_uri
        != veoveo_artifact_contract::ArtifactUri::presented(
            &veoveo_types::ResourceScheme::parse("frames")?,
            artifact.artifact_id(),
        )
    {
        bail!(
            "batch artifact URI `{}` did not match artifact id `{}`",
            artifact.artifact_uri,
            artifact.artifact_id()
        );
    }
    if artifact.metadata.get("task_id").and_then(Value::as_str) != Some(task_id.as_str()) {
        bail!("batch artifact metadata did not carry task id `{task_id}`: {artifact:?}");
    }

    run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "artifact".into(),
            artifact.artifact_id().to_string().into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
    )?;
    assert_output_file(&output_dir, "bin")?;

    {
        let mut identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("frames")?)?;
        identity.work_context = veoveo_types::WorkContextId::parse("intruder-context")?;
        identity.subject = veoveo_mcp_contract::TokenSubject::parse("intruder")?;
        let bearer = fixture_bearer(identity)?;
        assert_direct_mcp_denied(
            &veoveo_testing_support::artifacts::executable(
                "veoveo-artifact-service",
                "artifact-smoke",
            )?,
            &mcp_url,
            [
                "--scheme".into(),
                "frames".into(),
                "artifact".into(),
                artifact.artifact_id().to_string().into(),
                "--output-dir".into(),
                tmpdir.join("denied-intruder").as_os_str().to_os_string(),
            ],
            [("MCP_BEARER_TOKEN", bearer.into())],
            veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
                -32602,
                format!("unknown artifact `{}`", artifact.artifact_id()),
            ),
        )
    }?;
    {
        let mut identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("frames")?)?;
        identity.tenant = veoveo_types::TenantId::parse("other-tenant")?;
        let bearer = fixture_bearer(identity)?;
        assert_direct_mcp_denied(
            &veoveo_testing_support::artifacts::executable(
                "veoveo-artifact-service",
                "artifact-smoke",
            )?,
            &mcp_url,
            [
                "--scheme".into(),
                "frames".into(),
                "artifact".into(),
                artifact.artifact_id().to_string().into(),
                "--output-dir".into(),
                tmpdir
                    .join("denied-cross-tenant")
                    .as_os_str()
                    .to_os_string(),
            ],
            [("MCP_BEARER_TOKEN", bearer.into())],
            veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
                -32602,
                format!("unknown artifact `{}`", artifact.artifact_id()),
            ),
        )
    }?;

    let usage = wait_for_actual_usage_for_scheme(conformance, &mcp_url, "frames", &task_id, None)?;
    let usage_uri = veoveo_frames_mcp::contract::FrameTaskUsageUri::new(task_id.parse()?)?;
    if usage.usage_uri != usage_uri.as_str() {
        bail!("frames usage URI was wrong: {usage:?}");
    }
    let actual = usage
        .records
        .iter()
        .find(|record| record.kind == veoveo_mcp_contract::UsageKind::Actual)
        .ok_or_else(|| anyhow!("usage report had no actual record: {usage:?}"))?;
    if actual.quantity != Some(1.0)
        || actual.unit.as_deref() != Some("point")
        || actual.amount.is_some()
        || actual.currency.is_some()
    {
        bail!("frames usage actual record had wrong shape: {usage:?}");
    }

    let post_run_resources = run_frames_mcp(conformance, &mcp_url, ["resources".into()])?;
    not_contains(&post_run_resources, &usage.usage_uri)?;
    not_contains(&post_run_resources, artifact.artifact_uri.as_str())?;
    let usage_catalog = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "resource".into(),
            veoveo_frames_mcp::contract::FrameUsageIndexUri::ROOT.into(),
        ],
    )?;
    let usage_catalog: veoveo_frames_mcp::contract::FrameUsagePage =
        serde_json::from_str(&usage_catalog)?;
    if usage_catalog.items().len() != 1
        || usage_catalog.items()[0].usage_uri() != &usage_uri
        || usage_catalog.next_cursor().is_some()
    {
        bail!("Frames usage page did not contain the completed task: {usage_catalog:?}");
    }
    for identity in [
        {
            let mut identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("frames")?)?;
            identity.subject = veoveo_mcp_contract::TokenSubject::parse("intruder")?;
            identity.work_context = veoveo_types::WorkContextId::parse("intruder-context")?;
            identity
        },
        {
            let mut identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("frames")?)?;
            identity.tenant = veoveo_types::TenantId::parse("other-tenant")?;
            identity
        },
        {
            let mut identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("frames")?)?;
            identity.profile = veoveo_mcp_contract::GatewayProfileId::parse("observer")?;
            identity
        },
    ] {
        let bearer = fixture_bearer(identity)?;
        for uri in [
            usage_uri.as_str(),
            operation_uri.as_str(),
            batch_operation_uri.as_str(),
        ] {
            assert_direct_mcp_denied(
                conformance,
                &mcp_url,
                [
                    "--scheme".into(),
                    "frames".into(),
                    "resource".into(),
                    uri.into(),
                ],
                [("MCP_BEARER_TOKEN", bearer.clone().into())],
                veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
                    -32602,
                    if uri == usage_uri.as_str() {
                        format!("unknown usage task `{task_id}`")
                    } else {
                        format!("unknown operation `{uri}`")
                    },
                ),
            )?;
        }
    }
    frames_child.stop_checked().await?;
    cleanup.remove_on_drop();
    println!("frames MCP smoke ok");
    Ok(())
}

fn run_frames_mcp(
    conformance: &Path,
    mcp_url: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let bearer = fixture_bearer(fixture_identity(veoveo_mcp_contract::ServerSlug::parse(
        "frames",
    )?)?)?;
    let mut all_args = vec!["--scheme".into(), "frames".into()];
    all_args.extend(args);
    run_direct_mcp(
        conformance,
        mcp_url,
        all_args,
        [("MCP_BEARER_TOKEN", bearer.into())],
    )
}

fn assert_json_pointer_str(value: &Value, pointer: &str, expected: &str) -> Result<()> {
    if value.pointer(pointer).and_then(Value::as_str) == Some(expected) {
        Ok(())
    } else {
        bail!("JSON pointer `{pointer}` did not equal `{expected}`: {value}");
    }
}

fn operation_id<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("JSON pointer `{pointer}` was not a string: {value}"))
}

/// Installed qualification uses the same owner scenario and never replays a mutation.
pub(crate) async fn frames_installed(
    installation: &support::InstalledTarget,
    evidence: &Path,
) -> Result<()> {
    use rmcp::model::SubscriptionFilter;
    use veoveo_frames_mcp::contract::*;
    use veoveo_testing_support::connect_mcp_client;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    let mut evidence_file = admit_frames_evidence(evidence)?;
    let world_id = FrameWorldId::parse(format!("acceptance-{}", uuid::Uuid::new_v4()))?;
    let world_uri = FrameWorldUri::new(&world_id);
    let worlds_uri = FrameWorldsUri::new(None);
    let mut receipt = InstalledFramesReceipt {
        schema_version: "veoveo.ai/frames-installed-evidence/v1",
        world_id: world_id.clone(),
        revision_uri: None,
        outcome: InstalledFramesOutcome::FailedBeforeMutation,
        cleanup: InstalledFramesCleanup::NotOpened,
        retained_append_only: true,
    };
    write_frames_evidence(&mut evidence_file, &receipt)?;
    let token = tokio::time::timeout_at(frames_admission_deadline(deadline), installation.token())
        .await
        .map_err(|_| anyhow!("Frames OAuth admission deadline exceeded"))?
        .map_err(|_| anyhow!("Frames OAuth admission failed before mutation"))?;
    let client = tokio::time::timeout_at(
        frames_admission_deadline(deadline),
        connect_mcp_client(installation.operator.resource.as_str(), &token),
    )
    .await
    .map_err(|_| anyhow!("Frames MCP connection admission deadline exceeded"))?
    .map_err(|_| anyhow!("Frames MCP connection admission failed before mutation"))?;
    receipt.cleanup = InstalledFramesCleanup::Unresolved;
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions([worlds_uri.to_string()])
        .build();
    let mut subscription = match tokio::time::timeout_at(
        frames_admission_deadline(deadline),
        client.listen(filter.clone()),
    )
    .await
    {
        Ok(Ok(subscription)) => subscription,
        _ => {
            if matches!(
                tokio::time::timeout(Duration::from_secs(5), client.cancel()).await,
                Ok(Ok(_))
            ) {
                receipt.cleanup = InstalledFramesCleanup::ConnectionsClosed;
            }
            let _ = write_frames_evidence(&mut evidence_file, &receipt);
            bail!("installed Frames worlds subscription failed before mutation");
        }
    };
    let result = tokio::time::timeout_at(deadline, async {
        anyhow::ensure!(
            subscription.acknowledged() == &filter,
            "Frames subscription changed its filter"
        );
        // Listen sends a current resource invalidation baseline. Consume it before dispatch
        // so a baseline cannot masquerade as the authored change.
        let initial = tokio::time::timeout(Duration::from_secs(15), subscription.next())
            .await
            .context("Frames baseline wait exceeded fifteen seconds")?
            .map_err(|_| anyhow!("Frames baseline stream observation unresolved"))?
            .context("Frames subscription ended before baseline")?;
        require_worlds_invalidation(initial, worlds_uri.as_str())?;
        record_frames_mutation_intent(&mut evidence_file, &mut receipt)?;
        let created: CreateWorldOutput = installed_frames_call(
            &client,
            "create_world",
            CreateWorldRequest {
                world_id: world_id.clone(),
                display_name: "Installed acceptance world".into(),
                description: Some("Retained deterministic Frames acceptance fixture".into()),
            },
        )
        .await?;
        anyhow::ensure!(
            created.world.world_id() == world_id,
            "Frames created a different world"
        );
        let update = tokio::time::timeout(Duration::from_secs(15), subscription.next())
            .await
            .context("Frames authored invalidation wait exceeded fifteen seconds")?
            .map_err(|_| anyhow!("Frames authored invalidation stream observation unresolved"))?
            .context("Frames subscription ended before authored invalidation")?;
        require_worlds_invalidation(update, worlds_uri.as_str())?;
        let owned: FrameWorldSummary = installed_frames_read(&client, world_uri.as_str()).await?;
        anyhow::ensure!(
            owned.world_id() == world_id,
            "Frames read returned a different world"
        );
        let published: PublishWorldOutput = installed_frames_call(
            &client,
            "publish_world",
            PublishWorldRequest {
                world_id: world_id.clone(),
                expected_head_revision_id: None,
                tree: installed_frames_tree()?,
            },
        )
        .await?;
        anyhow::ensure!(
            published.created
                && published.world.world_id() == world_id
                && published.revision.world_id() == world_id,
            "Frames publication did not create the owned revision"
        );
        let revision_uri = published.revision.revision_uri().clone();
        receipt.revision_uri = Some(revision_uri.clone());
        write_frames_evidence(&mut evidence_file, &receipt)?;
        let revision: FrameWorldRevision =
            installed_frames_read(&client, revision_uri.as_str()).await?;
        anyhow::ensure!(
            revision == published.revision && revision.tree() == &installed_frames_tree()?,
            "Frames immutable revision differs from the published deterministic tree"
        );
        let immutable_filter = SubscriptionFilter::builder()
            .resource_subscriptions([revision_uri.to_string()])
            .build();
        match client.listen(immutable_filter).await {
            Err(rmcp::ServiceError::McpError(error))
                if error.code == rmcp::model::ErrorCode::INVALID_PARAMS => {}
            Ok(mut unexpected) => {
                let _ = tokio::time::timeout(Duration::from_secs(5), unexpected.cancel()).await;
                bail!("Frames admitted an immutable revision subscription");
            }
            Err(_) => bail!("Frames immutable subscription did not return invalid params"),
        }
        receipt.outcome = InstalledFramesOutcome::Passed;
        Ok::<(), anyhow::Error>(())
    })
    .await;
    let listener_closed = matches!(
        tokio::time::timeout(Duration::from_secs(5), subscription.cancel()).await,
        Ok(Ok(_))
    );
    let client_closed = matches!(
        tokio::time::timeout(Duration::from_secs(5), client.cancel()).await,
        Ok(Ok(_))
    );
    if listener_closed && client_closed {
        receipt.cleanup = InstalledFramesCleanup::ConnectionsClosed;
    }
    let written = write_frames_evidence(&mut evidence_file, &receipt);
    // Preserve the first domain/observation failure even if receipt persistence also fails.
    result.context("installed Frames acceptance exceeded ninety seconds")??;
    written?;
    anyhow::ensure!(
        listener_closed && client_closed,
        "Frames connection cleanup remains unresolved"
    );
    Ok(())
}

fn frames_admission_deadline(overall: tokio::time::Instant) -> tokio::time::Instant {
    overall.min(tokio::time::Instant::now() + Duration::from_secs(15))
}
fn admit_frames_evidence(path: &Path) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| anyhow!("Frames evidence requires a new writable private file"))
}
fn write_frames_evidence(file: &mut File, receipt: &InstalledFramesReceipt) -> Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    let bytes = serde_json::to_vec_pretty(receipt)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.set_len(bytes.len().try_into()?)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}
fn record_frames_mutation_intent(
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
) -> Result<()> {
    receipt.outcome = InstalledFramesOutcome::MutationUnresolved;
    write_frames_evidence(file, receipt)
}
fn require_complete_frames_response(
    response: rmcp::model::CallToolResponse,
) -> Result<rmcp::model::CallToolResult> {
    match response {
        rmcp::model::CallToolResponse::Complete(result) => Ok(result),
        _ => bail!("Frames mutation requires one complete response; outcome remains unresolved"),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledFramesReceipt {
    schema_version: &'static str,
    world_id: veoveo_frames_mcp::contract::FrameWorldId,
    revision_uri: Option<veoveo_frames_mcp::contract::FrameWorldRevisionUri>,
    outcome: InstalledFramesOutcome,
    cleanup: InstalledFramesCleanup,
    retained_append_only: bool,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum InstalledFramesOutcome {
    FailedBeforeMutation,
    MutationUnresolved,
    Passed,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum InstalledFramesCleanup {
    NotOpened,
    Unresolved,
    ConnectionsClosed,
}

fn require_worlds_invalidation(
    notification: rmcp::model::ServerNotification,
    expected: &str,
) -> Result<()> {
    match notification {
        rmcp::model::ServerNotification::ResourceUpdatedNotification(update)
            if update.params.uri == expected =>
        {
            Ok(())
        }
        _ => bail!("Frames delivered an unexpected resource notification"),
    }
}
fn installed_frames_tree() -> Result<veoveo_frames_mcp::contract::FrameWorldTree> {
    // The existing local owner fixture uses these same deterministic tree bytes.
    Ok(serde_json::from_str(
        r#"{"frames":[{"frameId":"earth-ecef","basis":{"kind":"ecef_wgs84"}},{"frameId":"launch-enu","basis":{"kind":"enu"},"parentFrameId":"earth-ecef","parentTransform":{"kind":"geodetic_tangent","origin":{"latitudeDegrees":37.4219999,"longitudeDegrees":-122.0840575,"ellipsoidHeightM":10.0}}},{"frameId":"robot-world","basis":{"kind":"enu"},"parentFrameId":"launch-enu","parentTransform":{"kind":"static_rigid","translationM":[0.0,0.0,0.0],"rotationXyzw":[0.0,0.0,0.0,1.0]}}]}"#,
    )?)
}
async fn installed_frames_call<T: serde::de::DeserializeOwned>(
    client: &veoveo_testing_support::SmokeMcpClient,
    name: &str,
    request: impl serde::Serialize,
) -> Result<T> {
    let local = veoveo_types::LocalToolName::parse(name)?;
    let name = veoveo_gateway_contract::GatewayToolName::from_parts(
        &veoveo_mcp_contract::ServerSlug::parse("frames")?,
        &local,
    )?;
    let response = client
        .call_tool_once(
            rmcp::model::CallToolRequestParams::new(name.to_string())
                .with_arguments(serde_json::from_value(serde_json::to_value(request)?)?),
        )
        .await
        .map_err(|_| anyhow!("Frames mutation response unresolved"))?;
    let response = require_complete_frames_response(response)?;
    anyhow::ensure!(
        response.is_error != Some(true),
        "Frames tool returned an error"
    );
    serde_json::from_value(
        response
            .structured_content
            .context("Frames response omitted structured content")?,
    )
    .map_err(|_| anyhow!("Frames mutation response failed owner admission"))
}
async fn installed_frames_read<T: serde::de::DeserializeOwned>(
    client: &veoveo_testing_support::SmokeMcpClient,
    uri: &str,
) -> Result<T> {
    use rmcp::model::{ReadResourceRequestParams, ResourceContents};
    let result = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await
        .map_err(|_| anyhow!("installed Frames resource read failed"))?;
    let [
        ResourceContents::TextResourceContents {
            uri: returned,
            text,
            ..
        },
    ] = result.contents.as_slice()
    else {
        bail!("Frames resource must return one JSON body");
    };
    anyhow::ensure!(returned == uri, "Frames resource returned a different URI");
    serde_json::from_str(text).map_err(|_| anyhow!("Frames resource failed owner admission"))
}

#[cfg(test)]
mod installed_frames_tests {
    use super::*;
    #[test]
    fn evidence_admission_refuses_occupied_symlink_and_unwritable_destinations() -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir()?;
        let occupied = directory.path().join("occupied.json");
        fs::write(&occupied, b"retained-private-sentinel")?;
        assert!(admit_frames_evidence(&occupied).is_err());
        let link = directory.path().join("symlink.json");
        symlink(&occupied, &link)?;
        assert!(admit_frames_evidence(&link).is_err());
        assert!(admit_frames_evidence(&occupied.join("unwritable.json")).is_err());
        assert!(
            admit_frames_evidence(Path::new("/proc/self/frames-acceptance-receipt.json")).is_err()
        );
        assert_eq!(fs::read(&occupied)?, b"retained-private-sentinel");
        let path = directory.path().join("receipt.json");
        let mut handle = admit_frames_evidence(&path)?;
        assert_eq!(handle.metadata()?.permissions().mode() & 0o777, 0o600);
        let reserved = directory.path().join("reserved.json");
        fs::rename(&path, &reserved)?;
        symlink(&occupied, &path)?;
        handle.write_all(b"owned-receipt")?;
        handle.flush()?;
        assert_eq!(fs::read(reserved)?, b"owned-receipt");
        assert_eq!(fs::read(occupied)?, b"retained-private-sentinel");
        Ok(())
    }
    #[test]
    fn one_dispatch_refuses_input_required_without_using_request_state() -> Result<()> {
        use rmcp::model::{CallToolResponse, CallToolResult, InputRequiredResult};
        let input = CallToolResponse::InputRequired(InputRequiredResult::from_request_state(
            "private-never-redispatched-state",
        ));
        let error = require_complete_frames_response(input).unwrap_err();
        assert!(
            !error
                .to_string()
                .contains("private-never-redispatched-state")
        );
        require_complete_frames_response(CallToolResponse::Complete(CallToolResult::default()))?;
        Ok(())
    }
    #[tokio::test]
    async fn admission_budget_is_capped_by_overall_deadline() {
        let expired = tokio::time::Instant::now() - Duration::from_secs(1);
        assert_eq!(frames_admission_deadline(expired), expired);
        assert!(
            tokio::time::timeout_at(
                frames_admission_deadline(expired),
                std::future::pending::<()>()
            )
            .await
            .is_err()
        );
        let overall = tokio::time::Instant::now() + Duration::from_secs(1);
        assert_eq!(frames_admission_deadline(overall), overall);
    }
    #[test]
    fn persisted_intent_precedes_dispatch_and_retains_observed_revision() -> Result<()> {
        use veoveo_frames_mcp::contract::{FrameWorldId, FrameWorldRevisionId, FrameWorldUri};
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("receipt.json");
        let mut file = admit_frames_evidence(&path)?;
        let world = FrameWorldId::parse("acceptance-intent")?;
        let mut receipt = InstalledFramesReceipt {
            schema_version: "veoveo.ai/frames-installed-evidence/v1",
            world_id: world.clone(),
            revision_uri: None,
            outcome: InstalledFramesOutcome::FailedBeforeMutation,
            cleanup: InstalledFramesCleanup::NotOpened,
            retained_append_only: true,
        };
        write_frames_evidence(&mut file, &receipt)?;
        record_frames_mutation_intent(&mut file, &mut receipt)?;
        let persisted: Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(persisted["outcome"], "mutation_unresolved");
        let revision = FrameWorldUri::new(&world).revision(&FrameWorldRevisionId::parse("rev-1")?);
        receipt.revision_uri = Some(revision.clone());
        write_frames_evidence(&mut file, &receipt)?;
        let persisted: Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(persisted["revisionUri"], revision.to_string());
        assert_eq!(persisted["outcome"], "mutation_unresolved");
        let mut read_only = File::open(&path)?;
        let dispatched = record_frames_mutation_intent(&mut read_only, &mut receipt).is_ok();
        assert!(
            !dispatched,
            "failed intent persistence must refuse dispatch"
        );
        assert_eq!(fs::read(&path)?, serde_json::to_vec_pretty(&receipt)?);
        Ok(())
    }
    #[test]
    fn worlds_notification_requires_exact_owned_collection() -> Result<()> {
        use rmcp::model::{
            ResourceUpdatedNotification, ResourceUpdatedNotificationParam, ServerNotification,
        };
        let update = |uri: &str| {
            ServerNotification::ResourceUpdatedNotification(ResourceUpdatedNotification::new(
                ResourceUpdatedNotificationParam::new(uri),
            ))
        };
        require_worlds_invalidation(update("frames://worlds"), "frames://worlds")?;
        for uri in [
            "frames://world/other",
            "frames://worlds?cursor=foreign",
            "view://views",
        ] {
            assert!(require_worlds_invalidation(update(uri), "frames://worlds").is_err());
        }
        Ok(())
    }
    #[test]
    fn installed_fixture_tree_and_receipt_preserve_owner_context() -> Result<()> {
        use veoveo_frames_mcp::contract::{FrameWorldId, FrameWorldUri};
        let world = FrameWorldId::parse("acceptance-fixture")?;
        let revision = FrameWorldUri::new(&world).revision(
            &veoveo_frames_mcp::contract::FrameWorldRevisionId::parse("rev-1")?,
        );
        assert_eq!(revision.world_id(), world);
        assert_eq!(installed_frames_tree()?.frames.len(), 3);
        let receipt = InstalledFramesReceipt {
            schema_version: "veoveo.ai/frames-installed-evidence/v1",
            world_id: world,
            revision_uri: Some(revision),
            outcome: InstalledFramesOutcome::MutationUnresolved,
            cleanup: InstalledFramesCleanup::ConnectionsClosed,
            retained_append_only: true,
        };
        let value = serde_json::to_value(receipt)?;
        assert_eq!(value["outcome"], "mutation_unresolved");
        assert_eq!(value["cleanup"], "connections_closed");
        assert_eq!(value["retainedAppendOnly"], true);
        assert!(value.get("world_id").is_none());
        Ok(())
    }
}
