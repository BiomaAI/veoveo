//! Actual Rig -> native MCP -> durable Task integration using explicit local providers.
use super::*;
use axum::response::{Sse, sse::Event};
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Notify;
use veoveo_platform_store::workspace::WorkspaceOperationPhase;

#[derive(Clone, Default)]
struct Provider {
    calls: Arc<AtomicUsize>,
    release_cancelled: Arc<Notify>,
}

async fn completion(
    State(provider): State<Provider>,
    Json(body): Json<Value>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let model = body["model"].as_str().unwrap().to_owned();
    let call = provider.calls.fetch_add(1, Ordering::SeqCst);
    assert_eq!(body["tools"].as_array().unwrap().len(), 1);
    assert_eq!(body["tools"][0]["function"]["name"], "fixture_task");
    assert!(
        !body.to_string().contains("Choose a count for the fixture"),
        "private input prompts never enter shared model context"
    );
    assert!(!body.to_string().contains("protected-fixture-continuation"));
    if call > 0 && model == "duplicate" {
        assert!(
            body["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|message| message["role"] == "tool")
        );
    }
    let stream = async_stream::stream! {
        if model == "cancel" {
            yield Ok(chunk(&model, json!({"role":"assistant","content":"Preparing a capability request"}), Value::Null));
            provider.release_cancelled.notified().await;
        }
        if call < 2 || model == "cancel" {
            yield Ok(chunk(&model, json!({"role":"assistant","tool_calls":[{"index":0,"id":format!("call-{call}"),"type":"function","function":{"name":"fixture_task","arguments":"{}"}}]}), Value::Null));
            yield Ok(chunk(&model, json!({}), json!("tool_calls")));
        } else {
            yield Ok(chunk(&model, json!({"role":"assistant","content":"Follow this operation in your private Activity."}), Value::Null));
            yield Ok(chunk(&model, json!({}), json!("stop")));
        }
        yield Ok(Event::default().data("[DONE]"));
    };
    Sse::new(stream)
}

fn chunk(model: &str, delta: Value, finish: Value) -> Event {
    Event::default().data(
        json!({"id":"fixture", "object":"chat.completion.chunk","created":0,"model":model,
        "choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
        .to_string(),
    )
}

#[tokio::test]
async fn model_tools_reuse_one_private_task_and_cancelled_runs_cannot_dispatch() {
    tokio::time::timeout(Duration::from_secs(35), async {
        let db = crate::test_store::TestDb::new().await;
        super::super::tests::setup(&db.a).await;
        let mut subject = super::super::tests::subject("Alice");
        subject.access_token.session_family = None;
        let domain =
            super::super::operations::test_domain::Fixture::start(db.a.clone(), &subject).await;
        let provider = Provider::default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(
            axum::serve(
                listener,
                Router::new()
                    .route("/chat/completions", post(completion))
                    .with_state(provider.clone()),
            )
            .into_future(),
        );
        struct Abort(tokio::task::AbortHandle);
        impl Drop for Abort {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _server = Abort(server.abort_handle());
        let catalog = GatewayCatalogHandle::new(Arc::new(tests::catalog()));
        let gateway = GatewayState::new(db.b.clone());
        let stop = CancellationToken::new();
        let _stop = stop.clone().drop_guard();
        let operations = OperationState::new(
            db.a.clone(),
            gateway.clone(),
            catalog.clone(),
            stop.clone(),
            domain.port,
            "https://workspace.test",
        )
        .unwrap();
        let definitions = ["duplicate", "cancel"].map(|id| {
            let mut definition = tests::definition(id, &origin);
            definition.tools = vec![
                veoveo_mcp_contract::GatewayToolName::new("fixture_task").unwrap(),
                veoveo_mcp_contract::GatewayToolName::new("unavailable_tool").unwrap(),
            ];
            definition
        });
        let state = RunState {
            workspace: WorkspaceState {
                store: db.a.clone(),
            },
            gateway,
            catalog,
            definitions: Arc::new(definitions.to_vec()),
            limits: Arc::new(Semaphore::new(16)),
            stop,
            http: reqwest::Client::new(),
            keys: keys::ModelKeys {
                fixture: Some("fixture-key".into()),
            },
            operations,
        };
        let actor = authority::admit(&state.workspace, &subject).await.unwrap();
        let chat = WorkspaceChatId::new();
        db.a.create_workspace_chat(&actor, chat, "Model Tasks")
            .await
            .unwrap();
        let trigger = WorkspaceMessageId::new();
        db.a.send_workspace_message(&actor, chat, trigger, "Use the fixture capability", None)
            .await
            .unwrap();
        let app = routes(state).layer(Extension(subject));
        for name in ["duplicate", "cancel"] {
            let (status, agent) = tests::request(
                &app,
                &format!("/chats/{chat}/agents"),
                json!({"definition":name}),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            let start = json!({"agent":agent["id"],"trigger":trigger.as_uuid()});
            let (status, run) =
                tests::request(&app, &format!("/chats/{chat}/runs"), start.clone()).await;
            assert_eq!(status, StatusCode::OK);
            let id = Uuid::parse_str(run["id"].as_str().unwrap()).unwrap();
            loop {
                let runs = db.b.workspace_runs(&actor, chat).await.unwrap();
                let current = runs
                    .iter()
                    .find(|r| r.id == WorkspaceRunId::from_uuid(id).record_id())
                    .unwrap();
                assert!(
                    !matches!(
                        current.state,
                        WorkspaceRunState::Failed | WorkspaceRunState::Interrupted
                    ),
                    "{current:?}"
                );
                if name == "duplicate" && current.state == WorkspaceRunState::Completed {
                    break;
                }
                if name == "cancel" && !current.text.is_empty() {
                    assert_eq!(
                        tests::request(
                            &app,
                            &format!("/chats/{chat}/runs/{id}/cancel"),
                            Value::Null
                        )
                        .await
                        .0,
                        StatusCode::OK
                    );
                    provider.release_cancelled.notify_one();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert_eq!(
                tests::request(&app, &format!("/chats/{chat}/runs"), start)
                    .await
                    .1["id"],
                run["id"]
            );
        }
        // Let the released model stream attempt its post-cancellation tool call.
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
        assert_eq!(
            domain.domain.calls.load(Ordering::SeqCst),
            1,
            "repeated model call and cancelled run create no second Task"
        );
        let operations =
            db.b.workspace_operations(&actor, Some(chat), None)
                .await
                .unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].phase, WorkspaceOperationPhase::Task);
        assert!(operations[0].run.is_some());
        assert!(operations[0].task_id.is_some());
        let completed = db.b.workspace_runs(&actor, chat).await.unwrap();
        assert!(
            completed
                .iter()
                .any(|run| run.state == WorkspaceRunState::Completed
                    && run.text.contains("Activity"))
        );
        assert!(
            completed
                .iter()
                .any(|run| run.state == WorkspaceRunState::Cancelled)
        );
    })
    .await
    .expect("bounded model to native MCP Task acceptance");
}
