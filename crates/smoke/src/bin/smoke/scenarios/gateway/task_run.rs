use super::*;

pub(crate) async fn gateway_task_run(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let media_port = 18801u16;
    let gateway_port = 18802u16;
    let provider_port = 18806u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let provider_base = format!("http://127.0.0.1:{provider_port}");
    let provider_log = tmpdir.join("provider.log");
    let media_log = tmpdir.join("media.log");
    let gateway_log = tmpdir.join("gateway.log");
    let provider_ready = tmpdir.join("provider.ready");
    let media_state_db = tmpdir.join("media-state.duckdb");
    let gateway_state_db = tmpdir.join("gateway-state.duckdb");
    let output_dir = tmpdir.join("outputs");

    let mut provider = spawn_fake_media_provider(
        conformance,
        provider_port,
        &provider_ready,
        &provider_log,
        Some(4000),
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

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, control_plane, &gateway_state_db),
        [
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    assert_ready_profiles(&gateway_base, 1).await?;

    let token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    let token = token.trim();

    let cancel_output = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "run".into(),
            "fake/image".into(),
            "--tool-name".into(),
            "media__run".into(),
            "--input".into(),
            r#"{"prompt":"cancel"}"#.into(),
            "--cancel".into(),
        ],
    )?;
    let cancel_task_id = task_id_from_output(&cancel_output)?;
    contains(
        &cancel_output,
        &format!("cancelled task {cancel_task_id} (status Cancelled)"),
    )?;
    contains(&cancel_output, "  [resource list changed]")?;
    contains(
        &cancel_output,
        &format!("  [task {cancel_task_id}] Working: submitted; prediction"),
    )?;

    let complete_output = run_mcp(
        conformance,
        &gateway_base,
        token,
        ["complete".into(), "fake".into()],
    )?;
    contains(&complete_output, "fake/image")?;

    let run_output = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "run".into(),
            "fake/image".into(),
            "--tool-name".into(),
            "media__run".into(),
            "--input".into(),
            r#"{"prompt":"smoke"}"#.into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
    )?;
    let task_id = task_id_from_output(&run_output)?;
    for expected in [
        "  [progress] 10%".to_string(),
        "  [resource list changed]".to_string(),
        format!("  [task {task_id}] Working: submitted; prediction"),
        "  [progress] 30%".to_string(),
        "  [resource updated] media://prediction/".to_string(),
        "  [progress] 100%".to_string(),
        format!("  [task {task_id}] Completed: completed;"),
        "subscribed to media://prediction/".to_string(),
        "unsubscribed from media://prediction/".to_string(),
    ] {
        contains(&run_output, &expected)?;
    }

    let structured: SmokeGenerationRunOutput = structured_from_output(&run_output)?;
    if structured.artifacts.is_empty() {
        bail!("run output had no artifacts: {run_output}");
    }
    for artifact in &structured.artifacts {
        if artifact.metadata.get("task_id").and_then(Value::as_str) != Some(task_id.as_str()) {
            bail!("artifact metadata did not use task id `{task_id}`: {artifact:?}");
        }
        if artifact.compliance.tenant_id.as_deref() != Some("tenant-a")
            || !artifact
                .compliance
                .data_labels
                .iter()
                .any(|label| label == "cui")
        {
            bail!("artifact compliance labels were not propagated: {artifact:?}");
        }
    }
    assert_output_file(&output_dir, "png")?;

    let usage = wait_for_actual_usage(
        conformance,
        &format!("{gateway_base}/mcp/default"),
        &task_id,
        Some(token),
    )?;
    assert_usage_report(&usage, "media", &task_id)?;

    let media_mcp_url = format!("{media_base}/media/mcp");
    let other_profile_tasks = run_direct_mcp(
        conformance,
        &media_mcp_url,
        ["--internal-profile".into(), "ops".into(), "tasks".into()],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;
    contains(&other_profile_tasks, "0 task(s)")?;

    assert_direct_mcp_denied(
        conformance,
        &media_mcp_url,
        [
            "--internal-profile".into(),
            "ops".into(),
            "usage".into(),
            task_id.clone().into(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;

    assert_direct_mcp_denied(
        conformance,
        &media_mcp_url,
        [
            "--internal-profile".into(),
            "ops".into(),
            "artifact".into(),
            structured.artifacts[0].sha256.clone().into(),
            "--output-dir".into(),
            tmpdir
                .join("denied-gateway-artifacts")
                .as_os_str()
                .to_os_string(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;

    gateway_child.stop();
    let audit_summary = run_checked(
        gateway,
        [
            "audit-method-summary".into(),
            "--state-db".into(),
            gateway_state_db.as_os_str().to_os_string(),
        ],
        [],
    )?;
    let audit_summary: Value = serde_json::from_str(&audit_summary)?;
    assert_no_audit_denies(&audit_summary)?;
    assert_audit_method(&audit_summary, "completion/complete", 1, 0)?;
    assert_audit_method(&audit_summary, "tools/call", 2, 0)?;
    assert_audit_method(&audit_summary, "tasks/cancel", 1, 0)?;
    assert_audit_method(&audit_summary, "tasks/get", 2, 0)?;
    assert_audit_method(&audit_summary, "tasks/result", 2, 0)?;
    assert_audit_method(&audit_summary, "resources/subscribe", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/unsubscribe", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/read", 2, 0)?;

    media_child.stop();
    provider.stop();
    cleanup.remove_on_drop();
    println!("gateway task run smoke ok");
    Ok(())
}
