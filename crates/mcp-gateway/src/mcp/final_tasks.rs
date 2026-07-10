use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use futures::{Stream, StreamExt};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use rmcp::model::ErrorData as McpError;
use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use veoveo_mcp_task_extension::{
    AcknowledgeTaskResult, CANCEL_TASK_METHOD, CancelTaskParams, CreateTaskResult, DISCOVER_METHOD,
    DetailedTask, DiscoverParams, DiscoverResult, EXTENSION_ID, GET_TASK_METHOD, GetTaskParams,
    GetTaskResult, HEADER_MCP_METHOD, HEADER_MCP_NAME, HEADER_MCP_PROTOCOL_VERSION, InputResponses,
    LISTEN_METHOD, ListenParams, NotificationSelection, PROTOCOL_VERSION, ProtocolTaskId,
    RequestMeta, SUBSCRIPTION_ACKNOWLEDGED_METHOD, TASK_NOTIFICATION_METHOD, ToolCallParams,
    UPDATE_TASK_METHOD, UpdateTaskParams,
};

use crate::GatewayCatalog;

use super::upstream_http::build_upstream_http_client;
use veoveo_mcp_contract::ServerManifest;

pub(super) type FinalTaskStream =
    Pin<Box<dyn Stream<Item = Result<DetailedTask, McpError>> + Send + 'static>>;

#[derive(Clone)]
pub struct FinalTaskClient {
    http: reqwest::Client,
    endpoint: String,
    bearer_token: SecretString,
    request_ids: Arc<AtomicU64>,
}

