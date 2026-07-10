use super::*;

pub(crate) async fn coordinates_mcp(
    conformance: &Path,
    coordinates: &Path,
    artifact_service: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(coordinates)?;
    assert_executable(artifact_service)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let port = 18809u16;
    let base = format!("http://127.0.0.1:{port}");
    let log = tmpdir.join("coordinates.log");
    let output_dir = tmpdir.join("outputs");

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut coordinates_child =
        spawn_coordinates_smoke(coordinates, port, &base, &plane.url, &plane.platform, &log)?;
    wait_for_http(&format!("{base}/coordinates/healthz")).await?;
    let health = reqwest::get(format!("{base}/coordinates/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;
    let untrusted_host_status = reqwest::Client::new()
        .get(format!("{base}/coordinates/healthz"))
        .header(HOST, "evil.example.com")
        .send()
        .await?
        .status();
    if untrusted_host_status != StatusCode::MISDIRECTED_REQUEST {
        bail!("coordinates untrusted Host status was {untrusted_host_status}, expected 421");
    }
    assert_json_log(
        &log,
        &[
            ("message", "listening"),
            ("service", "veoveo-coordinates-mcp"),
            ("mcp_path", "/coordinates/mcp"),
        ],
    )?;
    assert_http_status(&format!("{base}/coordinates/mcp"), StatusCode::UNAUTHORIZED).await?;
    assert_http_status(
        &format!(
            "{base}/coordinates/artifacts/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        StatusCode::NOT_FOUND,
    )
    .await?;

    let mcp_url = format!("{base}/coordinates/mcp");
    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--scheme".into(),
            "coordinates".into(),
            "--internal-server".into(),
            "media".into(),
            "info".into(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;

    let info = run_coordinates_mcp(conformance, &mcp_url, ["info".into()])?;
    for expected in [
        "server: coordinates",
        "tool `batch_transform`",
        "tool `convert_frame`",
        "tool `derive_local_frame`",
        "tool `geodesic_inverse`",
        "tool `transform_crs`",
        "tool `validate_geofence`",
        "prompt `coordinates-frame-audit`",
        "template: coordinates://frame/{frame_id}",
        "template: coordinates://crs/{authority}/{code}",
        "template: coordinates://artifact/{artifact_id}",
    ] {
        contains(&info, expected)?;
    }

    let resources = run_coordinates_mcp(conformance, &mcp_url, ["resources".into()])?;
    for expected in [
        "coordinates://frames",
        "coordinates://crs",
        "coordinates://usage",
        "coordinates://frame/WGS84",
        "coordinates://frame/ECEF",
    ] {
        contains(&resources, expected)?;
    }

    let frames = run_coordinates_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), "coordinates://frames".into()],
    )?;
    contains(&frames, "\"frame_id\": \"WGS84\"")?;
    contains(
        &frames,
        "\"axis_convention\": \"latitude_longitude_height\"",
    )?;

    let crs = run_coordinates_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), "coordinates://crs/EPSG/4326".into()],
    )?;
    contains(&crs, "\"crs\": \"EPSG:4326\"")?;
    contains(&crs, "\"engine\": \"PROJ\"")?;

    let frame_completion = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "complete-resource".into(),
            "--uri".into(),
            "coordinates://frame/{frame_id}".into(),
            "--argument".into(),
            "frame_id".into(),
            "WG".into(),
        ],
    )?;
    contains(&frame_completion, "WGS84")?;

    let crs_completion = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "complete-resource".into(),
            "--uri".into(),
            "coordinates://crs/{authority}/{code}".into(),
            "--argument".into(),
            "code".into(),
            "43".into(),
        ],
    )?;
    contains(&crs_completion, "4326")?;

    let prompt = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "prompt".into(),
            "coordinates-local-frame-select".into(),
            "--arguments".into(),
            r#"{"workflow":"UAV waypoint mission around a small survey site","origin_hint":"mission launch point"}"#
                .into(),
        ],
    )?;
    contains(&prompt, "derive_local_frame request")?;

    let derive = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "derive_local_frame".into(),
            "--arguments".into(),
            r#"{"frame_id":"ENU:smoke","kind":"enu","origin":{"latitude_deg":37.4219999,"longitude_deg":-122.0840575,"height_m":10.0},"description":"smoke local tangent frame"}"#.into(),
        ],
    )?;
    contains(&derive, "derived frame ENU:smoke")?;
    let derived: Value = structured_from_output(&derive)?;
    assert_json_pointer_str(&derived, "/frame/frame_id", "ENU:smoke")?;
    assert_json_pointer_str(&derived, "/frame/axis_convention", "east_north_up")?;

    let convert = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "convert_frame".into(),
            "--arguments".into(),
            r#"{"target_frame":"ENU:smoke","points":[{"kind":"wgs84","latitude_deg":37.4220999,"longitude_deg":-122.0840575,"height_m":12.0}]}"#.into(),
        ],
    )?;
    contains(&convert, "converted 1 point(s)")?;
    let converted: Value = structured_from_output(&convert)?;
    assert_json_pointer_str(&converted, "/points/0/kind", "enu")?;
    assert_json_pointer_str(&converted, "/points/0/frame_id", "ENU:smoke")?;
    let operation_id = operation_id(&converted, "/provenance/operation/operation_id")?;
    let operation = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "resource".into(),
            format!("coordinates://operation/{operation_id}").into(),
        ],
    )?;
    contains(&operation, "\"kind\": \"frame_conversion\"")?;
    contains(&operation, "\"target_frame\": \"ENU:smoke\"")?;

    let transform = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "transform_crs".into(),
            "--arguments".into(),
            r#"{"source_crs":"EPSG:4326","target_crs":"EPSG:3857","points":[{"crs":"EPSG:4326","x":-122.0840575,"y":37.4219999}]}"#.into(),
        ],
    )?;
    contains(&transform, "transformed 1 point(s)")?;
    let transformed: Value = structured_from_output(&transform)?;
    assert_json_pointer_str(&transformed, "/points/0/crs", "EPSG:3857")?;
    let mercator_x = transformed
        .pointer("/points/0/x")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("transform output had no numeric x: {transformed}"))?;
    if !(-13_600_000.0..-13_500_000.0).contains(&mercator_x) {
        bail!("unexpected Web Mercator x coordinate {mercator_x}: {transformed}");
    }

    let inverse = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "geodesic_inverse".into(),
            "--arguments".into(),
            r#"{"start":{"latitude_deg":37.4219999,"longitude_deg":-122.0840575,"height_m":10.0},"end":{"latitude_deg":37.4229999,"longitude_deg":-122.0840575,"height_m":10.0}}"#.into(),
        ],
    )?;
    let inverse: Value = structured_from_output(&inverse)?;
    let distance = inverse
        .get("distance_m")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("geodesic inverse output had no distance_m: {inverse}"))?;
    if !(110.0..112.0).contains(&distance) {
        bail!("unexpected geodesic distance {distance}: {inverse}");
    }

    let direct = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "geodesic_direct".into(),
            "--arguments".into(),
            r#"{"start":{"latitude_deg":37.4219999,"longitude_deg":-122.0840575,"height_m":10.0},"initial_azimuth_deg":0.0,"distance_m":100.0}"#.into(),
        ],
    )?;
    let direct: Value = structured_from_output(&direct)?;
    let direct_lat = direct
        .pointer("/end/latitude_deg")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("geodesic direct output had no latitude: {direct}"))?;
    if direct_lat <= 37.4219999 {
        bail!("geodesic direct did not move north: {direct}");
    }

    let geofence = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "validate_geofence".into(),
            "--arguments".into(),
            r#"{"geofence":{"frame_id":"ENU:smoke","rule":"must_stay_inside","polygon":{"exterior":{"coordinates":[[-100.0,-100.0],[100.0,-100.0],[100.0,100.0],[-100.0,100.0],[-100.0,-100.0]]}}},"path":{"coordinates":[[0.0,0.0],[120.0,0.0]]}}"#.into(),
        ],
    )?;
    contains(&geofence, "geofence invalid with 1 violation(s)")?;
    let geofence: Value = structured_from_output(&geofence)?;
    if geofence.get("valid").and_then(Value::as_bool) != Some(false) {
        bail!("geofence output was not invalid: {geofence}");
    }

    let batch = run_coordinates_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "batch_transform".into(),
            "--arguments".into(),
            r#"{"artifact":true,"convert":{"target_frame":"ECEF","points":[{"kind":"wgs84","latitude_deg":37.4219999,"longitude_deg":-122.0840575,"height_m":10.0}]}}"#.into(),
            "--task".into(),
        ],
    )?;
    let task_id = task_id_from_output(&batch)?;
    contains(&batch, "batch transform completed with 1 point(s)")?;
    contains(&batch, "output: coordinates://artifact/")?;
    let batch_output: SmokeCoordinatesBatchOutput = structured_from_output(&batch)?;
    assert_json_pointer_str(&batch_output.result, "/points/0/kind", "ecef")?;
    let artifact = batch_output
        .artifact
        .ok_or_else(|| anyhow!("batch output had no artifact metadata"))?;
    if artifact.artifact_uri != format!("coordinates://artifact/{}", artifact.artifact_id) {
        bail!(
            "batch artifact URI `{}` did not match artifact id `{}`",
            artifact.artifact_uri,
            artifact.artifact_id
        );
    }
    if artifact.metadata.get("task_id").and_then(Value::as_str) != Some(task_id.as_str()) {
        bail!("batch artifact metadata did not carry task id `{task_id}`: {artifact:?}");
    }

    run_coordinates_mcp(
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
            "coordinates".into(),
            "--internal-server".into(),
            "coordinates".into(),
            "--internal-principal-subject".into(),
            "intruder".into(),
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
            "coordinates".into(),
            "--internal-server".into(),
            "coordinates".into(),
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

    let usage =
        wait_for_actual_usage_for_scheme(conformance, &mcp_url, "coordinates", &task_id, None)?;
    if usage.usage_uri != format!("coordinates://usage/task/{task_id}") {
        bail!("coordinates usage URI was wrong: {usage:?}");
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
        bail!("coordinates usage actual record had wrong shape: {usage:?}");
    }

    let post_run_resources = run_coordinates_mcp(conformance, &mcp_url, ["resources".into()])?;
    contains(
        &post_run_resources,
        &format!("coordinates://usage/task/{task_id}"),
    )?;
    not_contains(&post_run_resources, &artifact.artifact_uri)?;

    coordinates_child.stop();
    cleanup.remove_on_drop();
    println!("coordinates MCP smoke ok");
    Ok(())
}

fn run_coordinates_mcp(
    conformance: &Path,
    mcp_url: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let mut all_args = vec![
        "--scheme".into(),
        "coordinates".into(),
        "--internal-server".into(),
        "coordinates".into(),
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
