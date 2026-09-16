//! Explicit model HTTP fixtures exercise the actual Rig stream and durable run
//! pipeline. These are not production model or MCP Tasks acceptance.
use super::*;
use axum::{
    Extension,
    body::{Body, to_bytes},
    http::Request,
    response::{Sse, sse::Event},
};
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Notify;
use tower::ServiceExt;
use veoveo_mcp_contract::GatewayControlPlane;
use veoveo_mcp_gateway::GatewayCatalog;

#[derive(Clone)]
struct Provider {
    requests: Arc<AtomicUsize>,
    release: Arc<Notify>,
}
async fn completion(
    State(provider): State<Provider>,
    Json(body): Json<Value>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    provider.requests.fetch_add(1, Ordering::SeqCst);
    let model = body["model"].as_str().unwrap().to_owned();
    assert!(!body.to_string().contains("PRIVATE OTHER CHAT"));
    let prompt: Value = serde_json::from_str(
        body["messages"].as_array().unwrap().last().unwrap()["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(prompt["request"]["reply_to"]["kind"], "message");
    assert_eq!(
        prompt["request"]["reply_context"]["text"],
        "The shared topic being discussed"
    );
    assert_eq!(
        prompt["request"]["attachment_references"][0]["name"],
        "Human-published reference"
    );
    assert_eq!(
        prompt["request"]["attachment_references"][0]["kind"],
        "artifact"
    );
    let stream = async_stream::stream! {
        yield Ok(Event::default().data(json!({"id":"fixture", "object":"chat.completion.chunk", "created":0, "model":model,
            "choices":[{"index":0,"delta":{"role":"assistant","content":format!("{model} response")},"finish_reason":null}]}).to_string()));
        provider.release.notified().await;
        yield Ok(Event::default().data(json!({"id":"fixture", "object":"chat.completion.chunk", "created":0, "model":model,
            "choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}).to_string()));
        yield Ok(Event::default().data("[DONE]"));
    };
    Sse::new(stream)
}
pub(super) async fn request(app: &Router, path: &str, value: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workspace-api/operator{path}"))
                .header("content-type", "application/json")
                .header("authorization", "Bearer explicit-workspace-fixture")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 65536).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}
pub(super) fn catalog() -> GatewayCatalog {
    let plane: GatewayControlPlane = serde_json::from_str(include_str!(
        "../../../../../../../configs/gateway.smoke.json"
    ))
    .unwrap();
    GatewayCatalog::from_control_plane(plane).unwrap()
}
pub(super) fn definition(id: &str, base_url: &str) -> config::Definition {
    serde_json::from_value(
        json!({"id":id,"name":id,"description":"Explicit fixture","provider":"Fixture",
        "tenant":"test","work_contexts":["shared"],"instructions":"Respond to the current request.","tools":[],
        "model":{"base_url":base_url,"name":id,"api_key":"fixture-key","max_output_tokens":128}}),
    )
    .unwrap()
}

#[tokio::test]
async fn http_model_runs_stream_independently_and_replay_does_not_dispatch_again() {
    tokio::time::timeout(Duration::from_secs(25), async {
        let db = crate::test_store::TestDb::new().await;
        super::super::tests::setup(&db.a).await;
        let provider = Provider {
            requests: Arc::new(AtomicUsize::new(0)),
            release: Arc::new(Notify::new()),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let provider_task = tokio::spawn(
            axum::serve(
                listener,
                Router::new()
                    .route("/chat/completions", post(completion))
                    .with_state(provider.clone()),
            )
            .into_future(),
        );
        let provider_guard = provider_task.abort_handle();
        struct Abort(tokio::task::AbortHandle);
        impl Drop for Abort {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _guard = Abort(provider_guard);
        let mut subject = super::super::tests::subject("Alice");
        subject.access_token.session_family = None; // signature/session admission is a separate explicit fixture boundary
        let stop = CancellationToken::new();
        let _stop_guard = stop.clone().drop_guard();
        let state = RunState {
            operations: OperationState::new(
                db.a.clone(),
                GatewayState::new(db.b.clone()),
                GatewayCatalogHandle::new(Arc::new(catalog())),
                stop.clone(),
                1,
                "https://workspace.test",
            )
            .unwrap(),
            workspace: WorkspaceState {
                store: db.a.clone(),
            },
            gateway: GatewayState::new(db.b.clone()),
            catalog: GatewayCatalogHandle::new(Arc::new(catalog())),
            definitions: Arc::new(vec![
                definition("writer", &origin),
                definition("reviewer", &origin),
            ]),
            limits: Arc::new(Semaphore::new(16)),
            stop,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            keys: keys::ModelKeys {
                fixture: Some("fixture-key".into()),
            },
        };
        let actor = authority::admit(&state.workspace, &subject).await.unwrap();
        let chat = WorkspaceChatId::new();
        db.a.create_workspace_chat(&actor, chat, "Shared")
            .await
            .unwrap();
        let private = WorkspaceChatId::new();
        db.a.create_workspace_chat(&actor, private, "Other")
            .await
            .unwrap();
        db.a.send_workspace_turn(
            &actor,
            private,
            veoveo_platform_store::workspace::WorkspaceTurnRequest {
                id: WorkspaceMessageId::new(),
                text: ("PRIVATE OTHER CHAT").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
        let trigger = WorkspaceMessageId::new();
        let reply = WorkspaceMessageId::new();
        db.a.send_workspace_turn(&actor, chat, veoveo_platform_store::workspace::WorkspaceTurnRequest {
            attachments: vec![],
            id: reply, text: "The shared topic being discussed".into(), reply_to: None, addressed_agents: vec![],
            deadline: Utc::now() + TimeDelta::seconds(120),
        }).await.unwrap();
        let app = routes(state).layer(Extension(subject));
        let mut agent_ids = Vec::new();
        for name in ["writer", "reviewer"] {
            let (status, agent) = request(&app, &format!("/chats/{chat}/agents"), json!({"definition":name})).await;
            assert_eq!(status, StatusCode::OK, "{agent}");
            agent_ids.push(agent["id"].as_str().unwrap().to_owned());
        }
        let admission = json!({"id":trigger.as_uuid(),"text":"Discuss this", "replyTo":{"kind":"message","id":reply.as_uuid()},"attachments":[{"kind":"artifact","id":veoveo_mcp_contract::ArtifactId::new(),"name":"Human-published reference"}],"addressedAgents":agent_ids});
        let path = format!("/chats/{chat}/messages");
        let (one, two) = tokio::join!(request(&app, &path, admission.clone()), request(&app, &path, admission.clone()));
        assert_eq!(one.0, StatusCode::OK, "{one:?}");
        assert_eq!(one, two);
        assert_eq!(one.1["replyTo"], admission["replyTo"]);
        assert_eq!(one.1["attachments"], admission["attachments"]);
        assert_eq!(one.1["replyContext"]["text"], "The shared topic being discussed");
        // Both admissions and the human message survive the completed HTTP
        // request. Concurrent exact replay claims each model dispatch once.
        let mut run_ids = Vec::new();
        loop {
            let runs = db.b.workspace_runs(&actor, chat).await.unwrap();
            if runs.len() == 2 && runs.iter().all(|r| !r.text.is_empty()) {
                for id in &agent_ids {
                    let run = runs.iter().find(|run| super::super::projection::uuid(&run.agent).unwrap().to_string() == *id).unwrap();
                    run_ids.push(super::super::projection::uuid(&run.id).unwrap().to_string());
                }
                assert_eq!(runs[0].context_sequence, runs[1].context_sequence);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(provider.requests.load(Ordering::SeqCst), 2);
        db.a.send_workspace_turn(
            &actor,
            chat,
            veoveo_platform_store::workspace::WorkspaceTurnRequest {
                id: WorkspaceMessageId::new(),
                text: ("Human continues").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
        let (status, cancelled) = request(
            &app,
            &format!("/chats/{chat}/runs/{}/cancel", run_ids[0]),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(cancelled["state"], "cancelled");
        let (status, repeated) =
            request(&app, &path, admission.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(repeated["id"], trigger.as_uuid().to_string());
        provider.release.notify_waiters();
        loop {
            let runs = db.b.workspace_runs(&actor, chat).await.unwrap();
            if runs.iter().any(|r| r.state == WorkspaceRunState::Completed) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(provider.requests.load(Ordering::SeqCst), 2);
        let restored = db.b.workspace_runs(&actor, chat).await.unwrap();
        assert!(
            restored
                .iter()
                .any(|r| r.state == WorkspaceRunState::Cancelled)
        );
        assert!(
            restored
                .iter()
                .any(|r| r.state == WorkspaceRunState::Completed && r.text == "reviewer response")
        );
    })
    .await
    .expect("bounded model stream fixture");
}

#[test]
fn model_admission_requires_exact_context_registered_provider_secret_and_bounded_output() {
    let catalog = catalog();
    let context = &catalog.control_plane().work_contexts[0];
    let mut definition = definition("helper", "https://model.example/v1");
    definition.tenant = context.tenant.clone();
    definition.work_contexts = vec![context.id.clone()];
    definition.model.api_key =
        veoveo_mcp_contract::SecretReferenceId::new("media_provider_api_key").unwrap();
    config::validate(&[definition.clone()], &catalog).unwrap();
    for destination in [
        "https://credential@model.example/v1",
        "https://model.example/v1?key=secret",
        "file:///tmp/model",
    ] {
        let mut invalid = definition.clone();
        invalid.model.base_url = destination.into();
        assert!(config::validate(&[invalid], &catalog).is_err());
    }
    let mut invalid = definition.clone();
    invalid.model.max_output_tokens = 9000;
    assert!(config::validate(&[invalid], &catalog).is_err());
    let mut invalid = definition.clone();
    invalid.tenant = veoveo_mcp_contract::TenantId::new("foreign").unwrap();
    assert!(config::validate(&[invalid], &catalog).is_err());
    let mut invalid = definition.clone();
    invalid.model.api_key = veoveo_mcp_contract::SecretReferenceId::new("unregistered").unwrap();
    assert!(config::validate(&[invalid], &catalog).is_err());
    assert!(config::validate(&[definition.clone(), definition], &catalog).is_err());
}

/// Human-only deployment with no configured model is a supported router state.
pub(crate) fn empty_routes(store: &PlatformStore) -> Router {
    let gateway = GatewayState::new(store.clone());
    let catalog = GatewayCatalogHandle::new(Arc::new(catalog()));
    let stop = CancellationToken::new();
    routes(RunState {
        operations: OperationState::new(
            store.clone(),
            gateway.clone(),
            catalog.clone(),
            stop.clone(),
            1,
            "https://workspace.test",
        )
        .unwrap(),
        workspace: WorkspaceState {
            store: store.clone(),
        },
        gateway,
        catalog,
        definitions: Arc::new(vec![]),
        limits: Arc::new(Semaphore::new(16)),
        stop,
        http: reqwest::Client::new(),
        keys: keys::ModelKeys::default(),
    })
}
