use super::*;

const SMOKE_CSV: &str = "city,population,elevation_m,region\nQuito,2800000,2850,sierra\nGuayaquil,3100000,4,costa\nCuenca,640000,2560,sierra\nLoja,290000,2060,sierra\nManta,310000,6,costa\n";

/// Direct hosted smoke for the Python template server: auth boundary, full MCP
/// surface, the final task extension, artifact-plane output, and usage.
pub(crate) async fn datasheet_mcp(conformance: &Path, artifact_service: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(artifact_service)?;

    let template_dir = Path::new("templates/python-mcp");
    if !template_dir.is_dir() {
        bail!("datasheet smoke must run from the repository root");
    }
    run_checked(
        Path::new("uv"),
        [
            "sync".into(),
            "--project".into(),
            template_dir.as_os_str().to_os_string(),
        ],
        [],
    )?;
    let datasheet_bin = template_dir.join(".venv/bin/datasheet-mcp");
    assert_executable(&datasheet_bin)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let port = 18811u16;
    let base = format!("http://127.0.0.1:{port}");
    let log = tmpdir.join("datasheet.log");
    let output_dir = tmpdir.join("outputs");

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut datasheet_child = spawn_datasheet_smoke(
        &datasheet_bin,
        port,
        &base,
        &plane.url,
        &plane.platform,
        &log,
    )?;
    wait_for_http(&format!("{base}/datasheet/readyz")).await?;
    let health = reqwest::get(format!("{base}/datasheet/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;
    let untrusted_host_status = reqwest::Client::new()
        .get(format!("{base}/datasheet/healthz"))
        .header(HOST, "evil.example.com")
        .send()
        .await?
        .status();
    if untrusted_host_status != StatusCode::MISDIRECTED_REQUEST {
        bail!("datasheet untrusted Host status was {untrusted_host_status}, expected 421");
    }
    assert_json_log(
        &log,
        &[
            ("message", "listening"),
            ("service", "veoveo-datasheet-mcp"),
            ("mcp_path", "/datasheet/mcp"),
        ],
    )?;
    assert_http_status(&format!("{base}/datasheet/mcp"), StatusCode::UNAUTHORIZED).await?;

    let mcp_url = format!("{base}/datasheet/mcp");
    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--scheme".into(),
            "datasheet".into(),
            "--internal-server".into(),
            "media".into(),
            "info".into(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;

    let info = run_datasheet_mcp(conformance, &mcp_url, ["info".into()])?;
    for expected in [
        "server: datasheet",
        "tool `column_stats`",
        "tool `preview_dataset`",
        "tool `profile_dataset`",
        "prompt `datasheet-profile-dataset`",
        "prompt `datasheet-report-review`",
        "template: datasheet://usage/task/{task_id}",
        "template: datasheet://artifact/{artifact_id}",
    ] {
        contains(&info, expected)?;
    }

    let resources = run_datasheet_mcp(conformance, &mcp_url, ["resources".into()])?;
    contains(&resources, "datasheet://reports")?;
    contains(&resources, "datasheet://usage")?;

    let prompt = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "prompt".into(),
            "datasheet-profile-dataset".into(),
            "--arguments".into(),
            r#"{"dataset_uri":"artifact://01900000-0000-7000-8000-000000000001"}"#.into(),
        ],
    )?;
    contains(&prompt, "profile_dataset")?;

    let preview_args = serde_json::json!({"inline_csv": SMOKE_CSV, "rows": 3}).to_string();
    let preview = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "preview_dataset".into(),
            "--arguments".into(),
            preview_args.into(),
        ],
    )?;
    contains(&preview, "previewed 3 of 5 row(s)")?;
    let previewed: Value = structured_from_output(&preview)?;
    if previewed.pointer("/row_count").and_then(Value::as_i64) != Some(5) {
        bail!("preview output had wrong row_count: {previewed}");
    }
    if previewed.pointer("/columns/1/name").and_then(Value::as_str) != Some("population") {
        bail!("preview output had wrong column order: {previewed}");
    }

    let stats_args = serde_json::json!({"inline_csv": SMOKE_CSV, "column": "region"}).to_string();
    let stats = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "column_stats".into(),
            "--arguments".into(),
            stats_args.into(),
        ],
    )?;
    let stats: Value = structured_from_output(&stats)?;
    if stats.pointer("/distinct_count").and_then(Value::as_i64) != Some(2) {
        bail!("column_stats output had wrong distinct_count: {stats}");
    }
    if stats.pointer("/top_values/0/value").and_then(Value::as_str) != Some("sierra") {
        bail!("column_stats output had wrong top value: {stats}");
    }

    // The task-required tool must reject direct invocation with an in-band
    // tool error and no structured output.
    let direct_args = serde_json::json!({"inline_csv": SMOKE_CSV}).to_string();
    let rejected = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "profile_dataset".into(),
            "--arguments".into(),
            direct_args.into(),
        ],
    )?;
    contains(&rejected, "profile_dataset requires task-based invocation")?;
    not_contains(&rejected, "structured:")?;

    let profile_args = serde_json::json!({"inline_csv": SMOKE_CSV, "artifact": true}).to_string();
    let profiled = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "task-call".into(),
            "--tool-name".into(),
            "profile_dataset".into(),
            "--arguments".into(),
            profile_args.into(),
        ],
    )?;
    let task_id = task_id_from_output(&profiled)?;
    contains(&profiled, "profiled 5 row(s) across 4 column(s)")?;
    contains(&profiled, "output: datasheet://artifact/")?;
    let profile_output: Value = structured_from_output(&profiled)?;
    if profile_output
        .pointer("/profile/row_count")
        .and_then(Value::as_i64)
        != Some(5)
    {
        bail!("profile output had wrong row_count: {profile_output}");
    }
    if profile_output
        .pointer("/profile/columns/2/histogram/0/count")
        .and_then(Value::as_i64)
        .is_none()
    {
        bail!("profile output had no elevation histogram: {profile_output}");
    }
    let artifact_id = profile_output
        .pointer("/artifact/artifact_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("profile output had no artifact id: {profile_output}"))?
        .to_string();
    let artifact_uri = profile_output
        .pointer("/artifact/artifact_uri")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("profile output had no artifact uri: {profile_output}"))?
        .to_string();
    if artifact_uri != format!("datasheet://artifact/{artifact_id}") {
        bail!("profile artifact URI `{artifact_uri}` did not match id `{artifact_id}`");
    }
    if profile_output
        .pointer("/artifact/metadata/task_id")
        .and_then(Value::as_str)
        != Some(task_id.as_str())
    {
        bail!("profile artifact metadata did not carry task id `{task_id}`: {profile_output}");
    }

    run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "artifact".into(),
            artifact_id.clone().into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
    )?;
    assert_output_file(&output_dir, "bin")?;

    // Artifact access stays principal- and tenant-scoped on the shared plane.
    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--scheme".into(),
            "datasheet".into(),
            "--internal-server".into(),
            "datasheet".into(),
            "--internal-principal-subject".into(),
            "intruder".into(),
            "--internal-work-context".into(),
            "intruder-context".into(),
            "artifact".into(),
            artifact_id.clone().into(),
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
            "datasheet".into(),
            "--internal-server".into(),
            "datasheet".into(),
            "--internal-tenant".into(),
            "other-tenant".into(),
            "artifact".into(),
            artifact_id.clone().into(),
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
        wait_for_actual_usage_for_scheme(conformance, &mcp_url, "datasheet", &task_id, None)?;
    if usage.usage_uri != format!("datasheet://usage/task/{task_id}") {
        bail!("datasheet usage URI was wrong: {usage:?}");
    }
    let actual = usage
        .records
        .iter()
        .find(|record| record.kind == SmokeUsageKind::Actual)
        .ok_or_else(|| anyhow!("usage report had no actual record: {usage:?}"))?;
    if actual.quantity != Some(4.0)
        || actual.unit.as_deref() != Some("column")
        || actual.amount.is_some()
        || actual.currency.is_some()
    {
        bail!("datasheet usage actual record had wrong shape: {usage:?}");
    }

    let completion = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "complete-resource".into(),
            "--uri".into(),
            "datasheet://usage/task/{task_id}".into(),
            "--argument".into(),
            "task_id".into(),
            task_id[..8].to_string().into(),
        ],
    )?;
    contains(&completion, &task_id)?;

    let post_run_resources = run_datasheet_mcp(conformance, &mcp_url, ["resources".into()])?;
    contains(
        &post_run_resources,
        &format!("datasheet://usage/task/{task_id}"),
    )?;
    not_contains(&post_run_resources, &artifact_uri)?;

    let reports = run_datasheet_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), "datasheet://reports".into()],
    )?;
    contains(&reports, &task_id)?;
    contains(&reports, "\"status\": \"succeeded\"")?;

    datasheet_child.stop();
    cleanup.remove_on_drop();
    println!("datasheet MCP smoke ok");
    Ok(())
}

fn run_datasheet_mcp(
    conformance: &Path,
    mcp_url: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let mut all_args = vec![
        "--scheme".into(),
        "datasheet".into(),
        "--internal-server".into(),
        "datasheet".into(),
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
