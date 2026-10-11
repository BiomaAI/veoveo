use std::collections::HashMap;

use anyhow::ensure;
use reqwest::header::{HOST, HeaderValue};
use rmcp::model::{CallToolResult, TaskPayload};

use super::*;

mod delivery;
pub mod public_caller;
pub use delivery::{
    DeliveredTaskResult, RecoveredTaskResult, ResourceSnapshotDelivery, WorkingCheckpoint,
    WorkingTaskRecovery,
};

/// Full-profile task client used by smoke scenarios that address a hosted
/// server directly. It uses rmcp's Discover lifecycle and official Tasks
/// methods; no duplicate JSON-RPC or task wire model lives in the harness.
pub struct FinalTaskSmokeClient {
    endpoint: String,
    bearer_token: String,
    host: Option<HeaderValue>,
}

impl FinalTaskSmokeClient {
    /// Admit an owner-private OAuth token without exposing it in caller diagnostics.
    pub fn from_private_token_file(endpoint: &veoveo_types::HttpsUrl, path: &Path) -> Result<Self> {
        let header = crate::installed::knowledge::bearer_header(path)
            .map_err(|_| anyhow!("private bearer file admission failed"))?;
        let bearer = header
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix("Bearer "))
            .context("private bearer header admission failed")?;
        Ok(Self::new(endpoint.as_str(), bearer.to_owned()))
    }
    pub fn new(endpoint: &str, bearer_token: String) -> Self {
        Self {
            endpoint: endpoint.to_owned(),
            bearer_token,
            host: None,
        }
    }

    pub fn with_host(mut self, host: &'static str) -> Self {
        self.host = Some(HeaderValue::from_static(host));
        self
    }

    pub async fn connect(&self) -> Result<SmokeMcpClient> {
        let mut config = StreamableHttpClientTransportConfig::with_uri(self.endpoint.clone())
            .auth_header(self.bearer_token.clone());
        if let Some(host) = &self.host {
            config = config.custom_headers(HashMap::from([(HOST, host.clone())]));
        }
        let client = tokio::time::timeout(
            Duration::from_secs(30),
            SmokeMcpHandler.serve_with_lifecycle(
                StreamableHttpClientTransport::with_client(
                    crate::installed::knowledge::mcp_http_client_builder().build()?,
                    config,
                ),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                },
            ),
        )
        .await
        .context("MCP discovery exceeded 30 seconds")??;
        Ok(SmokeMcpClient::admitted(
            client,
            &self.endpoint,
            &self.bearer_token,
            self.host.clone(),
        ))
    }

    pub async fn run_tool(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<CallToolResult> {
        let client = self.connect().await?;
        ensure!(
            client
                .peer_info()
                .is_some_and(|info| info.capabilities.supports_tasks()),
            "server does not advertise official MCP Tasks"
        );

        let created = call_tool_as_task(&client, name, arguments).await?;
        let poll_ms = created.poll_interval_ms.unwrap_or(100).clamp(10, 5_000);
        let task_id = created.task_id;
        let terminal = super::mcp::await_task_observation(
            &client,
            &task_id,
            timeout,
            Duration::from_millis(poll_ms),
            true,
            |task| {
                println!(
                    "task {task_id}: {:?} {}",
                    task.status(),
                    task.task.status_message.as_deref().unwrap_or("")
                )
            },
        )
        .await
        .with_context(|| format!("timed out waiting for task {task_id}"))?;
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

    pub async fn read_resource<T: serde::de::DeserializeOwned>(
        &self,
        uri: &veoveo_types::ResourceUri,
    ) -> Result<T> {
        self.read_resource_owned(uri).await
    }

    pub async fn run_tool_structured(
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