impl std::fmt::Debug for FinalTaskClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FinalTaskClient")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl FinalTaskClient {
    pub async fn for_server(
        catalog: &GatewayCatalog,
        server: &ServerManifest,
        bearer_token: String,
    ) -> Result<Self, McpError> {
        let http = build_upstream_http_client(catalog, server).await?;
        Ok(Self::new(http, server.upstream.url.as_str(), bearer_token))
    }

    pub(super) fn new(
        http: reqwest::Client,
        endpoint: impl Into<String>,
        bearer_token: String,
    ) -> Self {
        Self {
            http,
            endpoint: endpoint.into(),
            bearer_token: bearer_token.into(),
            request_ids: Arc::new(AtomicU64::new(1)),
        }
    }

    pub(super) async fn discover(&self) -> Result<DiscoverResult, McpError> {
        let result: DiscoverResult = self
            .request(
                DISCOVER_METHOD,
                None,
                &DiscoverParams {
                    meta: RequestMeta::new(),
                },
            )
            .await?;
        let capabilities = result
            .capabilities
            .get("extensions")
            .and_then(Value::as_object);
        if result
            .supported_versions
            .iter()
            .all(|version| version != PROTOCOL_VERSION)
            || !capabilities.is_some_and(|extensions| extensions.contains_key(EXTENSION_ID))
        {
            return Err(McpError::invalid_request(
                "upstream does not support the final MCP task extension",
                None,
            ));
        }
        Ok(result)
    }

    pub(super) async fn start_tool(
        &self,
        request: ToolCallParams,
    ) -> Result<CreateTaskResult, McpError> {
        self.discover().await?;
        self.request("tools/call", Some(&request.name), &request)
            .await
    }

    pub(super) async fn get(&self, task_id: ProtocolTaskId) -> Result<DetailedTask, McpError> {
        let result: GetTaskResult = self
            .request(
                GET_TASK_METHOD,
                Some(&task_id.to_string()),
                &GetTaskParams {
                    meta: task_meta(),
                    task_id,
                },
            )
            .await?;
        Ok(result.task)
    }

    // The Rig projection has no input-update call, but the canonical client must.
    #[allow(dead_code)]
    pub(super) async fn update(
        &self,
        task_id: ProtocolTaskId,
        input_responses: InputResponses,
    ) -> Result<AcknowledgeTaskResult, McpError> {
        self.request(
            UPDATE_TASK_METHOD,
            Some(&task_id.to_string()),
            &UpdateTaskParams {
                meta: task_meta(),
                task_id,
                input_responses,
            },
        )
        .await
    }

    pub async fn cancel(&self, task_id: ProtocolTaskId) -> Result<AcknowledgeTaskResult, McpError> {
        self.request(
            CANCEL_TASK_METHOD,
            Some(&task_id.to_string()),
            &CancelTaskParams {
                meta: task_meta(),
                task_id,
            },
        )
        .await
    }

    pub(super) async fn subscribe(
        &self,
        task_ids: Vec<ProtocolTaskId>,
    ) -> Result<FinalTaskStream, McpError> {
        let request_id = self.next_request_id();
        let params = ListenParams {
            meta: task_meta(),
            notifications: NotificationSelection {
                task_ids: Some(task_ids),
            },
        };
        let response = self
            .request_builder(LISTEN_METHOD, None, request_id, &params)
            .header(ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(http_error)?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        let mut chunks = response.bytes_stream();
        Ok(Box::pin(async_stream::stream! {
            let mut buffer = Vec::new();
            while let Some(chunk) = chunks.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        yield Err(http_error(error));
                        break;
                    }
                };
                buffer.extend_from_slice(&chunk);
                while let Some((end, delimiter_len)) = next_sse_event(&buffer) {
                    let event = buffer.drain(..end).collect::<Vec<_>>();
                    buffer.drain(..delimiter_len);
                    match parse_sse_task(&event) {
                        Ok(Some(task)) => yield Ok(task),
                        Ok(None) => {}
                        Err(error) => {
                            yield Err(error);
                            return;
                        }
                    }
                }
            }
        }))
    }

    async fn request<T, P>(
        &self,
        method: &str,
        name: Option<&str>,
        params: &P,
    ) -> Result<T, McpError>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let response = self
            .request_builder(method, name, self.next_request_id(), params)
            .send()
            .await
            .map_err(http_error)?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        let envelope: RpcResponse<T> = response.json().await.map_err(http_error)?;
        envelope.into_result()
    }

    fn request_builder<P>(
        &self,
        method: &str,
        name: Option<&str>,
        request_id: u64,
        params: &P,
    ) -> reqwest::RequestBuilder
    where
        P: Serialize + ?Sized,
    {
        let mut builder = self
            .http
            .post(&self.endpoint)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json, text/event-stream")
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.bearer_token.expose_secret()),
            )
            .header(HEADER_MCP_PROTOCOL_VERSION, PROTOCOL_VERSION)
            .header(HEADER_MCP_METHOD, method)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": params,
            }));
        if let Some(name) = name {
            builder = builder.header(HEADER_MCP_NAME, name);
        }
        builder
    }

    fn next_request_id(&self) -> u64 {
        self.request_ids.fetch_add(1, Ordering::Relaxed)
    }
}

fn task_meta() -> RequestMeta {
    RequestMeta::new().with_task_capability()
}

#[derive(serde::Deserialize)]
struct RpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    result: Option<T>,
    error: Option<veoveo_mcp_task_extension::JsonRpcErrorData>,
}

impl<T> RpcResponse<T> {
    fn into_result(self) -> Result<T, McpError> {
        match (self.result, self.error) {
            (Some(result), None) => Ok(result),
            (_, Some(error)) => Err(McpError::new(
                rmcp::model::ErrorCode(error.code),
                error.message,
                error.data,
            )),
            _ => Err(McpError::internal_error(
                "upstream returned an invalid JSON-RPC response",
                None,
            )),
        }
    }
}

async fn response_error(response: reqwest::Response) -> McpError {
    let status = response.status();
    match response.json::<RpcResponse<Value>>().await {
        Ok(envelope) => envelope.into_result().err().unwrap_or_else(|| {
            McpError::internal_error(format!("upstream returned HTTP {status}"), None)
        }),
        Err(_) => McpError::internal_error(format!("upstream returned HTTP {status}"), None),
    }
}

