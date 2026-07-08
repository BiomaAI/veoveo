use super::*;

/// Pre-validations for the agent kernel's gateway usage:
///
/// 1. A client-initiated task-augmented `tools/call` on a task-OPTIONAL tool
///    (duckdb `execute`/`query`, `taskSupport: optional`) returns a
///    `CreateTaskResult` and completes through the client-driven task
///    lifecycle. This is the dispatch shape rig's `McpTaskPolicy::Preferred`
///    emits for every optional-support tool.
/// 2. Task continuity across sessions of the same principal: a task created
///    on session A stays visible to a later session B holding a fresh token
///    for the same service client — `tasks/get` and `tasks/result` succeed.
///    This is what makes kernel token rotation safe.
pub(crate) async fn agent_gateway(
    conformance: &Path,
    duckdb: &Path,
    gateway: &Path,
    control_plane: &Path,
    artifact_service: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(duckdb)?;
    assert_executable(gateway)?;
    assert_executable(artifact_service)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let duckdb_port = 18820u16;
    let gateway_port = 18821u16;
    let duckdb_base = format!("http://127.0.0.1:{duckdb_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let generated_control_plane = tmpdir.join("gateway.agent.json");
    let gateway_state_db = tmpdir.join("gateway-state.duckdb");
    let duckdb_data_dir = tmpdir.join("duckdb");
    let duckdb_log = tmpdir.join("duckdb.log");
    let gateway_log = tmpdir.join("gateway.log");

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut duckdb_child = spawn_duckdb_smoke(
        duckdb,
        duckdb_port,
        &duckdb_base,
        &duckdb_data_dir,
        &plane.url,
        &duckdb_log,
    )?;
    wait_for_http(&format!("{duckdb_base}/duckdb/healthz")).await?;

    run_checked(
        conformance,
        [
            "gateway-agent-smoke-control-plane".into(),
            "--base".into(),
            control_plane.as_os_str().to_os_string(),
            "--output".into(),
            generated_control_plane.as_os_str().to_os_string(),
            "--duckdb-upstream-url".into(),
            format!("{duckdb_base}/duckdb/mcp").into(),
        ],
        [],
    )?;
    let validation = run_checked(
        gateway,
        [
            "validate".into(),
            "--control-plane".into(),
            generated_control_plane.as_os_str().to_os_string(),
        ],
        [],
    )?;
    contains(&validation, "ok: 1 server(s)")?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let control_db = spawn_gateway_control_db(gateway, &generated_control_plane).await?;
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

    let token_a = gateway_token_for_profile(
        conformance,
        &gateway_base,
        "operator",
        &["--scope", "operator:use"],
    )?;
    let token_a = token_a.trim();
    let session_a = connect_mcp_session(&format!("{gateway_base}/mcp/operator"), token_a).await?;

    // The gateway must project duckdb's genuine task support: optional stays
    // optional (rig Preferred will task it) and required stays required for
    // full-MCP clients.
    let tools = session_a.list_tools(Default::default()).await?;
    for (tool_name, expected) in [
        ("duckdb__query", rmcp::model::TaskSupport::Optional),
        ("duckdb__execute", rmcp::model::TaskSupport::Optional),
        ("duckdb__export", rmcp::model::TaskSupport::Required),
    ] {
        let tool = tools
            .tools
            .iter()
            .find(|tool| tool.name.as_ref() == tool_name)
            .ok_or_else(|| anyhow!("gateway did not list {tool_name}: {tools:?}"))?;
        if tool.task_support() != expected {
            bail!(
                "{tool_name} task support was {:?}, expected {expected:?}",
                tool.task_support()
            );
        }
    }

    // (1) Task-augmented call on a task-OPTIONAL tool.
    let created = call_tool_as_task(
        &session_a,
        "duckdb__execute",
        serde_json::json!({
            "db": "agent_smoke",
            "sql": "CREATE TABLE facts AS SELECT 42 AS answer",
            "create_if_missing": true
        }),
    )
    .await?;
    println!(
        "optional-tool task {} created (status {:?})",
        created.task_id, created.status
    );
    let completed = await_task_terminal(&session_a, &created.task_id).await?;
    if completed.status != rmcp::model::TaskStatus::Completed {
        bail!(
            "task-augmented duckdb__execute ended {:?}: {:?}",
            completed.status,
            completed.status_message
        );
    }
    let payload = task_payload(&session_a, &created.task_id).await?;
    if payload.is_error == Some(true) {
        bail!("task-augmented duckdb__execute payload was an error: {payload:?}");
    }

    // (2) Session continuity for the kernel's token-rotation design: create a
    // task on session A, close the session, and drive the task to its result
    // from session B under a fresh token for the same principal.
    let continuity = call_tool_as_task(
        &session_a,
        "duckdb__query",
        serde_json::json!({
            "db": "agent_smoke",
            "sql": "SELECT answer FROM facts"
        }),
    )
    .await?;
    session_a.cancel().await?;

    let token_b = gateway_token_for_profile(
        conformance,
        &gateway_base,
        "operator",
        &["--scope", "operator:use"],
    )?;
    let token_b = token_b.trim();
    if token_a == token_b {
        bail!("expected a fresh token for session B");
    }
    let session_b = connect_mcp_session(&format!("{gateway_base}/mcp/operator"), token_b).await?;
    let completed = await_task_terminal(&session_b, &continuity.task_id).await?;
    if completed.status != rmcp::model::TaskStatus::Completed {
        bail!(
            "cross-session duckdb__query ended {:?}: {:?}",
            completed.status,
            completed.status_message
        );
    }
    let payload = task_payload(&session_b, &continuity.task_id).await?;
    if payload.is_error == Some(true) {
        bail!("cross-session task payload was an error: {payload:?}");
    }
    let structured = payload
        .structured_content
        .clone()
        .ok_or_else(|| anyhow!("cross-session task payload had no structured content"))?;
    if !structured.to_string().contains("42") {
        bail!("cross-session query result did not contain the expected row: {structured}");
    }
    session_b.cancel().await?;

    gateway_child.stop();
    duckdb_child.stop();
    cleanup.remove_on_drop();
    println!("agent gateway smoke ok");
    Ok(())
}

/// `tools/call` with task augmentation, expecting the server to accept the
/// call as a task (SEP-1686).
async fn call_tool_as_task(
    session: &SmokeMcpSession,
    tool_name: &str,
    arguments: Value,
) -> Result<rmcp::model::Task> {
    let arguments = arguments
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("tool arguments must be a JSON object"))?;
    let params = CallToolRequestParams::new(tool_name.to_string())
        .with_arguments(arguments)
        .with_task(rmcp::model::TaskMetadata::new().with_ttl(3_600_000));
    let result = session
        .send_request(rmcp::model::ClientRequest::CallToolRequest(
            rmcp::model::Request::new(params),
        ))
        .await?;
    match result {
        rmcp::model::ServerResult::CreateTaskResult(created) => Ok(created.task),
        other => bail!("expected CreateTaskResult for {tool_name}, got {other:?}"),
    }
}

