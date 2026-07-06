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
    let control_db = spawn_gateway_control_db(gateway, control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &control_db.url, &gateway_state_db),
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
    assert_ready_profiles(&gateway_base, 2).await?;

    let token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "operator:use",
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
        &format!("{gateway_base}/mcp/operator"),
        &task_id,
        Some(token),
    )?;
    assert_usage_report(&usage, "media", &task_id)?;

    let full_session = connect_mcp_session(&format!("{gateway_base}/mcp/operator"), token).await?;
    let full_tools = full_session.list_tools(Default::default()).await?;
    if full_tools.tools.iter().any(|tool| {
        matches!(
            tool.name.as_ref(),
            "media__artifact" | "media__models" | "media__model_schema" | "task_result"
        )
    }) {
        bail!("full-MCP client unexpectedly saw compatibility helpers: {full_tools:?}");
    }
    let full_run_tool = full_tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "media__run")
        .ok_or_else(|| anyhow!("full-MCP client did not see media__run: {full_tools:?}"))?;
    if full_run_tool.task_support() != rmcp::model::TaskSupport::Required {
        bail!(
            "full-MCP media__run task support was {:?}, expected Required",
            full_run_tool.task_support()
        );
    }

    let compat_token = gateway_hosted_public_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "operator:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    let compat_token = compat_token.trim();
    let session =
        connect_mcp_session(&format!("{gateway_base}/mcp/operator"), compat_token).await?;
    let listed_tools = session.list_tools(Default::default()).await?;
    for expected_tool in [
        "media__artifact",
        "media__models",
        "media__model_schema",
        "media__run",
        "task_result",
    ] {
        if !listed_tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == expected_tool)
        {
            bail!("gateway did not list {expected_tool}: {listed_tools:?}");
        }
    }
    let models_result = session
        .call_tool(
            CallToolRequestParams::new("media__models").with_arguments(
                serde_json::json!({
                    "query": "fake",
                    "type": "image-to-image",
                    "limit": 5
                })
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await?;
    if models_result.is_error == Some(true) {
        bail!("gateway media__models returned an error: {models_result:?}");
    }
    let models_structured = models_result
        .structured_content
        .clone()
        .ok_or_else(|| anyhow!("media__models returned no structured content"))?;
    let models = models_structured
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("media__models returned no models array: {models_structured}"))?;
    if !models.iter().any(|model| {
        model
            .get("model_id")
            .and_then(Value::as_str)
            .is_some_and(|model_id| model_id == "fake/image")
    }) {
        bail!("media__models did not return fake/image: {models_structured}");
    }
    let schema_result = session
        .call_tool(
            CallToolRequestParams::new("media__model_schema").with_arguments(
                serde_json::json!({ "model": "fake/image" })
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await?;
    if schema_result.is_error == Some(true) {
        bail!("gateway media__model_schema returned an error: {schema_result:?}");
    }
    let schema_structured = schema_result
        .structured_content
        .clone()
        .ok_or_else(|| anyhow!("media__model_schema returned no structured content"))?;
    if schema_structured.get("model_id").and_then(Value::as_str) != Some("fake/image")
        || !schema_structured
            .get("request_schema")
            .and_then(|schema| schema.get("required"))
            .and_then(Value::as_array)
            .is_some_and(|required| required.iter().any(|field| field == "prompt"))
    {
        bail!("media__model_schema did not return fake/image prompt schema: {schema_structured}");
    }
    let media_tool = listed_tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "media__run")
        .ok_or_else(|| anyhow!("gateway did not list media__run: {listed_tools:?}"))?;
    if media_tool.task_support() != rmcp::model::TaskSupport::Optional {
        bail!(
            "gateway media__run task support was {:?}, expected Optional",
            media_tool.task_support()
        );
    }
    let direct_result = session
        .call_tool(
            CallToolRequestParams::new("media__run").with_arguments(
                serde_json::json!({
                    "model": "fake/image",
                    "input": { "prompt": "direct-call smoke" }
                })
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await?;
    if direct_result.is_error == Some(true) {
        bail!("direct gateway tools/call returned an error: {direct_result:?}");
    }
    let direct_task_id = direct_result
        .meta
        .as_ref()
        .and_then(|meta| meta.0.get(RELATED_TASK_META_KEY))
        .and_then(|value| value.get("taskId"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("direct tools/call returned no gateway task id: {direct_result:?}"))?
        .to_string();
    let direct_structured: SmokeGenerationRunOutput = serde_json::from_value(
        direct_result
            .structured_content
            .clone()
            .ok_or_else(|| anyhow!("direct tools/call returned no structured output"))?,
    )?;
    if direct_structured.artifacts.is_empty() {
        bail!("direct tools/call returned no artifacts: {direct_result:?}");
    }
    if direct_structured
        .artifacts
        .iter()
        .any(|artifact| artifact.download_url.is_some())
    {
        bail!("direct tools/call leaked artifact download_url: {direct_structured:?}");
    }
    if !direct_result
        .content
        .iter()
        .any(|block| block.as_image().is_some())
    {
        bail!("direct tools/call did not inline image content for tools-compatible client");
    }
    let task_result = session
        .call_tool(
            CallToolRequestParams::new("task_result").with_arguments(
                serde_json::json!({
                    "task_uri": format!("veoveo://task/{direct_task_id}")
                })
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await?;
    if task_result.is_error == Some(true) {
        bail!("task_result returned an error: {task_result:?}");
    }
    if !task_result
        .content
        .iter()
        .any(|block| block.as_image().is_some())
    {
        bail!("task_result did not inline image content for tools-compatible client");
    }
    let task_result_structured: SmokeGenerationRunOutput = serde_json::from_value(
        task_result
            .structured_content
            .clone()
            .ok_or_else(|| anyhow!("task_result returned no structured output"))?,
    )?;
    if task_result_structured
        .artifacts
        .iter()
        .any(|artifact| artifact.download_url.is_some())
    {
        bail!("task_result leaked artifact download_url: {task_result_structured:?}");
    }
    let artifact_result = session
        .call_tool(
            CallToolRequestParams::new("media__artifact").with_arguments(
                serde_json::json!({
                    "artifact_uri": direct_structured.artifacts[0].artifact_uri
                })
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await?;
    if artifact_result.is_error == Some(true) {
        bail!("media__artifact returned an error: {artifact_result:?}");
    }
    if !artifact_result
        .content
        .iter()
        .any(|block| block.as_image().is_some())
    {
        bail!("media__artifact did not return image content: {artifact_result:?}");
    }
    let artifact_structured = artifact_result
        .structured_content
        .clone()
        .ok_or_else(|| anyhow!("media__artifact returned no structured content"))?;
    if artifact_structured
        .get("artifact")
        .and_then(|artifact| artifact.get("download_url"))
        .is_some()
    {
        bail!("media__artifact leaked artifact download_url: {artifact_structured}");
    }
    let direct_status: GatewayTaskStatusDocument = serde_json::from_value(
        read_mcp_resource_json(&session, &format!("veoveo://task/{direct_task_id}")).await?,
    )?;
    if direct_status.task.status != GatewayTaskStatusKind::Completed {
        bail!("direct gateway task status was not completed: {direct_status:?}");
    }
    if direct_status.result.is_none() {
        bail!("completed gateway task status resource did not include result");
    }

    let media_mcp_url = format!("{media_base}/media/mcp");
    let other_profile_tasks = run_direct_mcp(
        conformance,
        &media_mcp_url,
        ["--internal-profile".into(), "admin".into(), "tasks".into()],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;
    contains(&other_profile_tasks, "0 task(s)")?;

    assert_direct_mcp_denied(
        conformance,
        &media_mcp_url,
        [
            "--internal-profile".into(),
            "admin".into(),
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
            "admin".into(),
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
    assert_audit_method(&audit_summary, "tools/call", 6, 0)?;
    assert_audit_method(&audit_summary, "tasks/cancel", 1, 0)?;
    assert_audit_method(&audit_summary, "tasks/get", 3, 0)?;
    assert_audit_method(&audit_summary, "tasks/result", 4, 0)?;
    assert_audit_method(&audit_summary, "resources/subscribe", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/unsubscribe", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/read", 3, 0)?;

    media_child.stop();
    provider.stop();
    cleanup.remove_on_drop();
    println!("gateway task run smoke ok");
    Ok(())
}
