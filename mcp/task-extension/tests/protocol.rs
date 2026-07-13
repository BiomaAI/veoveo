use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::{Router, middleware};
use futures::stream;
use serde_json::{Value, json};
use tower::ServiceExt;
use veoveo_mcp_task_extension::{
    AcknowledgeTaskResult, AdapterError, CancelTaskParams, CreateTaskResult, DetailedTask,
    GetTaskParams, GetTaskResult, Implementation, PROTOCOL_VERSION, ProtocolTaskId,
    ServerDiscovery, Task, TaskExtensionAdapter, TaskExtensionHandler, TaskMetadata, TaskStatus,
    TaskSubscription, ToolCallParams, UpdateTaskParams, task_extension_middleware,
};

#[derive(Clone)]
struct FakeHandler {
    task_id: ProtocolTaskId,
    authentications: Arc<AtomicUsize>,
}

impl FakeHandler {
    fn task(&self) -> Task {
        let now = chrono::Utc::now();
        Task {
            task_id: self.task_id,
            status: TaskStatus::Working,
            status_message: Some("working".to_owned()),
            created_at: now,
            last_updated_at: now,
            ttl_ms: Some(60_000),
            poll_interval_ms: Some(3_000),
        }
    }

    fn detailed(&self) -> DetailedTask {
        let task = self.task();
        DetailedTask::Working {
            metadata: TaskMetadata {
                task_id: task.task_id,
                status_message: task.status_message,
                created_at: task.created_at,
                last_updated_at: task.last_updated_at,
                ttl_ms: task.ttl_ms,
                poll_interval_ms: task.poll_interval_ms,
            },
        }
    }
}

impl TaskExtensionHandler for FakeHandler {
    type Caller = ();

