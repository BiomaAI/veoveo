use super::*;

pub(crate) async fn media_mcp_auth(conformance: &Path, media: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let port = 18800u16;
    let base = format!("http://127.0.0.1:{port}");
    let log = tmpdir.join("media.log");
    let state_db = tmpdir.join("state.duckdb");
    let mut media_child = spawn_media_s3_smoke(media, port, PUBLIC_BASE_URL, &state_db, &log)?;
    wait_for_http(&format!("{base}/media/healthz")).await?;
    let health = reqwest::get(format!("{base}/media/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;
    let untrusted_host_status = reqwest::Client::new()
        .get(format!("{base}/media/healthz"))
        .header(HOST, "evil.example.com")
        .send()
        .await?
        .status();
    if untrusted_host_status != StatusCode::MISDIRECTED_REQUEST {
        bail!("media untrusted Host status was {untrusted_host_status}, expected 421");
    }
    assert_json_log(
        &log,
        &[
            ("message", "listening"),
            ("service", "veoveo-media-mcp"),
            ("mcp_path", "/media/mcp"),
        ],
    )?;
    assert_json_log(&log, &[("message", "media retention gc completed")])?;
    assert_http_status(&format!("{base}/media/mcp"), StatusCode::UNAUTHORIZED).await?;
    assert_http_status(
        &format!("{base}/media/artifacts/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        StatusCode::UNAUTHORIZED,
    )
    .await?;

    run_checked(
        conformance,
        [
            "--url".into(),
            format!("{base}/media/mcp").into(),
            "info".into(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;

    media_child.stop();
    cleanup.remove_on_drop();
    println!("media MCP auth smoke ok");
    Ok(())
}

pub(crate) async fn media_task_run(conformance: &Path, media: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let media_port = 18807u16;
    let provider_port = 18808u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let provider_base = format!("http://127.0.0.1:{provider_port}");
    let provider_log = tmpdir.join("provider.log");
    let media_log = tmpdir.join("media.log");
    let provider_ready = tmpdir.join("provider.ready");
    let media_state_db = tmpdir.join("media-state.duckdb");
    let output_dir = tmpdir.join("outputs");

    let mut provider = spawn_fake_media_provider(
        conformance,
        provider_port,
        &provider_ready,
        &provider_log,
        None,
    )?;
    wait_for_file_and_http(&provider_ready, &format!("{provider_base}/api/v3/models")).await?;

    let mut media_child = spawn_media_memory_smoke(
        media,
        media_port,
        &media_base,
        &media_state_db,
        &provider_base,
        &media_log,
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;
    let health = reqwest::get(format!("{media_base}/media/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;

    let mcp_url = format!("{media_base}/media/mcp");
    let cancel_output = run_direct_mcp(
        conformance,
        &mcp_url,
        [
            "run".into(),
            "fake/image".into(),
            "--input".into(),
            r#"{"prompt":"cancel"}"#.into(),
            "--cancel".into(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;
    let cancel_task_id = task_id_from_output(&cancel_output)?;
    contains(
        &cancel_output,
        &format!("cancelled task {cancel_task_id} (status Cancelled)"),
    )?;

    let complete_output = run_direct_mcp(
        conformance,
        &mcp_url,
        ["complete".into(), "fake".into()],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;
    contains(&complete_output, "fake/image")?;

    let run_output = run_direct_mcp(
        conformance,
        &mcp_url,
        [
            "run".into(),
            "fake/image".into(),
            "--input".into(),
            r#"{"prompt":"smoke"}"#.into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;
    let task_id = task_id_from_output(&run_output)?;
    let structured: SmokeGenerationRunOutput = structured_from_output(&run_output)?;
    if structured.artifacts.is_empty() {
        bail!("run output had no artifacts: {run_output}");
    }
    if structured.artifacts.iter().any(|artifact| {
        artifact.metadata.get("task_id").and_then(Value::as_str) != Some(task_id.as_str())
    }) {
        bail!("not all artifact metadata rows used task id `{task_id}`: {structured:?}");
    }
    assert_output_file(&output_dir, "png")?;

    let usage = wait_for_actual_usage(conformance, &mcp_url, &task_id, None)?;
    assert_usage_report(&usage, "media", &task_id)?;

    media_child.stop();
    provider.stop();
    cleanup.remove_on_drop();
    println!("media task run smoke ok");
    Ok(())
}