fn http_error(error: reqwest::Error) -> McpError {
    McpError::internal_error(format!("upstream task request failed: {error}"), None)
}

fn next_sse_event(buffer: &[u8]) -> Option<(usize, usize)> {
    buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2))
        .or_else(|| {
            buffer
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| (position, 4))
        })
}

fn parse_sse_task(event: &[u8]) -> Result<Option<DetailedTask>, McpError> {
    let event = std::str::from_utf8(event).map_err(|error| {
        McpError::internal_error(format!("invalid upstream SSE: {error}"), None)
    })?;
    let data = event
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() {
        return Ok(None);
    }
    let notification: RpcNotification = serde_json::from_str(&data).map_err(|error| {
        McpError::internal_error(format!("invalid upstream task notification: {error}"), None)
    })?;
    match notification.method.as_str() {
        SUBSCRIPTION_ACKNOWLEDGED_METHOD => Ok(None),
        TASK_NOTIFICATION_METHOD => {
            let mut params = notification.params;
            if let Value::Object(params) = &mut params {
                params.remove("_meta");
            }
            serde_json::from_value(params).map(Some).map_err(|error| {
                McpError::internal_error(
                    format!("invalid upstream detailed task notification: {error}"),
                    None,
                )
            })
        }
        method => Err(McpError::internal_error(
            format!("unexpected upstream subscription notification `{method}`"),
            None,
        )),
    }
}

#[derive(serde::Deserialize)]
struct RpcNotification {
    method: String,
    params: Value,
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};

    use super::*;

    #[derive(Clone, Default)]
    struct RequestCapture(Arc<Mutex<Option<(HeaderMap, Value)>>>);

    async fn capture_task_request(
        State(capture): State<RequestCapture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        *capture.0.lock().expect("capture lock") = Some((headers, body.clone()));
        Json(json!({
            "jsonrpc": "2.0",
            "id": body["id"],
            "result": AcknowledgeTaskResult::complete(),
        }))
    }

    #[tokio::test]
    async fn cancellation_uses_the_final_task_extension_transport() {
        let capture = RequestCapture::default();
        let app = Router::new()
            .route("/mcp", post(capture_task_request))
            .with_state(capture.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake final-task server");
        let address = listener.local_addr().expect("fake server address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve requests") });
        let task_id = ProtocolTaskId::new();
        let client = FinalTaskClient::new(
            reqwest::Client::new(),
            format!("http://{address}/mcp"),
            "internal-secret".to_owned(),
        );

        let result = client.cancel(task_id).await.expect("cancel task");

        assert_eq!(result, AcknowledgeTaskResult::complete());
        let (headers, body) = capture
            .0
            .lock()
            .expect("capture lock")
            .clone()
            .expect("captured task request");
        assert_eq!(headers[AUTHORIZATION], "Bearer internal-secret");
        assert_eq!(headers[HEADER_MCP_METHOD], CANCEL_TASK_METHOD);
        assert_eq!(headers[HEADER_MCP_PROTOCOL_VERSION], PROTOCOL_VERSION);
        assert_eq!(headers[HEADER_MCP_NAME], task_id.to_string());
        assert_eq!(body["method"], CANCEL_TASK_METHOD);
        assert_eq!(body["params"]["taskId"], task_id.to_string());
    }

    #[test]
    fn parses_final_task_notification() {
        let id = ProtocolTaskId::new();
        let event = format!(
            "event: message\ndata: {}",
            json!({
                "jsonrpc": "2.0",
                "method": TASK_NOTIFICATION_METHOD,
                "params": {
                    "_meta": {"io.modelcontextprotocol/subscriptionId": 1},
                    "status": "working",
                    "taskId": id,
                    "createdAt": "2026-07-09T00:00:00Z",
                    "lastUpdatedAt": "2026-07-09T00:00:00Z"
                }
            })
        );
        let parsed = parse_sse_task(event.as_bytes()).unwrap().unwrap();
        assert_eq!(parsed.metadata().task_id, id);
    }
}
