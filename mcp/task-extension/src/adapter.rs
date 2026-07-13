use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Request, State};
use axum::http::{Extensions, HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use futures::{Stream, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use crate::{
    AcknowledgeTaskResult, CANCEL_TASK_METHOD, CancelTaskParams, CreateTaskResult, DISCOVER_METHOD,
    DiscoverParams, DiscoverResult, EXTENSION_ID, GET_TASK_METHOD, GetTaskParams, GetTaskResult,
    HEADER_MCP_METHOD, HEADER_MCP_NAME, HEADER_MCP_PROTOCOL_VERSION, Implementation, LISTEN_METHOD,
    ListenParams, MISSING_REQUIRED_CLIENT_CAPABILITY, PROTOCOL_VERSION, ProtocolTaskId,
    SUBSCRIPTION_ACKNOWLEDGED_METHOD, SUBSCRIPTION_ID_META_KEY, TASK_NOTIFICATION_METHOD,
    ToolCallParams, UPDATE_TASK_METHOD, UpdateTaskParams,
};

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const JSON_RPC_VERSION: &str = "2.0";

pub struct TaskSubscription {
    pub accepted_task_ids: Vec<ProtocolTaskId>,
    pub updates:
        Pin<Box<dyn Stream<Item = Result<crate::DetailedTask, AdapterError>> + Send + 'static>>,
}

pub trait TaskExtensionHandler: Send + Sync + 'static {
    type Caller: Clone + Send + Sync + 'static;

    fn authenticate(&self, extensions: &Extensions) -> Result<Self::Caller, AdapterError>;

    fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: ToolCallParams,
    ) -> impl Future<Output = Result<Option<CreateTaskResult>, AdapterError>> + Send;

    fn get_task(
        &self,
        caller: &Self::Caller,
        request: GetTaskParams,
    ) -> impl Future<Output = Result<GetTaskResult, AdapterError>> + Send;

    fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> impl Future<Output = Result<AcknowledgeTaskResult, AdapterError>> + Send;

    fn cancel_task(
        &self,
        caller: &Self::Caller,
        request: CancelTaskParams,
    ) -> impl Future<Output = Result<AcknowledgeTaskResult, AdapterError>> + Send;

    fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<ProtocolTaskId>,
    ) -> impl Future<Output = Result<TaskSubscription, AdapterError>> + Send;
}

#[derive(Clone, Debug)]
pub struct ServerDiscovery {
    pub capabilities: crate::JsonObject,
    pub server_info: Implementation,
    pub instructions: Option<String>,
}

impl ServerDiscovery {
    pub fn new(
        mut capabilities: crate::JsonObject,
        server_info: Implementation,
        instructions: Option<String>,
    ) -> Self {
        let extensions = capabilities
            .entry("extensions".to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(extensions) = extensions {
            extensions.insert(EXTENSION_ID.to_owned(), Value::Object(Map::new()));
        } else {
            *extensions = json!({EXTENSION_ID: {}});
        }
        Self {
            capabilities,
            server_info,
            instructions,
        }
    }

    fn result(&self) -> DiscoverResult {
        DiscoverResult::new(
            self.capabilities.clone(),
            self.server_info.clone(),
            self.instructions.clone(),
        )
    }
}

#[derive(Clone)]
pub struct TaskExtensionAdapter<H> {
    handler: Arc<H>,
    discovery: ServerDiscovery,
}

impl<H> TaskExtensionAdapter<H> {
    pub fn new(handler: Arc<H>, discovery: ServerDiscovery) -> Self {
        Self { handler, discovery }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AdapterError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
    pub http_status: StatusCode,
}

impl AdapterError {
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32_602,
            message: message.into(),
            data: None,
            http_status: StatusCode::BAD_REQUEST,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            code: -32_600,
            message: message.into(),
            data: None,
            http_status: StatusCode::UNAUTHORIZED,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32_603,
            message: message.into(),
            data: None,
            http_status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn missing_task_capability() -> Self {
        Self {
            code: MISSING_REQUIRED_CLIENT_CAPABILITY,
            message: "missing required client capability".to_owned(),
            data: Some(json!({
                "requiredCapabilities": {
                    "extensions": { EXTENSION_ID: {} }
                }
            })),
            http_status: StatusCode::BAD_REQUEST,
        }
    }
}

pub async fn task_extension_middleware<H>(
    State(adapter): State<Arc<TaskExtensionAdapter<H>>>,
    request: Request,
    next: Next,
) -> Response
where
    H: TaskExtensionHandler,
{
    let (mut parts, body) = request.into_parts();
    let bytes = match to_bytes(body, MAX_REQUEST_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return error_response(
                None,
                AdapterError::invalid_params(format!("reading request body failed: {error}")),
            );
        }
    };
    let rpc: RpcRequest = match serde_json::from_slice(&bytes) {
        Ok(rpc) => rpc,
        Err(_) => {
            return next
                .run(Request::from_parts(parts, Body::from(bytes)))
                .await;
        }
    };
    if rpc.jsonrpc != JSON_RPC_VERSION {
        return error_response(
            Some(rpc.id),
            AdapterError::invalid_params("jsonrpc must be `2.0`"),
        );
    }

