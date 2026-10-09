use super::*;
#[path = "frames/immutable_subscription.rs"]
mod immutable_subscription;
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

fn installed_frames_tree() -> Result<veoveo_frames_mcp::contract::FrameWorldTree> {
    // The existing local owner fixture uses these same deterministic tree bytes.
    Ok(serde_json::from_str(
        r#"{"frames":[{"frameId":"earth-ecef","basis":{"kind":"ecef_wgs84"}},{"frameId":"launch-enu","basis":{"kind":"enu"},"parentFrameId":"earth-ecef","parentTransform":{"kind":"geodetic_tangent","origin":{"latitudeDegrees":37.4219999,"longitudeDegrees":-122.0840575,"ellipsoidHeightM":10.0}}},{"frameId":"robot-world","basis":{"kind":"enu"},"parentFrameId":"launch-enu","parentTransform":{"kind":"static_rigid","translationM":[0.0,0.0,0.0],"rotationXyzw":[0.0,0.0,0.0,1.0]}}]}"#,
    )?)
}

#[path = "frames/installed.rs"]
mod installed;
pub(crate) use installed::frames_installed;

pub(crate) use installed::recovery::run as frames_recovery;
