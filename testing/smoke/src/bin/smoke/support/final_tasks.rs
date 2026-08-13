use std::collections::HashMap;

use anyhow::ensure;
use reqwest::header::{HOST, HeaderValue};
use rmcp::model::{CallToolResponse, CallToolResult, TaskPayload, TaskStatus};

use super::*;

/// Full-profile task client used by smoke scenarios that address a hosted
/// server directly. It uses rmcp's Discover lifecycle and official Tasks
/// methods; no duplicate JSON-RPC or task wire model lives in the harness.
pub(crate) struct FinalTaskSmokeClient {
    endpoint: String,
    bearer_token: String,
    host: Option<HeaderValue>,
}

impl FinalTaskSmokeClient {
    pub(crate) fn new(endpoint: &str, bearer_token: String) -> Self {
        Self {
            endpoint: endpoint.to_owned(),
            bearer_token,
            host: None,
        }
    }

    pub(crate) fn with_host(mut self, host: &'static str) -> Self {
        self.host = Some(HeaderValue::from_static(host));
        self
    }

    pub(crate) async fn run_tool(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<CallToolResult> {
        let mut config = StreamableHttpClientTransportConfig::with_uri(self.endpoint.clone())
            .auth_header(self.bearer_token.clone());
        if let Some(host) = &self.host {
            config = config.custom_headers(HashMap::from([(HOST, host.clone())]));
        }
        let client = SmokeMcpHandler
            .serve_with_lifecycle(
                StreamableHttpClientTransport::from_config(config),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                },
            )
            .await?;
        ensure!(
            client
                .peer_info()
                .is_some_and(|info| info.capabilities.supports_tasks()),
            "server does not advertise official MCP Tasks"
        );

        let arguments = arguments
            .as_object()
            .context("task tool arguments are not an object")?
            .clone();
        let created = match client
            .call_tool_once(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments))
            .await?
        {
            CallToolResponse::Task(created) => created.task,
            other => bail!("expected task result from {name}, got {other:?}"),
        };
        let task_id = created.task_id;
        let poll_ms = created.poll_interval_ms.unwrap_or(100).clamp(10, 5_000);
        let terminal = tokio::time::timeout(timeout, async {
            loop {
                let task = client
                    .get_task(rmcp::model::GetTaskParams::new(&task_id))
                    .await?
                    .task;
                println!(
                    "task {task_id}: {:?} {}",
                    task.status(),
                    task.task.status_message.as_deref().unwrap_or("")
                );
                if task.status() == TaskStatus::Working {
                    tokio::time::sleep(Duration::from_millis(poll_ms)).await;
                    continue;
                }
                return Ok::<_, anyhow::Error>(task);
            }
        })
        .await
        .with_context(|| format!("timed out waiting for task {task_id}"))??;
        client.cancel().await?;

        match terminal.payload {
            TaskPayload::Completed { result } => {
                let result: CallToolResult = serde_json::from_value(Value::Object(result))?;
                ensure!(
                    result.is_error != Some(true),
                    "task tool returned an error: {:?}",
                    result.content
                );
                Ok(result)
            }
            TaskPayload::Failed { error } => bail!("task failed: {error:?}"),
            TaskPayload::Cancelled => bail!("task was cancelled"),
            TaskPayload::InputRequired { input_requests } => {
                bail!("task unexpectedly requested input: {input_requests:?}")
            }
            TaskPayload::Working => unreachable!("task wait returns a non-working state"),
            other => bail!("task returned an unsupported payload: {other:?}"),
        }
    }

    pub(crate) async fn run_tool_structured(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        self.run_tool(name, arguments, timeout)
            .await?
            .structured_content
            .context("task completed without structured content")
    }
}