    let handled = matches!(
        rpc.method.as_str(),
        DISCOVER_METHOD | GET_TASK_METHOD | UPDATE_TASK_METHOD | CANCEL_TASK_METHOD | LISTEN_METHOD
    );
    let extension_request = request_protocol_version(&rpc.params).is_some();
    if !handled && !extension_request {
        return next
            .run(Request::from_parts(parts, Body::from(bytes)))
            .await;
    }

    let caller = match adapter.handler.authenticate(&parts.extensions) {
        Ok(caller) => caller,
        Err(error) => return error_response(Some(rpc.id), error),
    };

    if (handled || extension_request)
        && let Err(error) = validate_protocol(&parts.headers, &rpc)
    {
        return error_response(Some(rpc.id), error);
    }

    match rpc.method.as_str() {
        DISCOVER_METHOD => {
            if let Err(error) = parse_params::<DiscoverParams>(&rpc.params) {
                return error_response(Some(rpc.id), error);
            }
            json_result(rpc.id, adapter.discovery.result())
        }
        GET_TASK_METHOD => {
            let params = match parse_params::<GetTaskParams>(&rpc.params) {
                Ok(params) => params,
                Err(error) => return error_response(Some(rpc.id), error),
            };
            if let Err(error) = require_task_capability(&params.meta) {
                return error_response(Some(rpc.id), error);
            }
            if let Err(error) =
                validate_task_routing(&parts.headers, GET_TASK_METHOD, params.task_id)
            {
                return error_response(Some(rpc.id), error);
            }
            match adapter.handler.get_task(&caller, params).await {
                Ok(result) => json_result(rpc.id, result),
                Err(error) => error_response(Some(rpc.id), error),
            }
        }
        UPDATE_TASK_METHOD => {
            let params = match parse_params::<UpdateTaskParams>(&rpc.params) {
                Ok(params) => params,
                Err(error) => return error_response(Some(rpc.id), error),
            };
            if let Err(error) = require_task_capability(&params.meta) {
                return error_response(Some(rpc.id), error);
            }
            if let Err(error) =
                validate_task_routing(&parts.headers, UPDATE_TASK_METHOD, params.task_id)
            {
                return error_response(Some(rpc.id), error);
            }
            match adapter.handler.update_task(&caller, params).await {
                Ok(result) => json_result(rpc.id, result),
                Err(error) => error_response(Some(rpc.id), error),
            }
        }
        CANCEL_TASK_METHOD => {
            let params = match parse_params::<CancelTaskParams>(&rpc.params) {
                Ok(params) => params,
                Err(error) => return error_response(Some(rpc.id), error),
            };
            if let Err(error) = require_task_capability(&params.meta) {
                return error_response(Some(rpc.id), error);
            }
            if let Err(error) =
                validate_task_routing(&parts.headers, CANCEL_TASK_METHOD, params.task_id)
            {
                return error_response(Some(rpc.id), error);
            }
            match adapter.handler.cancel_task(&caller, params).await {
                Ok(result) => json_result(rpc.id, result),
                Err(error) => error_response(Some(rpc.id), error),
            }
        }
        LISTEN_METHOD => {
            let params = match parse_params::<ListenParams>(&rpc.params) {
                Ok(params) => params,
                Err(error) => return error_response(Some(rpc.id), error),
            };
            if let Err(error) = require_task_capability(&params.meta) {
                return error_response(Some(rpc.id), error);
            }
            let task_ids = match params.notifications.task_ids {
                Some(task_ids) => task_ids,
                None => {
                    return error_response(
                        Some(rpc.id),
                        AdapterError::invalid_params(
                            "subscriptions/listen requires notifications.taskIds",
                        ),
                    );
                }
            };
            match adapter.handler.subscribe_tasks(&caller, task_ids).await {
                Ok(subscription) => subscription_response(rpc.id, subscription),
                Err(error) => error_response(Some(rpc.id), error),
            }
        }
        "tools/call" => {
            let params = match parse_params::<ToolCallParams>(&rpc.params) {
                Ok(params) => params,
                Err(error) => return error_response(Some(rpc.id), error),
            };
            if let Err(error) = validate_named_routing(&parts.headers, "tools/call", &params.name) {
                return error_response(Some(rpc.id), error);
            }
            if params.meta.declares_tasks() {
                match adapter.handler.start_tool_task(&caller, params).await {
                    Ok(Some(result)) => return json_result(rpc.id, result),
                    Ok(None) => {}
                    Err(error) => return error_response(Some(rpc.id), error),
                }
            }
            parts.headers.remove(HEADER_MCP_PROTOCOL_VERSION);
            let body = serde_json::to_vec(&rpc).expect("RpcRequest serializes");
            next.run(Request::from_parts(parts, Body::from(body))).await
        }
        _ => {
            parts.headers.remove(HEADER_MCP_PROTOCOL_VERSION);
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RpcRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

fn parse_params<T>(params: &Option<Value>) -> Result<T, AdapterError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(params.clone().unwrap_or_else(|| json!({})))
        .map_err(|error| AdapterError::invalid_params(error.to_string()))
}

fn request_protocol_version(params: &Option<Value>) -> Option<String> {
    params
        .as_ref()?
        .get("_meta")?
        .get(crate::PROTOCOL_VERSION_META_KEY)?
        .as_str()
        .map(str::to_owned)
}

fn validate_protocol(headers: &HeaderMap, rpc: &RpcRequest) -> Result<(), AdapterError> {
    let header_version = header_value(headers, HEADER_MCP_PROTOCOL_VERSION)?;
    if header_version != PROTOCOL_VERSION {
        return Err(AdapterError::invalid_params(format!(
            "MCP-Protocol-Version must be `{PROTOCOL_VERSION}`"
        )));
    }
    if request_protocol_version(&rpc.params).as_deref() != Some(PROTOCOL_VERSION) {
        return Err(AdapterError::invalid_params(format!(
            "request _meta protocol version must be `{PROTOCOL_VERSION}`"
        )));
    }
    let method = header_value(headers, HEADER_MCP_METHOD)?;
    if method != rpc.method {
        return Err(AdapterError::invalid_params(
            "Mcp-Method header does not match JSON-RPC method",
        ));
    }
    Ok(())
}

fn validate_task_routing(
    headers: &HeaderMap,
    method: &str,
    task_id: ProtocolTaskId,
) -> Result<(), AdapterError> {
    validate_named_routing(headers, method, &task_id.to_string())
}

fn validate_named_routing(
    headers: &HeaderMap,
    method: &str,
    name: &str,
) -> Result<(), AdapterError> {
    if header_value(headers, HEADER_MCP_METHOD)? != method {
        return Err(AdapterError::invalid_params(
            "Mcp-Method header does not match JSON-RPC method",
        ));
    }
    if header_value(headers, HEADER_MCP_NAME)? != name {
        return Err(AdapterError::invalid_params(
            "Mcp-Name header does not match the routed task or tool",
        ));
    }
    Ok(())
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, AdapterError> {
    headers
        .get(name)
        .ok_or_else(|| AdapterError::invalid_params(format!("missing {name} header")))?
        .to_str()
        .map_err(|_| AdapterError::invalid_params(format!("invalid {name} header")))
}

fn require_task_capability(meta: &crate::RequestMeta) -> Result<(), AdapterError> {
    if meta.declares_tasks() {
        Ok(())
    } else {
        Err(AdapterError::missing_task_capability())
    }
}

fn json_result<T>(id: Value, result: T) -> Response
where
    T: serde::Serialize,
{
    json_response(
        StatusCode::OK,
        json!({"jsonrpc": JSON_RPC_VERSION, "id": id, "result": result}),
    )
}

fn error_response(id: Option<Value>, error: AdapterError) -> Response {
    json_response(
        error.http_status,
        json!({
            "jsonrpc": JSON_RPC_VERSION,
            "id": id.unwrap_or(Value::Null),
            "error": {
                "code": error.code,
                "message": error.message,
                "data": error.data,
            }
        }),
    )
}

fn json_response(status: StatusCode, value: Value) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&value).expect("JSON-RPC response serializes"),
        ))
        .expect("valid JSON response")
}

