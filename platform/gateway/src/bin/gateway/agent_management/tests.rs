use super::*;

#[test]
fn public_executable_content_round_trips_without_provider_secrets() {
    let content: wire::Content = serde_json::from_value(serde_json::json!({
        "model":{"id":"approved","revision":format!("sha256:{}", "a".repeat(64))},
        "instructions":"A bounded assistant", "tools":[], "execution":{"kind":"chat"},
        "budgets":{"maxOutputTokens":128,"maxCompletionCalls":4,"maxToolCalls":8,"deadlineSeconds":120}
    })).unwrap();
    let stored = projection::content(content.clone());
    assert!(stored.validate().is_ok());
    let actual = projection::public_content(stored).ok().unwrap();
    assert_eq!(actual, content);
}

use axum::{
    Extension,
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use veoveo_mcp_contract::{GatewayControlPlane, GatewayProfileId, ScopeName};
use veoveo_mcp_gateway::GatewayCatalog;

pub(super) fn fixture_subject(name: &str) -> AuthenticatedSubject {
    let mut subject = crate::workspace::tests::subject(name);
    // Signature and browser family validation have dedicated gateway acceptance.
    subject.access_token.session_family = None;
    subject
        .principal
        .scopes
        .insert(ScopeName::new("operator:use").unwrap());
    subject.access_token.scopes = subject.principal.scopes.clone();
    subject
}

pub(crate) fn fixture_catalog() -> GatewayCatalog {
    let mut value: Value =
        serde_json::from_str(include_str!("../../../../../../configs/gateway.smoke.json")).unwrap();
    value["tenants"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"test","title":"Test","metadata":{}}));
    value["work_contexts"].as_array_mut().unwrap().push(json!({
        "id":"shared", "tenant":"test", "title":"Shared", "policy_revision":"2026-07-02",
        "output_policy":{"owner":{"kind":"group","id":"collaborators"}},
        "memberships":[{"level":"contributor","groups":["collaborators"]}]
    }));
    value["policies"][0]["rules"].as_array_mut().unwrap().push(json!({
        "id":"agent-authoring-fixture", "effect":"allow", "profiles":["operator"],
        "required_scopes":["operator:use"],
        "actions":["agent_definitions_read","agent_definitions_read_content","agent_definitions_create",
            "agent_definitions_edit","agent_definitions_publish","agent_definitions_use",
            "agent_definitions_control","agent_definitions_archive","agent_definitions_transfer",
            "agent_instances_deploy","agent_instances_control"]
    }));
    GatewayCatalog::from_control_plane(
        serde_json::from_value::<GatewayControlPlane>(value).unwrap(),
    )
    .unwrap()
}

pub(super) fn fixture_model() -> models::ModelConnection {
    serde_json::from_value(json!({
        "id":"approved","name":"Approved model","provider":"Fixture", "tenant":"test",
        "work_contexts":["shared"],"base_url":"https://provider.test/v1", "model":"model",
        "api_key":"fixture-secret", "limits":{"maxOutputTokens":128,"maxCompletionCalls":4,"maxToolCalls":8,"deadlineSeconds":120}
    })).unwrap()
}

pub(super) fn state(store: &PlatformStore) -> AgentManagementState {
    let gateway = GatewayState::new(store.clone());
    let catalog = GatewayCatalogHandle::new(Arc::new(fixture_catalog()));
    let stop = CancellationToken::new();
    AgentManagementState {
        operations: OperationState::new(
            store.clone(),
            gateway.clone(),
            catalog.clone(),
            stop.clone(),
            1,
            "https://workspace.test",
        )
        .unwrap(),
        gateway,
        catalog,
        stop,
        models: Arc::new(vec![fixture_model()]),
        definition_limit: 10,
        instance_limits: crate::agent_management::instance_limits()
            .expect("valid managed capacity"),
    }
}

pub(super) fn app(state: &AgentManagementState, subject: AuthenticatedSubject) -> Router {
    router(state.clone()).layer(Extension(subject))
}