    fn authenticate(&self, _extensions: &http::Extensions) -> Result<Self::Caller, AdapterError> {
        self.authentications.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn start_tool_task(
        &self,
        _caller: &Self::Caller,
        request: ToolCallParams,
    ) -> Result<Option<CreateTaskResult>, AdapterError> {
        Ok((request.name == "forecast").then(|| CreateTaskResult::new(self.task())))
    }

    async fn get_task(
        &self,
        _caller: &Self::Caller,
        _request: GetTaskParams,
    ) -> Result<GetTaskResult, AdapterError> {
        Ok(GetTaskResult::new(self.detailed()))
    }

    async fn update_task(
        &self,
        _caller: &Self::Caller,
        _request: UpdateTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        Ok(AcknowledgeTaskResult::complete())
    }

    async fn cancel_task(
        &self,
        _caller: &Self::Caller,
        _request: CancelTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        Ok(AcknowledgeTaskResult::complete())
    }

    async fn subscribe_tasks(
        &self,
        _caller: &Self::Caller,
        task_ids: Vec<ProtocolTaskId>,
    ) -> Result<TaskSubscription, AdapterError> {
        Ok(TaskSubscription {
            accepted_task_ids: task_ids,
            updates: Box::pin(stream::iter([Ok(self.detailed())])),
        })
    }
}

fn app(handler: FakeHandler) -> Router {
    let discovery = ServerDiscovery::new(
        std::collections::BTreeMap::from([
            ("tools".to_owned(), json!({})),
            ("resources".to_owned(), json!({})),
        ]),
        Implementation {
            name: "test-server".to_owned(),
            version: "1.0.0".to_owned(),
        },
        None,
    );
    let adapter = Arc::new(TaskExtensionAdapter::new(Arc::new(handler), discovery));
    Router::new()
        .fallback(|| async { axum::Json(json!({"forwarded": true})) })
        .layer(middleware::from_fn_with_state(
            adapter,
            task_extension_middleware::<FakeHandler>,
        ))
}

fn meta(with_tasks: bool) -> Value {
    let extensions = if with_tasks {
        json!({"io.modelcontextprotocol/tasks": {}})
    } else {
        json!({})
    };
    json!({
        "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientCapabilities": {"extensions": extensions},
    })
}

fn request(method: &str, name: Option<&str>, params: Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", PROTOCOL_VERSION)
        .header("mcp-method", method);
    if let Some(name) = name {
        builder = builder.header("mcp-name", name);
    }
    builder
        .body(Body::from(
            serde_json::to_vec(&json!({
                "jsonrpc": "2.0",
                "id": "request-1",
                "method": method,
                "params": params,
            }))
            .unwrap(),
        ))
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

#[tokio::test]
async fn discovery_advertises_only_the_final_task_extension() {
    let authentications = Arc::new(AtomicUsize::new(0));
    let handler = FakeHandler {
        task_id: ProtocolTaskId::new(),
        authentications: authentications.clone(),
    };
    let response = app(handler)
        .oneshot(request(
            "server/discover",
            None,
            json!({"_meta": meta(false)}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["result"]["supportedVersions"],
        json!([PROTOCOL_VERSION])
    );
    assert_eq!(
        body["result"]["capabilities"]["extensions"]["io.modelcontextprotocol/tasks"],
        json!({})
    );
    assert_eq!(authentications.load(Ordering::SeqCst), 1);

    let handler = FakeHandler {
        task_id: ProtocolTaskId::new(),
        authentications: Arc::new(AtomicUsize::new(0)),
    };
    let response = app(handler)
        .oneshot(request(
            "server/discover",
            None,
            json!({
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28"
                }
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["error"]["code"], -32_602);
}

#[tokio::test]
async fn task_creation_is_per_request_capability_gated() {
    let authentications = Arc::new(AtomicUsize::new(0));
    let handler = FakeHandler {
        task_id: ProtocolTaskId::new(),
        authentications: authentications.clone(),
    };
    let response = app(handler)
        .oneshot(request(
            "tools/call",
            Some("forecast"),
            json!({"name": "forecast", "arguments": {}}),
        ))
        .await
        .unwrap();
    assert_eq!(body_json(response).await["forwarded"], true);
    assert_eq!(authentications.load(Ordering::SeqCst), 0);

    let handler = FakeHandler {
        task_id: ProtocolTaskId::new(),
        authentications: Arc::new(AtomicUsize::new(0)),
    };
    let response = app(handler.clone())
        .oneshot(request(
            "tools/call",
            Some("forecast"),
            json!({"_meta": meta(false), "name": "forecast", "arguments": {}}),
        ))
        .await
        .unwrap();
    assert_eq!(body_json(response).await["forwarded"], true);

    let response = app(handler)
        .oneshot(request(
            "tools/call",
            Some("forecast"),
            json!({"_meta": meta(true), "name": "forecast", "arguments": {}}),
        ))
        .await
        .unwrap();
    let body = body_json(response).await;
    assert_eq!(body["result"]["resultType"], "task");
    assert_eq!(body["result"]["status"], "working");

    let handler = FakeHandler {
        task_id: ProtocolTaskId::new(),
        authentications: Arc::new(AtomicUsize::new(0)),
    };
    let response = app(handler)
        .oneshot(request(
            "tools/call",
            Some("forecast"),
            json!({
                "_meta": meta(false),
                "name": "forecast",
                "arguments": {},
                "task": {"ttl": 60_000}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["error"]["code"], -32_602);
}

#[tokio::test]
async fn lifecycle_methods_require_capability_and_exact_routing_headers() {
    let task_id = ProtocolTaskId::new();
    let handler = FakeHandler {
        task_id,
        authentications: Arc::new(AtomicUsize::new(0)),
    };
    let response = app(handler.clone())
        .oneshot(request(
            "tasks/get",
            Some(&task_id.to_string()),
            json!({"_meta": meta(false), "taskId": task_id}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["error"]["code"], -32_003);

    let response = app(handler)
        .oneshot(request(
            "tasks/get",
            Some("wrong-task"),
            json!({"_meta": meta(true), "taskId": task_id}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["error"]["code"], -32_602);
}

#[tokio::test]
async fn update_and_cancel_have_final_shapes() {
    let task_id = ProtocolTaskId::new();
    let handler = FakeHandler {
        task_id,
        authentications: Arc::new(AtomicUsize::new(0)),
    };
    for method in ["tasks/update", "tasks/cancel"] {
        let params = if method == "tasks/update" {
            json!({"_meta": meta(true), "taskId": task_id, "inputResponses": {}})
        } else {
            json!({"_meta": meta(true), "taskId": task_id})
        };
        let response = app(handler.clone())
            .oneshot(request(method, Some(&task_id.to_string()), params))
            .await
            .unwrap();
        assert_eq!(
            body_json(response).await["result"]["resultType"],
            "complete"
        );
    }
}

#[tokio::test]
async fn subscription_stream_acknowledges_then_emits_full_task_notification() {
    let task_id = ProtocolTaskId::new();
    let handler = FakeHandler {
        task_id,
        authentications: Arc::new(AtomicUsize::new(0)),
    };
    let response = app(handler)
        .oneshot(request(
            "subscriptions/listen",
            None,
            json!({
                "_meta": meta(true),
                "notifications": {"taskIds": [task_id]},
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let body = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("notifications/subscriptions/acknowledged"));
    assert!(body.contains("notifications/tasks"));
    assert!(body.contains("io.modelcontextprotocol/subscriptionId"));
}