fn subscription_response(id: Value, mut subscription: TaskSubscription) -> Response {
    let accepted = subscription.accepted_task_ids.clone();
    let stream = async_stream::stream! {
        let acknowledged = json!({
            "jsonrpc": JSON_RPC_VERSION,
            "method": SUBSCRIPTION_ACKNOWLEDGED_METHOD,
            "params": {
                "_meta": { SUBSCRIPTION_ID_META_KEY: id.clone() },
                "notifications": { "taskIds": accepted },
            }
        });
        yield Ok::<Bytes, Infallible>(sse_message(&acknowledged));
        while let Some(update) = subscription.updates.next().await {
            let Ok(task) = update else {
                break;
            };
            let mut params = match serde_json::to_value(task) {
                Ok(Value::Object(params)) => params,
                _ => break,
            };
            params.insert(
                "_meta".to_owned(),
                json!({SUBSCRIPTION_ID_META_KEY: id.clone()}),
            );
            let notification = json!({
                "jsonrpc": JSON_RPC_VERSION,
                "method": TASK_NOTIFICATION_METHOD,
                "params": params,
            });
            yield Ok::<Bytes, Infallible>(sse_message(&notification));
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(stream))
        .expect("valid SSE response")
}

fn sse_message(value: &Value) -> Bytes {
    Bytes::from(format!(
        "event: message\ndata: {}\n\n",
        serde_json::to_string(value).expect("notification serializes")
    ))
}
