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
async fn model_capability_admission_checks_only_required_discovery_surfaces() {
    let db = crate::test_store::TestDb::new().await;
    super::super::tests::setup(&db.a).await;
    let subject = alice();
    let fixture = super::test_domain::Fixture::start(db.a.clone(), &subject).await;
    let state = new_state(db.a.clone(), fixture.port);
    let caller = Caller {
        profile: GatewayProfileId::new("operator").unwrap(),
        subject,
        bearer: "explicit-workspace-fixture".to_owned().into(),
    };
    let required = [GatewayToolName::new("fixture__task").unwrap()];
    *fixture.domain.degraded_server.lock().unwrap() =
        Some(veoveo_mcp_contract::ServerSlug::new("fixture").unwrap());
    assert_eq!(
        state.capabilities(&caller, &required).await.unwrap_err(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    *fixture.domain.degraded_server.lock().unwrap() =
        Some(veoveo_mcp_contract::ServerSlug::new("unrelated").unwrap());
    assert_eq!(
        state.capabilities(&caller, &required).await.unwrap().len(),
        2
    );
    *fixture.domain.degraded_server.lock().unwrap() = None;
    assert_eq!(
        state.capabilities(&caller, &required).await.unwrap().len(),
        2
    );
    assert_eq!(fixture.domain.calls.load(Ordering::SeqCst), 0);
    state.stop.cancel();
}

#[tokio::test]
async fn required_capabilities_recover_on_native_catalog_notifications_without_dispatch() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let db = crate::test_store::TestDb::new().await;
        super::super::tests::setup(&db.a).await;
        let subject = alice();
        let fixture = super::test_domain::Fixture::start(db.a.clone(), &subject).await;
        fixture
            .domain
            .reactive_catalog
            .store(true, Ordering::SeqCst);
        *fixture.domain.degraded_server.lock().unwrap() =
            Some(veoveo_mcp_contract::ServerSlug::new("fixture").unwrap());
        let state = new_state(db.a.clone(), fixture.port);
        let caller = Caller {
            profile: GatewayProfileId::new("operator").unwrap(),
            subject,
            bearer: "explicit-workspace-fixture".to_owned().into(),
        };
        let pending = tokio::spawn({
            let state = state.clone();
            async move {
                state
                    .capabilities(&caller, &[GatewayToolName::new("fixture__task").unwrap()])
                    .await
            }
        });
        fixture.domain.catalog_requested.notified().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !pending.is_finished(),
            "partial discovery must await its notification"
        );
        assert_eq!(fixture.domain.catalog_reads.load(Ordering::SeqCst), 1);
        *fixture.domain.degraded_server.lock().unwrap() = None;
        fixture.domain.catalog_epoch.send_replace(1);
        assert_eq!(pending.await.unwrap().unwrap().len(), 2);
        assert_eq!(fixture.domain.catalog_reads.load(Ordering::SeqCst), 2);
        assert_eq!(fixture.domain.calls.load(Ordering::SeqCst), 0);
        state.stop.cancel();
    })
    .await
    .unwrap();
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
        let start = json!({"id":id,"tool":"fixture__task","arguments":{}});
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
        let image = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
        domain.runtime.transition(&task.id, TaskTransition::Succeeded { message: "fixture domain rejection".into(),
            result: serde_json::to_value(CallToolResult::error(vec![ContentBlock::text("Fixture rejected the requested action."),
                serde_json::from_value(json!({"type":"image","mimeType":"image/png","data":image})).unwrap()])).unwrap() }).await.unwrap();
        let changed = tokio::time::timeout(Duration::from_secs(3), async {
            loop { let bytes = wakes.next().await.unwrap().unwrap(); if String::from_utf8_lossy(&bytes).contains("event: change") { break; } }
        }).await;
        assert!(changed.is_ok(), "native Task subscription wakes the client");
        drop(wakes);
        let restored = detail(&new_app(new_state(db.b.clone(), port)), id).await;
        assert_eq!(restored.task.unwrap().id, task.id);
        let result = restored.result.unwrap();
        assert!(result.is_error);
        assert_eq!(result.images[0].data, image, "native image survives a new gateway state without redispatch");
        assert_eq!(result.images[0].mime_type, wire::ResultImageMime::Png);
        assert_eq!(result.omitted_images, 0);
        let mut other = super::super::tests::subject("Bob");
        other.access_token.session_family = None;
        let other_app = router(new_state(db.b.clone(), port)).layer(Extension(other));
        let (denied, body) = request(&other_app, "GET", &format!("/operations/{id}"), Value::Null).await;
        assert!(matches!(denied, StatusCode::NOT_FOUND | StatusCode::FORBIDDEN));
        assert!(!body.to_string().contains(image), "a second person cannot read the image receipt");
        assert_eq!(domain.calls.load(Ordering::SeqCst), 1, "reload never replays tools/call");

        let second = Uuid::now_v7();
        assert_eq!(request(&app, "POST", &path, json!({"id":second,"tool":"fixture__task","arguments":{}})).await.0, StatusCode::OK);
        let waiting = detail(&app, second).await.task.unwrap();
        assert_eq!(request(&app, "POST", &format!("/operations/{second}/cancel"), Value::Null).await.0, StatusCode::NO_CONTENT);
        assert_eq!(detail(&app, second).await.task.unwrap().state, wire::TaskState::Working, "acknowledgement is not cancellation");
        domain.runtime.transition(&waiting.id, TaskTransition::Cancelled).await.unwrap();
        assert_eq!(detail(&app, second).await.task.unwrap().state, wire::TaskState::Cancelled);
        assert_eq!(domain.calls.load(Ordering::SeqCst), 2);
        let continuation = Uuid::now_v7();
        assert_eq!(request(&app, "POST", &path, json!({"id":continuation,"tool":"fixture__task","arguments":{"mode":"mrtr"}})).await.0, StatusCode::OK);
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

