//! Native MCP over real HTTP and the durable Task runtime over isolated SurrealDB.
//! The hosted domain is explicit test data; this is not installed acceptance.
use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use futures::StreamExt;
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use tower::ServiceExt;
use veoveo_mcp_gateway::GatewayCatalog;
use veoveo_task_runtime::TaskTransition;

fn alice() -> AuthenticatedSubject {
    let mut subject = super::super::tests::subject("Alice");
    // The injected identity fixture bypasses only the outer JWT boundary.
    subject.access_token.session_family = None;
    subject
}
fn new_state(store: PlatformStore, port: u16) -> OperationState {
    let catalog = GatewayCatalog::from_control_plane(
        serde_json::from_str(include_str!(
            "../../../../../../../configs/gateway.smoke.json"
        ))
        .unwrap(),
    )
    .unwrap();
    OperationState::new(
        store.clone(),
        GatewayState::new(store),
        GatewayCatalogHandle::new(Arc::new(catalog)),
        CancellationToken::new(),
        port,
        "https://workspace.test",
    )
    .unwrap()
}
fn new_app(state: OperationState) -> Router {
    router(state).layer(Extension(alice()))
}
async fn request(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(format!("/workspace-api/operator{path}"))
                .header("authorization", "Bearer explicit-workspace-fixture")
                .header("content-type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn detail(app: &Router, id: Uuid) -> wire::OperationView {
    loop {
        let (status, value) = request(app, "GET", &format!("/operations/{id}"), Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{value}");
        let view: wire::OperationView = serde_json::from_value(value).unwrap();
        if view.operation.phase != wire::OperationPhase::Dispatching {
            return view;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn native_tasks_survive_restart_require_current_input_and_confirm_cancellation() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = crate::test_store::TestDb::new().await;
        super::super::tests::setup(&db.a).await;
        let subject = alice();
        let fixture = super::test_domain::Fixture::start(db.a.clone(), &subject).await;
        let domain = &fixture.domain;
        let port = fixture.port;
        let state = new_state(db.a.clone(), port);
        let authority = state.authority(&subject, &GatewayProfileId::new("operator").unwrap()).await.unwrap();
        let chat = WorkspaceChatId::new();
        db.a.create_workspace_chat(&authority, chat, "Native Tasks").await.unwrap();
        let app = new_app(state.clone());
        let id = Uuid::now_v7();
        let path = format!("/chats/{}/operations", chat.as_uuid());
        let start = json!({"id":id,"tool":"fixture_task","arguments":{}});
        assert_eq!(request(&app, "POST", &path, start.clone()).await.0, StatusCode::OK);
        assert_eq!(request(&app, "POST", &path, start).await.0, StatusCode::OK);
        let accepted = detail(&app, id).await;
        assert_eq!(domain.calls.load(Ordering::SeqCst), 1);
        let task = accepted.task.unwrap();
        assert_eq!(task.state, wire::TaskState::InputRequired);
        assert_eq!(accepted.inputs.len(), 1);
        assert_eq!(accepted.inputs[0].kind, wire::InputKind::Form);
        let watch = app.clone().oneshot(Request::builder().uri(format!("/workspace-api/operator/operations/events?ids={id}"))
            .header("authorization", "Bearer explicit-workspace-fixture").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(watch.status(), StatusCode::OK);
        let mut wakes = watch.into_body().into_data_stream();
        let first_wake = tokio::time::timeout(Duration::from_secs(3), wakes.next()).await.unwrap().unwrap().unwrap();
        assert!(String::from_utf8_lossy(&first_wake).contains("event: change"));
        let input = &accepted.inputs[0];
        let answer = |count| json!({"revision":accepted.operation.revision,"answers":[{"id":input.id,"digest":input.digest,"decision":"accept","content":{"count":count}}]});
        let input_path = format!("/operations/{id}/input");
        assert_eq!(request(&app, "POST", &input_path, answer(9)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(request(&app, "POST", &input_path, answer(2)).await.0, StatusCode::NO_CONTENT);
        assert_eq!(request(&app, "POST", &input_path, answer(2)).await.0, StatusCode::CONFLICT);
        domain.runtime.transition(&task.id, TaskTransition::Succeeded { message: "fixture domain rejection".into(),
            result: serde_json::to_value(CallToolResult::error(vec![ContentBlock::text("Fixture rejected the requested action.")])).unwrap() }).await.unwrap();
        let changed = tokio::time::timeout(Duration::from_secs(3), async {
            loop { let bytes = wakes.next().await.unwrap().unwrap(); if String::from_utf8_lossy(&bytes).contains("event: change") { break; } }
        }).await;
        assert!(changed.is_ok(), "native Task subscription wakes the client");
        drop(wakes);
        let restored = detail(&new_app(new_state(db.b.clone(), port)), id).await;
        assert_eq!(restored.task.unwrap().id, task.id);
        assert!(restored.result.unwrap().is_error);
        assert_eq!(domain.calls.load(Ordering::SeqCst), 1, "reload never replays tools/call");

        let second = Uuid::now_v7();
        assert_eq!(request(&app, "POST", &path, json!({"id":second,"tool":"fixture_task","arguments":{}})).await.0, StatusCode::OK);
        let waiting = detail(&app, second).await.task.unwrap();
        assert_eq!(request(&app, "POST", &format!("/operations/{second}/cancel"), Value::Null).await.0, StatusCode::NO_CONTENT);
        assert_eq!(detail(&app, second).await.task.unwrap().state, wire::TaskState::Working, "acknowledgement is not cancellation");
        domain.runtime.transition(&waiting.id, TaskTransition::Cancelled).await.unwrap();
        assert_eq!(detail(&app, second).await.task.unwrap().state, wire::TaskState::Cancelled);
        assert_eq!(domain.calls.load(Ordering::SeqCst), 2);
        let continuation = Uuid::now_v7();
        assert_eq!(request(&app, "POST", &path, json!({"id":continuation,"tool":"fixture_task","arguments":{"mode":"mrtr"}})).await.0, StatusCode::OK);
        let pending = detail(&app, continuation).await;
        assert_eq!(pending.operation.phase, wire::OperationPhase::InputRequired);
        assert!(!serde_json::to_string(&pending).unwrap().contains("protected-fixture-continuation"));
        let input = &pending.inputs[0];
        let answer = json!({"revision":pending.operation.revision,"answers":[{"id":input.id,"digest":input.digest,"decision":"accept","content":{"approved":true}}]});
        let continuation_path = format!("/operations/{continuation}/input");
        assert_eq!(request(&app, "POST", &continuation_path, answer.clone()).await.0, StatusCode::NO_CONTENT);
        assert_eq!(detail(&app, continuation).await.result.unwrap().text, ["Confirmed continuation"]);
        assert_eq!(request(&app, "POST", &continuation_path, answer).await.0, StatusCode::CONFLICT);
        assert_eq!(domain.calls.load(Ordering::SeqCst), 4, "two task admissions and one explicit two-round operation");
    }).await.expect("bounded native Workspace Tasks acceptance");
}

#[path = "app_tests.rs"]
mod apps;
