use super::*;

pub(crate) async fn media_mcp_auth(
    conformance: &Path,
    media: &Path,
    artifact_service: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(artifact_service)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let port = 18800u16;
    let base = format!("http://127.0.0.1:{port}");
    let log = tmpdir.join("media.log");
    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut media_child = spawn_media_s3_smoke(
        media,
        port,
        PUBLIC_BASE_URL,
        &plane.platform,
        &plane.url,
        &log,
    )?;
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
    assert_json_log(
        &log,
        &[("message", "media retention reconciliation completed")],
    )?;
    assert_http_status(&format!("{base}/media/mcp"), StatusCode::UNAUTHORIZED).await?;
    assert_http_status(
        &format!("{base}/media/artifacts/01900000-0000-7000-8000-000000000001"),
        StatusCode::NOT_FOUND,
    )
    .await?;

    assert_direct_mcp_denied(
        conformance,
        &format!("{base}/media/mcp"),
        [
            "--internal-server".into(),
            "simulation".into(),
            "info".into(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;

    run_checked(
        conformance,
        [
            "--url".into(),
            format!("{base}/media/mcp").into(),
            "info".into(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;

    media_child.stop();
    cleanup.remove_on_drop();
    println!("media MCP auth smoke ok");
    Ok(())
}

pub(crate) async fn media_task_run(
    conformance: &Path,
    media: &Path,
    artifact_service: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(artifact_service)?;

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
    let output_dir = tmpdir.join("outputs");

    let mut provider = spawn_fake_media_provider(
        conformance,
        provider_port,
        &provider_ready,
        &provider_log,
        Some(4000),
    )?;
    wait_for_file_and_http(&provider_ready, &format!("{provider_base}/api/v3/models")).await?;

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut media_child = spawn_media_memory_smoke(
        media,
        media_port,
        &media_base,
        &plane.platform,
        &provider_base,
        &plane.url,
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
    let resources_output = run_direct_mcp(
        conformance,
        &mcp_url,
        ["resources".into()],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;
    contains(&resources_output, "media://models")?;
    contains(&resources_output, "media://usage")?;

    let prompts_output = run_direct_mcp(
        conformance,
        &mcp_url,
        ["prompts".into()],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;
    contains(&prompts_output, "media-model-select")?;
    contains(&prompts_output, "media-task-review")?;

    let prompt_output = run_direct_mcp(
        conformance,
        &mcp_url,
        [
            "prompt".into(),
            "media-model-select".into(),
            "--arguments".into(),
            r#"{"goal":"generate a compact smoke test image","media_type":"image","budget":"low"}"#
                .into(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;
    contains(&prompt_output, "media://models")?;

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
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;
    let cancel_task_id = task_id_from_output(&cancel_output)?;
    contains(&cancel_output, "cancel target ready:")?;
    contains(
        &cancel_output,
        &format!("cancelled task {cancel_task_id} (status Cancelled)"),
    )?;
    wait_for_file_text(&provider_log, "fake media provider cancellation accepted:").await?;

    let late_webhook_cancel_output = run_direct_mcp(
        conformance,
        &mcp_url,
        [
            "run".into(),
            "fake/image".into(),
            "--input".into(),
            r#"{"prompt":"cancel but reconcile billing","_fake_provider_cancellation":"not_deleted"}"#
                .into(),
            "--cancel".into(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;
    let late_webhook_cancel_task_id = task_id_from_output(&late_webhook_cancel_output)?;
    contains(&late_webhook_cancel_output, "cancel target ready:")?;
    contains(
        &late_webhook_cancel_output,
        &format!("cancelled task {late_webhook_cancel_task_id} (status Cancelled)"),
    )?;
    wait_for_file_text(
        &provider_log,
        "fake media provider cancellation not deleted:",
    )
    .await?;
    let cancelled_usage =
        wait_for_actual_usage(conformance, &mcp_url, &late_webhook_cancel_task_id, None)?;
    assert_usage_report(&cancelled_usage, "media", &late_webhook_cancel_task_id)?;
    not_contains(
        &fs::read_to_string(&provider_log).unwrap_or_default(),
        "fake media provider output fetched",
    )?;

    let complete_output = run_direct_mcp(
        conformance,
        &mcp_url,
        ["complete".into(), "fake".into()],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
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
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;
    let task_id = task_id_from_output(&run_output)?;
    for expected in [
        "poll: Working — submitted; prediction".to_string(),
        "  [resource updated] media://prediction/".to_string(),
        "poll: Completed — completed;".to_string(),
        "subscribed to media://prediction/".to_string(),
        "unsubscribed from media://prediction/".to_string(),
    ] {
        contains(&run_output, &expected)?;
    }
    let structured: SmokeGenerationRunOutput = structured_from_output(&run_output)?;
    if structured.artifacts.is_empty() {
        bail!("run output had no artifacts: {run_output}");
    }
    if structured.artifacts.iter().any(|artifact| {
        artifact.metadata.get("task_id").and_then(Value::as_str) != Some(task_id.as_str())
    }) {
        bail!("not all artifact metadata rows used task id `{task_id}`: {structured:?}");
    }
    if structured.artifacts.iter().any(|artifact| {
        artifact.artifact_uri != format!("media://artifact/{}", artifact.artifact_id)
    }) {
        bail!("not all artifact metadata rows used canonical media artifact URIs: {structured:?}");
    }
    let artifact_uri = structured.artifacts[0].artifact_uri.clone();
    assert_output_file(&output_dir, "png")?;

    let usage = wait_for_actual_usage(conformance, &mcp_url, &task_id, None)?;
    assert_usage_report(&usage, "media", &task_id)?;

    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--internal-profile".into(),
            "admin".into(),
            "usage".into(),
            task_id.clone().into(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;

    // A principal outside the artifact's work context and grants cannot read it.
    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--internal-principal-subject".into(),
            "intruder".into(),
            "--internal-work-context".into(),
            "intruder-context".into(),
            "artifact".into(),
            structured.artifacts[0].artifact_id.clone().into(),
            "--output-dir".into(),
            tmpdir.join("denied-artifacts").as_os_str().to_os_string(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;

    // And the plane's hard tenant partition: a caller in another tenant is
    // denied even though it shares the (fixed) conformance principal id.
    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--internal-tenant".into(),
            "other-tenant".into(),
            "artifact".into(),
            structured.artifacts[0].artifact_id.clone().into(),
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

    let task_review_output = run_direct_mcp(
        conformance,
        &mcp_url,
        [
            "prompt".into(),
            "media-task-review".into(),
            "--arguments".into(),
            format!(r#"{{"task_id":"{task_id}"}}"#).into(),
        ],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;
    contains(
        &task_review_output,
        &format!("media://usage/task/{task_id}"),
    )?;

    let post_run_resources = run_direct_mcp(
        conformance,
        &mcp_url,
        ["resources".into()],
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )?;
    contains(
        &post_run_resources,
        &format!("media://usage/task/{task_id}"),
    )?;
    // Artifacts on the shared plane are addressable by URI but deliberately not
    // enumerated in the resource listing — listing them would be a cross-tenant
    // existence oracle. The artifact stays readable by its URI (asserted above);
    // it must not appear in the enumerable resource set.
    not_contains(&post_run_resources, &artifact_uri)?;

    media_child.stop();
    provider.stop();
    cleanup.remove_on_drop();
    println!("media task run smoke ok");
    Ok(())
}