#[path = "personal_tests.rs"]
mod personal;

#[tokio::test]
async fn request_progress_arrives_before_tool_receipt_and_ends_with_it() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = crate::test_store::TestDb::new().await;
        super::super::tests::setup(&db.a).await;
        let subject = alice();
        let fixture = super::test_domain::Fixture::start(db.a.clone(), &subject).await;
        fixture.domain.hold_dispatch.store(true, Ordering::SeqCst);
        let state = new_state(db.a.clone(), fixture.port);
        let authority = state
            .authority(&subject, &GatewayProfileId::new("operator").unwrap())
            .await
            .unwrap();
        let chat = WorkspaceChatId::new();
        db.a.create_workspace_chat(&authority, chat, "Measured work")
            .await
            .unwrap();
        let app = new_app(state);
        let id = Uuid::now_v7();
        assert_eq!(
            request(
                &app,
                "POST",
                &format!("/chats/{}/operations", chat.as_uuid()),
                json!({"id":id,"tool":"fixture__task","arguments":{}})
            )
            .await
            .0,
            StatusCode::OK
        );
        loop {
            let (status, value) =
                request(&app, "GET", &format!("/operations/{id}"), Value::Null).await;
            assert_eq!(status, StatusCode::OK);
            let view: wire::OperationView = serde_json::from_value(value).unwrap();
            if let Some(progress) = view.progress {
                assert_eq!(view.operation.phase, wire::OperationPhase::Dispatching);
                assert_eq!(progress.completed, 4.0);
                assert_eq!(progress.total, Some(10.0));
                assert_eq!(progress.message.as_deref(), Some("Measured fixture work"));
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        fixture.domain.release_dispatch.notify_one();
        let view = detail(&app, id).await;
        assert!(view.progress.is_none());
        assert!(view.task.is_some());
        assert_eq!(fixture.domain.calls.load(Ordering::SeqCst), 1);
    })
    .await
    .unwrap();
}
