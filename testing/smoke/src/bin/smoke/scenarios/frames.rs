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
    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--scheme".into(),
            "frames".into(),
            "--internal-server".into(),
            "media".into(),
            "info".into(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;

    let info = run_frames_mcp(conformance, &mcp_url, ["info".into()])?;
    for expected in [
        "server: frames",
        "tool `batch_transform`",
        "tool `convert_frame`",
        "tool `create_world`",
        "tool `publish_world`",
        "prompt `frames-frame-audit`",
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
    contains(&worlds, "[]")?;

    let prompt = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "prompt".into(),
            "frames-world-design".into(),
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
            r#"{"world_id":"smoke-world","display_name":"Smoke world","description":"Frames smoke world tree."}"#.into(),
        ],
    )?;
    contains(&create, "created frame world smoke-world")?;

    let publish = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "publish_world".into(),
            "--arguments".into(),
            r#"{"world_id":"smoke-world","tree":{"frames":[{"frame_id":"earth-ecef","basis":{"kind":"ecef_wgs84"}},{"frame_id":"launch-enu","basis":{"kind":"enu"},"parent_frame_id":"earth-ecef","parent_transform":{"kind":"geodetic_tangent","origin":{"latitude_degrees":37.4219999,"longitude_degrees":-122.0840575,"ellipsoid_height_m":10.0}}},{"frame_id":"robot-world","basis":{"kind":"enu"},"parent_frame_id":"launch-enu","parent_transform":{"kind":"static_rigid","translation_m":[0.0,0.0,0.0],"rotation_xyzw":[0.0,0.0,0.0,1.0]}}]}}"#.into(),
        ],
    )?;
    contains(&publish, "published frame world revision")?;
    let published: Value = structured_from_output(&publish)?;
    let revision_uri = published
        .pointer("/revision/revision_uri")
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
    contains(&frame, "\"frame_id\": \"robot-world\"")?;
    contains(&frame, "\"parent_frame_id\": \"launch-enu\"")?;

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
                    "frame_uri": robot_frame_uri,
                },
                "points": [{
                    "kind": "wgs84",
                    "latitude_degrees": 37.4220999,
                    "longitude_degrees": -122.0840575,
                    "ellipsoid_height_m": 12.0,
                }],
            }))?
            .into(),
        ],
    )?;
    contains(&convert, "converted 1 point(s)")?;
    let converted: Value = structured_from_output(&convert)?;
    assert_json_pointer_str(&converted, "/points/0/kind", "world_frame")?;
    assert_json_pointer_str(&converted, "/points/0/frame_uri", &robot_frame_uri)?;
    let operation_id = operation_id(&converted, "/provenance/operation/operation_id")?;
    let operation = run_frames_mcp(
        conformance,
        &mcp_url,
        [
            "resource".into(),
            format!("frames://operation/{operation_id}").into(),
        ],
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
            r#"{"artifact":true,"convert":{"target":{"kind":"ecef_wgs84"},"points":[{"kind":"wgs84","latitude_degrees":37.4219999,"longitude_degrees":-122.0840575,"ellipsoid_height_m":10.0}]}}"#.into(),
            "--task".into(),
        ],
    )?;
    let task_id = task_id_from_output(&batch)?;
    contains(&batch, "batch transform completed with 1 point(s)")?;
    contains(&batch, "output: frames://artifact/")?;
    let batch_output: SmokeFramesBatchOutput = structured_from_output(&batch)?;
    assert_json_pointer_str(&batch_output.result, "/points/0/kind", "ecef_wgs84")?;
    let artifact = batch_output
        .artifact
        .ok_or_else(|| anyhow!("batch output had no artifact metadata"))?;
    if artifact.artifact_uri != format!("frames://artifact/{}", artifact.artifact_id) {
        bail!(
            "batch artifact URI `{}` did not match artifact id `{}`",
            artifact.artifact_uri,
            artifact.artifact_id
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
            artifact.artifact_id.clone().into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
    )?;
    assert_output_file(&output_dir, "bin")?;

    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--scheme".into(),
            "frames".into(),
            "--internal-server".into(),
            "frames".into(),
            "--internal-principal-subject".into(),
            "intruder".into(),
            "--internal-work-context".into(),
            "intruder-context".into(),
            "artifact".into(),
            artifact.artifact_id.clone().into(),
            "--output-dir".into(),
            tmpdir.join("denied-intruder").as_os_str().to_os_string(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;
    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--scheme".into(),
            "frames".into(),
            "--internal-server".into(),
            "frames".into(),
            "--internal-tenant".into(),
            "other-tenant".into(),
            "artifact".into(),
            artifact.artifact_id.clone().into(),
            "--output-dir".into(),
            tmpdir
                .join("denied-cross-tenant")
                .as_os_str()
                .to_os_string(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;

    let usage = wait_for_actual_usage_for_scheme(conformance, &mcp_url, "frames", &task_id, None)?;
    if usage.usage_uri != format!("frames://usage/task/{task_id}") {
        bail!("frames usage URI was wrong: {usage:?}");
    }
    let actual = usage
        .records
        .iter()
        .find(|record| record.kind == SmokeUsageKind::Actual)
        .ok_or_else(|| anyhow!("usage report had no actual record: {usage:?}"))?;
    if actual.quantity != Some(1.0)
        || actual.unit.as_deref() != Some("point")
        || actual.amount.is_some()
        || actual.currency.is_some()
    {
        bail!("frames usage actual record had wrong shape: {usage:?}");
    }

    let post_run_resources = run_frames_mcp(conformance, &mcp_url, ["resources".into()])?;
    contains(
        &post_run_resources,
        &format!("frames://usage/task/{task_id}"),
    )?;
    not_contains(&post_run_resources, &artifact.artifact_uri)?;

    frames_child.stop();
    cleanup.remove_on_drop();
    println!("frames MCP smoke ok");
    Ok(())
}

fn run_frames_mcp(
    conformance: &Path,
    mcp_url: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let mut all_args = vec![
        "--scheme".into(),
        "frames".into(),
        "--internal-server".into(),
        "frames".into(),
    ];
    all_args.extend(args);
    run_direct_mcp(
        conformance,
        mcp_url,
        all_args,
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
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
