use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use anyhow::ensure;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use veoveo_mcp_task_extension::{
    CreateTaskResult, DISCOVER_METHOD, DetailedTask, DiscoverParams, DiscoverResult, EXTENSION_ID,
    GET_TASK_METHOD, GetTaskParams, GetTaskResult, HEADER_MCP_METHOD, HEADER_MCP_NAME,
    HEADER_MCP_PROTOCOL_VERSION, PROTOCOL_VERSION, RequestMeta, ToolCallParams,
};

use super::*;

pub(crate) struct FinalTaskSmokeClient {
    http: reqwest::Client,
    endpoint: String,
    bearer_token: String,
    request_ids: Arc<AtomicU64>,
}

impl FinalTaskSmokeClient {
    pub(crate) fn new(endpoint: &str, bearer_token: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.to_owned(),
            bearer_token,
            request_ids: Arc::new(AtomicU64::new(1)),
        }
    }

    pub(crate) async fn run_tool(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<CallToolResult> {
        self.discover().await?;
        let arguments = arguments
            .as_object()
            .context("task tool arguments are not an object")?
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let created: CreateTaskResult = self
            .request(
                "tools/call",
                Some(name),
                &ToolCallParams {
                    meta: RequestMeta::new().with_task_capability(),
                    name: name.to_owned(),
                    arguments,
                },
            )
            .await?;
        let task_id = created.task.task_id;
        let poll_ms = created
            .task
            .poll_interval_ms
            .unwrap_or(100)
            .clamp(10, 5_000);
        let task = tokio::time::timeout(timeout, async {
            loop {
                let task = self.get_task(task_id).await?;
                println!(
                    "task {task_id}: {:?} {}",
                    task.status(),
                    task.metadata().status_message.as_deref().unwrap_or("")
                );
                match task {
                    DetailedTask::Working { .. } => {
                        tokio::time::sleep(Duration::from_millis(poll_ms)).await;
                    }
                    terminal => return Ok::<_, anyhow::Error>(terminal),
                }
            }
        })
        .await
        .with_context(|| format!("timed out waiting for task {task_id}"))??;

        match task {
            DetailedTask::Completed { result, .. } => {
                let result: CallToolResult = serde_json::from_value(Value::Object(
                    result.into_iter().collect::<Map<String, Value>>(),
                ))?;
                ensure!(
                    result.is_error != Some(true),
                    "task tool returned an error: {:?}",
                    result.content
                );
                Ok(result)
            }
            DetailedTask::Failed { error, .. } => {
                bail!("task failed ({}): {}", error.code, error.message)
            }
            DetailedTask::Cancelled { .. } => bail!("task was cancelled"),
            DetailedTask::InputRequired { .. } => bail!("task unexpectedly requested input"),
            DetailedTask::Working { .. } => unreachable!("task wait returns a terminal state"),
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

    async fn discover(&self) -> Result<()> {
        let result: DiscoverResult = self
            .request(
                DISCOVER_METHOD,
                None,
                &DiscoverParams {
                    meta: RequestMeta::new(),
                },
            )
            .await?;
        let extensions = result
            .capabilities
            .get("extensions")
            .and_then(Value::as_object);
        ensure!(
            result
                .supported_versions
                .iter()
                .any(|version| version == PROTOCOL_VERSION)
                && extensions.is_some_and(|extensions| extensions.contains_key(EXTENSION_ID)),
            "server does not advertise final MCP tasks"
        );
        Ok(())
    }

    async fn get_task(
        &self,
        task_id: veoveo_mcp_task_extension::ProtocolTaskId,
    ) -> Result<DetailedTask> {
        let result: GetTaskResult = self
            .request(
                GET_TASK_METHOD,
                Some(&task_id.to_string()),
                &GetTaskParams {
                    meta: RequestMeta::new().with_task_capability(),
                    task_id,
                },
            )
            .await?;
        Ok(result.task)
    }

    async fn request<T, P>(&self, method: &str, name: Option<&str>, params: &P) -> Result<T>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let mut request = self
            .http
            .post(&self.endpoint)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .header(HEADER_MCP_PROTOCOL_VERSION, PROTOCOL_VERSION)
            .header(HEADER_MCP_METHOD, method)
            .header(AUTHORIZATION, format!("Bearer {}", self.bearer_token))
            .json(&json!({
                "jsonrpc": "2.0",
                "id": self.request_ids.fetch_add(1, Ordering::Relaxed),
                "method": method,
                "params": params,
            }));
        if let Some(name) = name {
            request = request.header(HEADER_MCP_NAME, name);
        }
        let response = request.send().await?;
        let status = response.status();
        let envelope: RpcResponse<T> = response.json().await?;
        match (envelope.result, envelope.error) {
            (Some(result), None) if status.is_success() => Ok(result),
            (_, Some(error)) => bail!(
                "task extension request `{method}` failed ({}): {}",
                error.code,
                error.message
            ),
            _ => bail!("task extension request `{method}` returned invalid HTTP {status}"),
        }
    }
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<veoveo_mcp_task_extension::JsonRpcErrorData>,
}
