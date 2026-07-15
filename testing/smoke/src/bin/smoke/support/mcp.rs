use super::*;

pub(crate) struct SmokeMcpClient;

impl ClientHandler for SmokeMcpClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-smoke", env!("CARGO_PKG_VERSION")),
        )
    }
}

pub(crate) type SmokeMcpSession = RunningService<rmcp::RoleClient, SmokeMcpClient>;

pub(crate) fn run_mcp(
    conformance: &Path,
    gateway_base: &str,
    token: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let mut all_args = vec![
        "--url".into(),
        format!("{gateway_base}/mcp/operator").into(),
    ];
    all_args.extend(args);
    run_checked(conformance, all_args, [("MCP_BEARER_TOKEN", token.into())])
}

pub(crate) async fn connect_mcp_session(url: &str, bearer_token: &str) -> Result<SmokeMcpSession> {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url.to_string())
            .auth_header(bearer_token.to_string()),
    );
    Ok(SmokeMcpClient.serve(transport).await?)
}

pub(crate) async fn read_mcp_resource_json(session: &SmokeMcpSession, uri: &str) -> Result<Value> {
    let result = session
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    let Some(text) = result.contents.iter().find_map(|content| match content {
        ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
        _ => None,
    }) else {
        bail!("MCP resource `{uri}` did not return text content: {result:?}");
    };
    Ok(serde_json::from_str(text)?)
}

pub(crate) async fn assert_mcp_session_resource_denied(
    session: &SmokeMcpSession,
    uri: &str,
) -> Result<()> {
    if read_mcp_resource_json(session, uri).await.is_ok() {
        bail!("same MCP session unexpectedly read `{uri}` after policy update");
    }
    Ok(())
}

pub(crate) async fn call_tool_as_task(
    session: &SmokeMcpSession,
    tool_name: &str,
    arguments: Value,
) -> Result<rmcp::model::Task> {
    let arguments = arguments
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("tool arguments must be a JSON object"))?;
    let params = CallToolRequestParams::new(tool_name.to_owned())
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

pub(crate) async fn await_task_terminal(
    session: &SmokeMcpSession,
    task_id: &str,
) -> Result<rmcp::model::Task> {
    await_task_terminal_with_timeout(session, task_id, Duration::from_secs(30)).await
}

pub(crate) async fn await_task_terminal_with_timeout(
    session: &SmokeMcpSession,
    task_id: &str,
    timeout: Duration,
) -> Result<rmcp::model::Task> {
    tokio::time::timeout(timeout, async {
        loop {
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
    })
    .await
    .with_context(|| format!("task {task_id} did not reach a terminal status"))?
}

pub(crate) async fn task_payload(
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

pub(crate) fn run_direct_mcp(
    conformance: &Path,
    url: &str,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let mut all_args = vec!["--url".into(), url.into()];
    all_args.extend(args);
    run_checked(conformance, all_args, envs)
}

pub(crate) fn assert_direct_mcp_denied(
    conformance: &Path,
    url: &str,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<()> {
    let mut all_args = vec!["--url".into(), url.into()];
    all_args.extend(args);
    let output = run_raw(conformance, all_args, envs)?;
    if output.status.success() {
        bail!(
            "direct MCP command was unexpectedly authorized\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(crate) fn assert_mcp_denied(
    conformance: &Path,
    mcp_url: &str,
    token: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<()> {
    let mut all_args = vec!["--url".into(), mcp_url.into()];
    all_args.extend(args);
    let output = run_raw(conformance, all_args, [("MCP_BEARER_TOKEN", token.into())])?;
    if output.status.success() {
        bail!(
            "MCP command was unexpectedly authorized\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
