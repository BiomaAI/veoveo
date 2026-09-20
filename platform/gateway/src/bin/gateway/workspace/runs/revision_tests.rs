//! Owner adoption crosses HTTP, current policy and the durable registry.
use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use veoveo_mcp_contract::GatewayAction;
use veoveo_platform_store::agent_management::{AgentDefinitionMutation, AgentPublicationContext};

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/workspace-api/operator{path}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn owner_reviews_and_adopts_an_exact_revision_without_reading_private_instructions() {
    let db = crate::test_store::TestDb::new().await;
    super::super::tests::setup(&db.a).await;
    let mut subject = super::super::tests::subject("Alice");
    subject.access_token.session_family = None;
    let mut plane = tests::catalog().control_plane().clone();
    for policy in &mut plane.policies {
        for rule in &mut policy.rules {
            rule.actions
                .retain(|action| *action != GatewayAction::AgentDefinitionsReadContent);
        }
    }
    let catalog = GatewayCatalogHandle::new(Arc::new(
        veoveo_mcp_gateway::GatewayCatalog::from_control_plane(plane).unwrap(),
    ));
    let stop = CancellationToken::new();
    let _guard = stop.clone().drop_guard();
    let gateway = GatewayState::new(db.a.clone());
    let operations = OperationState::new(
        db.a.clone(),
        gateway.clone(),
        catalog.clone(),
        stop.clone(),
        1,
        "https://workspace.test",
    )
    .unwrap();
    let agents = tests::registry(
        &db.a,
        &catalog,
        &operations,
        &stop,
        &[tests::definition("writer", "https://provider.test")],
    )
    .await;
    let author = agents
        .execution_authority(&"operator".parse().unwrap(), &subject)
        .await
        .unwrap();
    let workspace = WorkspaceState {
        store: db.a.clone(),
    };
    let actor = authority::admit(&workspace, &subject).await.unwrap();
    let chat = WorkspaceChatId::new();
    db.a.create_workspace_chat(&actor, chat, "Revision review")
        .await
        .unwrap();
    let routes = routes(RunState {
        workspace,
        gateway,
        catalog,
        agents,
        operations,
        stop,
        limits: Arc::new(Semaphore::new(16)),
        http: reqwest::Client::new(),
        keys: keys::ModelKeys::default(),
    });
    let app = routes.clone().layer(Extension(subject));
    let (status, participant) = tests::request(
        &app,
        &format!("/chats/{chat}/agents"),
        json!({"definition":"writer"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let first = participant["revision"].clone();
    let path = format!(
        "/chats/{chat}/agents/{}/revision",
        participant["id"].as_str().unwrap()
    );
    let draft = db.a.agent_definition(&author, "writer").await.unwrap();
    let mut content = draft.draft;
    content.instructions = "Private changed instructions".into();
    content.budgets.max_completion_calls = 2;
    let draft =
        db.a.mutate_agent_definition(
            &author,
            "writer",
            Uuid::now_v7(),
            Some(draft.revision),
            AgentDefinitionMutation::Draft { content },
        )
        .await
        .unwrap();
    db.a.mutate_agent_definition(
        &author,
        "writer",
        Uuid::now_v7(),
        Some(draft.revision),
        AgentDefinitionMutation::Publish {
            digest: draft.draft_digest.clone(),
            audience: vec![AgentPublicationContext {
                work_context: author.work_context.clone(),
                context_digest: author.context_digest.clone(),
            }],
        },
    )
    .await
    .unwrap();
    let (status, preview) = get(&app, &path).await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["current"]["revision"], first);
    assert_ne!(preview["target"]["revision"], first);
    assert!(preview["target"]["instructions"].is_null());
    assert_ne!(
        preview["target"]["instructionsDigest"],
        preview["current"]["instructionsDigest"]
    );
    assert_eq!(preview["target"]["publishedByName"], "Alice");
    assert_eq!(preview["target"]["budgets"]["maxCompletionCalls"], 2);
    let mut bob = super::super::tests::subject("Bob");
    bob.access_token.session_family = None;
    assert_eq!(
        get(&routes.layer(Extension(bob)), &path).await.0,
        StatusCode::NOT_FOUND
    );
    let request = json!({"requestId":Uuid::now_v7(), "expectedRevision":first, "revision":preview["target"]["revision"]});
    let result = tests::request(&app, &path, request.clone()).await;
    assert_eq!(result.0, StatusCode::OK, "{result:?}");
    assert_eq!(result.1["id"], participant["id"]);
    assert_eq!(result.1["revision"], preview["target"]["revision"]);
    assert_eq!(tests::request(&app, &path, request.clone()).await, result);
    let mut stale = request;
    stale["requestId"] = json!(Uuid::now_v7());
    assert_eq!(
        tests::request(&app, &path, stale).await.0,
        StatusCode::CONFLICT
    );
    let recover = json!({"requestId":Uuid::now_v7(), "expectedRevision":preview["target"]["revision"], "revision":first});
    assert_eq!(tests::request(&app, &path, recover).await.0, StatusCode::OK);
}
