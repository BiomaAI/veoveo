use super::*;

pub(crate) struct SmokeMcpHandler;

impl ClientHandler for SmokeMcpHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::builder().enable_tasks().build(),
            Implementation::new("veoveo-smoke", env!("CARGO_PKG_VERSION")),
        )
    }
}

pub(crate) type SmokeMcpClient = RunningService<rmcp::RoleClient, SmokeMcpHandler>;

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

pub(crate) async fn connect_mcp_client(url: &str, bearer_token: &str) -> Result<SmokeMcpClient> {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url.to_string())
            .auth_header(bearer_token.to_string()),
    );
    Ok(SmokeMcpHandler
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?)
}

pub(crate) async fn read_mcp_resource_json(session: &SmokeMcpClient, uri: &str) -> Result<Value> {
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

pub(crate) async fn assert_mcp_client_resource_denied(
    session: &SmokeMcpClient,
    uri: &str,
) -> Result<()> {
    if read_mcp_resource_json(session, uri).await.is_ok() {
        bail!("same MCP client unexpectedly read `{uri}` after policy update");
    }
    Ok(())
}

pub(crate) async fn call_tool_as_task(
    session: &SmokeMcpClient,
    tool_name: &str,
    arguments: Value,
) -> Result<rmcp::model::Task> {
    let arguments = arguments
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("tool arguments must be a JSON object"))?;
    let params = CallToolRequestParams::new(tool_name.to_owned()).with_arguments(arguments);
    let result = session.call_tool_once(params).await?;
    match result {
        rmcp::model::CallToolResponse::Task(created) => Ok(created.task),
        other => bail!("expected CreateTaskResult for {tool_name}, got {other:?}"),
    }
}

pub(crate) async fn await_task_terminal(
    session: &SmokeMcpClient,
    task_id: &str,
) -> Result<rmcp::model::DetailedTask> {
    await_task_terminal_with_timeout(session, task_id, Duration::from_secs(30)).await
}

pub(crate) async fn await_task_terminal_with_timeout(
    session: &SmokeMcpClient,
    task_id: &str,
    timeout: Duration,
) -> Result<rmcp::model::DetailedTask> {
    tokio::time::timeout(timeout, async {
        loop {
            let info = session
                .get_task(rmcp::model::GetTaskParams::new(task_id))
                .await?;
            match info.task.status() {
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
    session: &SmokeMcpClient,
    task_id: &str,
) -> Result<rmcp::model::CallToolResult> {
    let result = session
        .get_task(rmcp::model::GetTaskParams::new(task_id))
        .await?;
    match result.task.payload {
        rmcp::model::TaskPayload::Completed { result } => {
            Ok(serde_json::from_value(Value::Object(result))?)
        }
        other => bail!("expected completed task payload for {task_id}, got {other:?}"),
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