pub(super) async fn request(
    app: &Router,
    method: &str,
    path: &str,
    value: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(format!("/admin/operator/{path}"))
                .header("content-type", "application/json")
                .header("authorization", "Bearer explicit-authoring-fixture")
                .body(if value.is_null() {
                    Body::empty()
                } else {
                    Body::from(value.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[test]
fn model_revision_changes_only_when_execution_configuration_changes() {
    let model = fixture_model();
    let original = model.revision();
    let mut changed = model.clone();
    changed.name = "Renamed".into();
    changed.work_contexts.clear();
    assert_eq!(changed.revision(), original);
    assert!(!changed.permits(&fixture_subject("Alice"), &"shared".parse().unwrap()));
    changed.model = "another-model".into();
    assert_ne!(changed.revision(), original);
    let public = serde_json::to_value(model.public()).unwrap().to_string();
    assert!(!public.contains("fixture-secret"));
    assert!(!public.contains("provider.test"));
}

#[tokio::test]
async fn http_authoring_publishes_immutable_revisions_with_private_content_and_replay() {
    let db = crate::test_store::TestDb::new().await;
    crate::workspace::tests::setup(&db.a).await;
    let state = state(&db.a);
    let _stop = state.stop.clone().drop_guard();
    let alice = app(&state, fixture_subject("Alice"));
    let bob = app(&state, fixture_subject("Bob"));
    let (_, authoring) = request(&alice, "GET", "agent-authoring", Value::Null).await;
    assert_eq!(authoring["permissions"]["publish"], true);
    let mut content = json!({"model": authoring["models"][0]["reference"], "instructions":"Private original prompt",
        "tools":[], "execution":{"kind":"chat"}, "budgets":authoring["models"][0]["limits"]});
    let create = json!({"requestId":uuid::Uuid::now_v7(),"id":"researcher","name":"Researcher","description":"Test", "source":{"kind":"blank","content":content}});
    let (status, first) = request(&alice, "POST", "agent-definitions", create.clone()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert!(!first.to_string().contains("Private original"));
    assert_eq!(
        request(&alice, "POST", "agent-definitions", create.clone())
            .await
            .1,
        first
    );
    assert_eq!(
        request(
            &bob,
            "GET",
            "agent-definitions/researcher/draft",
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let publication = json!({"requestId":uuid::Uuid::now_v7(),"expectedRevision":first["revision"],"digest":first["draftDigest"],"audience":["shared"]});
    let (status, published) = request(
        &alice,
        "POST",
        "agent-definitions/researcher/publish",
        publication.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    assert_eq!(
        request(
            &alice,
            "POST",
            "agent-definitions/researcher/publish",
            publication.clone()
        )
        .await
        .1,
        published
    );
    content["instructions"] = json!("Private changed prompt");
    let (status, edited) = request(&alice, "PUT", "agent-definitions/researcher/draft", json!({"requestId":uuid::Uuid::now_v7(),"expectedRevision":published["revision"],"content":content})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        request(
            &alice,
            "POST",
            "agent-definitions/researcher/publish",
            publication.clone()
        )
        .await
        .1,
        published
    );
    let (status, second) = request(&alice, "POST", "agent-definitions/researcher/publish", json!({"requestId":uuid::Uuid::now_v7(),"expectedRevision":edited["revision"],"digest":edited["draftDigest"],"audience":["shared"]})).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    let (status, history) = request(
        &alice,
        "GET",
        "agent-definitions/researcher/revisions?limit=1",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{history}");
    assert_eq!(
        history["items"][0]["content"]["instructions"],
        "Private changed prompt"
    );
    let (_, older) = request(
        &alice,
        "GET",
        &format!(
            "agent-definitions/researcher/revisions?limit=1&after={}",
            history["next"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    assert_eq!(
        older["items"][0]["content"]["instructions"],
        "Private original prompt"
    );
    let mut changed_retry = publication.clone();
    changed_retry["digest"] = edited["draftDigest"].clone();
    assert_eq!(
        request(
            &alice,
            "POST",
            "agent-definitions/researcher/publish",
            changed_retry
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    // Enable revalidates the retained executable connection before changing state.
    let (_, disabled) = request(
        &alice,
        "POST",
        "agent-definitions/researcher/disable",
        json!({
            "requestId":uuid::Uuid::now_v7(),"expectedRevision":second["revision"]
        }),
    )
    .await;
    let mut changed = state.clone();
    let mut model = fixture_model();
    model.model = "replacement-model".into();
    changed.models = Arc::new(vec![model]);
    let (status, findings) = request(
        &app(&changed, fixture_subject("Alice")),
        "POST",
        "agent-definitions/researcher/enable",
        json!({
            "requestId":uuid::Uuid::now_v7(),"expectedRevision":disabled["revision"]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{findings}");
    assert_eq!(findings["findings"][0]["code"], "model_changed");
    let mut expired = fixture_subject("Alice");
    expired.access_token.expires_at = chrono::Utc::now();
    assert_eq!(
        request(&app(&state, expired), "GET", "agent-authoring", Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let mut unscoped = fixture_subject("Alice");
    unscoped.principal.scopes.clear();
    assert_eq!(
        request(&app(&state, unscoped), "POST", "agent-definitions", create)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let actor = authority::admit(
        &state,
        GatewayProfileId::new("operator").unwrap().to_string(),
        fixture_subject("Alice"),
        Action::AgentDefinitionsRead,
    )
    .await
    .unwrap();
    assert!(
        state
            .store()
            .agent_catalog_head(&actor.authority)
            .await
            .unwrap()
            > 0
    );
}

#[tokio::test]
async fn current_context_and_principal_override_authoring_policy() {
    let db = crate::test_store::TestDb::new().await;
    crate::workspace::tests::setup(&db.a).await;
    let state = state(&db.a);
    let _stop = state.stop.clone().drop_guard();
    let subject = fixture_subject("Alice");
    let alice = app(&state, subject.clone());
    let context = veoveo_platform_store::deterministic_work_context_id("test", "shared")
        .unwrap()
        .record_id();
    db.a.client()
        .query("UPDATE ONLY $context SET memberships[0].level = 'viewer';")
        .bind(("context", context.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let (status, authoring) = request(&alice, "GET", "agent-authoring", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(authoring["permissions"]["create"], false);
    let content = json!({"model":authoring["models"][0]["reference"],"instructions":"Test", "tools":[],"execution":{"kind":"chat"},"budgets":authoring["models"][0]["limits"]});
    assert_eq!(request(&alice, "POST", "agent-definitions", json!({"requestId":uuid::Uuid::now_v7(),"id":"forbidden","name":"Test","description":"Test","source":{"kind":"blank","content":content}})).await.0, StatusCode::FORBIDDEN);
    db.a.client()
        .query("UPDATE ONLY $context SET memberships = [];")
        .bind(("context", context))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        request(&alice, "GET", "agent-authoring", Value::Null)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}