/// Poll `tasks/get` until the task reaches a terminal status.
async fn await_task_terminal(
    session: &SmokeMcpSession,
    task_id: &str,
) -> Result<rmcp::model::Task> {
    for _ in 0..60 {
        let result = session
            .send_request(rmcp::model::ClientRequest::GetTaskRequest(
                rmcp::model::Request::new(rmcp::model::GetTaskParams::new(task_id)),
            ))
            .await?;
        let rmcp::model::ServerResult::GetTaskResult(info) = result else {
            bail!("expected GetTaskResult for {task_id}, got {result:?}");
        };
        match info.task.status {
            rmcp::model::TaskStatus::Working | rmcp::model::TaskStatus::InputRequired => {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            _ => return Ok(info.task),
        }
    }
    bail!("task {task_id} did not reach a terminal status in time")
}

/// `tasks/result` for a terminal task.
async fn task_payload(
    session: &SmokeMcpSession,
    task_id: &str,
) -> Result<rmcp::model::CallToolResult> {
    let result = session
        .send_request(rmcp::model::ClientRequest::GetTaskPayloadRequest(
            rmcp::model::Request::new(rmcp::model::GetTaskPayloadParams::new(task_id)),
        ))
        .await?;
    match result {
        rmcp::model::ServerResult::CallToolResult(payload) => Ok(payload),
        other => bail!("expected CallToolResult payload for {task_id}, got {other:?}"),
    }
}
