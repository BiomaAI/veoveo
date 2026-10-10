use super::*;

pub struct SmokeMcpHandler;

impl ClientHandler for SmokeMcpHandler {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::builder().enable_tasks().build(),
            Implementation::new("veoveo-smoke", env!("CARGO_PKG_VERSION")),
        )
    }
}

pub struct SmokeMcpClient {
    inner: RunningService<rmcp::RoleClient, SmokeMcpHandler>,
    endpoint: String,
    bearer: String,
    host: Option<reqwest::header::HeaderValue>,
    owned_tasks: std::sync::Mutex<
        std::collections::BTreeMap<String, crate::lifecycle::owner::CleanupRegistration>,
    >,
}
impl std::ops::Deref for SmokeMcpClient {
    type Target = RunningService<rmcp::RoleClient, SmokeMcpHandler>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl SmokeMcpClient {
    pub(crate) fn admitted(
        inner: RunningService<rmcp::RoleClient, SmokeMcpHandler>,
        endpoint: &str,
        bearer: &str,
        host: Option<reqwest::header::HeaderValue>,
    ) -> Self {
        Self {
            inner,
            endpoint: endpoint.to_owned(),
            bearer: bearer.to_owned(),
            host,
            owned_tasks: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }
    pub async fn cancel(self) -> Result<()> {
        self.inner.cancel().await?;
        Ok(())
    }
    fn settle_task(&self, id: &str) -> Result<()> {
        if let Some(owned) = self
            .owned_tasks
            .lock()
            .expect("owned Task registrations")
            .remove(id)
        {
            owned.settled()?;
        }
        Ok(())
    }
    /// Move the existing Task cleanup registration across an explicit SDK reconnect.
    pub(crate) fn take_task_cleanup(
        &self,
        id: &veoveo_types::CanonicalTaskId,
    ) -> Option<crate::lifecycle::owner::CleanupRegistration> {
        self.owned_tasks
            .lock()
            .expect("owned Task registrations")
            .remove(id.as_str())
    }
}

pub fn run_mcp(
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

pub async fn connect_mcp_client(url: &str, bearer_token: &str) -> Result<SmokeMcpClient> {
    Ok(SmokeMcpClient::admitted(
        connect_mcp_handler(url, bearer_token, SmokeMcpHandler).await?,
        url,
        bearer_token,
        None,
    ))
}

pub async fn connect_tools_only_mcp_client(
    url: &str,
    bearer_token: &str,
) -> Result<RunningService<rmcp::RoleClient, ()>> {
    connect_mcp_handler(url, bearer_token, ()).await
}

async fn connect_mcp_handler<H: ClientHandler>(
    url: &str,
    bearer_token: &str,
    handler: H,
) -> Result<RunningService<rmcp::RoleClient, H>> {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url.to_string())
            .auth_header(bearer_token.to_string()),
    );
    Ok(handler
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?)
}

pub async fn read_mcp_resource_json(
    session: &rmcp::Peer<rmcp::RoleClient>,
    uri: &str,
) -> Result<Value> {
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

pub async fn assert_mcp_client_resource_denied(
    session: &SmokeMcpClient,
    uri: &str,
    expected: veoveo_mcp_conformance::client::failure::ObservedFailure,
) -> Result<()> {
    // This assertion must reach current server policy on the existing transport.
    // The SDK can otherwise return the previous body, including on refresh errors.
    session.clear_response_cache().await;
    let error = read_mcp_resource_json(session, uri)
        .await
        .err()
        .context("same MCP client unexpectedly read the resource after policy update")?;
    let observed = veoveo_mcp_conformance::client::failure::observe(&error)
        .context("resource rejection has no typed peer observation; transport/parser failures cannot prove denial")?;
    anyhow::ensure!(
        observed == expected,
        "resource rejection disagrees with the expected policy denial"
    );
    Ok(())
}

pub async fn call_tool_as_task(
    session: &SmokeMcpClient,
    tool_name: &str,
    arguments: Value,
) -> Result<rmcp::model::Task> {
    let arguments = arguments
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("tool arguments must be a JSON object"))?;
    let params = CallToolRequestParams::new(tool_name.to_owned()).with_arguments(arguments);
    use crate::lifecycle::owner::{self, CleanupKind};
    let task = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let observed = std::sync::Arc::clone(&task);
    let endpoint = session.endpoint.clone();
    let bearer = session.bearer.clone();
    let host = session.host.clone();
    let registration = owner::register_cleanup(
        CleanupKind::Remote,
        "MCP Task",
        &uuid::Uuid::now_v7().to_string(),
        move || async move {
            let id = observed
                .lock()
                .expect("owned Task identity")
                .clone()
                .context("Task dispatch outcome unresolved; intent retained")?;
            let mut config =
                StreamableHttpClientTransportConfig::with_uri(endpoint).auth_header(bearer);
            if let Some(host) = host {
                config = config.custom_headers(std::collections::HashMap::from([(
                    reqwest::header::HOST,
                    host,
                )]));
            }
            let client = SmokeMcpHandler
                .serve_with_lifecycle(
                    StreamableHttpClientTransport::from_config(config),
                    ClientLifecycleMode::Discover {
                        preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                    },
                )
                .await?;
            let current = client
                .get_task(rmcp::model::GetTaskParams::new(&id))
                .await?
                .task;
            ensure!(current.task.task_id == id, "cleanup Task identity mismatch");
            if matches!(
                current.status(),
                rmcp::model::TaskStatus::Working | rmcp::model::TaskStatus::InputRequired
            ) {
                client
                    .cancel_task(rmcp::model::CancelTaskParams::new(&id))
                    .await?;
            }
            loop {
                let state = client
                    .get_task(rmcp::model::GetTaskParams::new(&id))
                    .await?
                    .task;
                ensure!(state.task.task_id == id, "cleanup Task identity mismatch");
                if !matches!(
                    state.status(),
                    rmcp::model::TaskStatus::Working | rmcp::model::TaskStatus::InputRequired
                ) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            client.cancel().await?;
            Ok(())
        },
    )?;
    owner::check_effect()?;
    let result = session.call_tool_once(params).await?;
    match result {
        rmcp::model::CallToolResponse::Task(created) => {
            *task.lock().expect("owned Task identity") = Some(created.task.task_id.clone());
            registration.observed_identity(&created.task.task_id)?;
            session
                .owned_tasks
                .lock()
                .expect("owned Task registrations")
                .insert(created.task.task_id.clone(), registration);
            Ok(created.task)
        }
        other => {
            registration.settled()?;
            bail!("expected CreateTaskResult for {tool_name}, got {other:?}")
        }
    }
}

pub async fn await_task_terminal(
    session: &SmokeMcpClient,
    task_id: &str,
) -> Result<rmcp::model::DetailedTask> {
    await_task_terminal_with_timeout(session, task_id, Duration::from_secs(30)).await
}

pub async fn await_task_terminal_with_timeout(
    session: &SmokeMcpClient,
    task_id: &str,
    timeout: Duration,
) -> Result<rmcp::model::DetailedTask> {
    await_task_observation(
        session,
        task_id,
        timeout,
        Duration::from_millis(500),
        false,
        |_| {},
    )
    .await
}

pub(crate) async fn await_task_observation(
    session: &SmokeMcpClient,
    task_id: &str,
    timeout: Duration,
    poll: Duration,
    stop_at_input: bool,
    observe: impl Fn(&rmcp::model::DetailedTask),
) -> Result<rmcp::model::DetailedTask> {
    tokio::time::timeout(timeout, async {
        loop {
            crate::lifecycle::owner::check_effect()?;
            let info = session
                .get_task(rmcp::model::GetTaskParams::new(task_id))
                .await?;
            ensure!(
                info.task.task.task_id == task_id,
                "observed Task identity mismatch"
            );
            observe(&info.task);
            match info.task.status() {
                rmcp::model::TaskStatus::InputRequired if stop_at_input => return Ok(info.task),
                rmcp::model::TaskStatus::Working | rmcp::model::TaskStatus::InputRequired => {
                    tokio::time::sleep(poll).await;
                }
                _ => {
                    session.settle_task(task_id)?;
                    return Ok(info.task);
                }
            }
        }
    })
    .await
    .with_context(|| format!("task {task_id} did not reach a terminal status"))?
}

pub async fn task_payload(
    session: &SmokeMcpClient,
    task_id: &str,
) -> Result<rmcp::model::CallToolResult> {
    let result = session
        .get_task(rmcp::model::GetTaskParams::new(task_id))
        .await?;
    ensure!(
        result.task.task.task_id == task_id,
        "observed Task payload identity mismatch"
    );
    match result.task.payload {
        rmcp::model::TaskPayload::Completed { result } => {
            Ok(serde_json::from_value(Value::Object(result))?)
        }
        other => bail!("expected completed task payload for {task_id}, got {other:?}"),
    }
}

pub fn run_direct_mcp(
    conformance: &Path,
    url: &str,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let mut all_args = vec!["--url".into(), url.into()];
    all_args.extend(args);
    run_checked(conformance, all_args, envs)
}

/// A fixture declares the expected actual peer outcome, separately from token issuance.
pub fn assert_direct_mcp_denied(
    conformance: &Path,
    url: &str,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
    expected: veoveo_mcp_conformance::client::failure::ObservedFailure,
) -> Result<()> {
    use veoveo_mcp_conformance::client::failure;
    let mut all_args: Vec<OsString> = vec!["--url".into(), url.into()];
    all_args.extend(args);
    let mut envs: Vec<_> = envs.into_iter().collect();
    let bearer = envs
        .iter()
        .find(|(name, _)| *name == "MCP_BEARER_TOKEN")
        .and_then(|(_, value)| value.to_str())
        .context("denial probe requires the actual explicit bearer")?;
    let bearer_digest = failure::bearer_digest(Some(bearer));
    let arguments_digest = failure::arguments_digest(all_args.iter().cloned());
    let root = env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    let directory = root.join(format!(".denial-{}", uuid::Uuid::now_v7()));
    fs::create_dir(&directory)?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let report_path = directory.join("observed.json");
    envs.push((
        "VEOVEO_SMOKE_FAILURE_REPORT",
        report_path.as_os_str().to_owned(),
    ));
    let output = run_raw(conformance, all_args, envs)?;
    anyhow::ensure!(
        !output.status.success(),
        "denial probe unexpectedly succeeded"
    );
    let report = failure::read(&report_path)?;
    anyhow::ensure!(
        report.endpoint == url
            && report.arguments_digest == arguments_digest
            && report.bearer_digest == bearer_digest,
        "typed peer failure disagrees with selected route/request/token identity"
    );
    anyhow::ensure!(
        report.observed == expected,
        "peer failure is unrelated to expected denial: {:?}",
        report.observed
    );
    fs::remove_dir_all(directory)?;
    Ok(())
}

pub fn assert_mcp_denied(
    conformance: &Path,
    mcp_url: &str,
    token: &str,
    args: impl IntoIterator<Item = OsString>,
    expected: veoveo_mcp_conformance::client::failure::ObservedFailure,
) -> Result<()> {
    assert_direct_mcp_denied(
        conformance,
        mcp_url,
        args,
        [("MCP_BEARER_TOKEN", token.into())],
        expected,
    )
}
