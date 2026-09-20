use super::tests::*;
use super::*;
use serde_json::{Value, json};
use veoveo_mcp_gateway::managed_agents::ManagedTemplateCatalog;

#[tokio::test]
async fn managed_publication_requires_exact_template_parameters_and_credential_admission() {
    let db = crate::test_store::TestDb::new().await;
    crate::workspace::tests::setup(&db.a).await;
    let mut state = managed_state(&db.a);
    let _stop = state.stop.clone().drop_guard();
    let alice = app(&state, fixture_subject("Alice"));
    let (status, choices) = request(&alice, "GET", "agent-templates", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(choices[0]["parameters"][0]["label"], "Session");
    assert!(!choices.to_string().contains("agent-store"));
    let (_, authoring) = request(&alice, "GET", "agent-authoring", Value::Null).await;
    let mut content = json!({"model":authoring["models"][0]["reference"],"instructions":"Literal ${PRIVATE_KEY}","tools":[],"budgets":authoring["models"][0]["limits"],
        "execution":{"kind":"managed","template":"bounded","templateRevision":choices[0]["revision"],"parameters":{"session":"${PRIVATE_KEY}"},"resourceSubscriptions":[]}});
    let (status, draft) = request(&alice, "POST", "agent-definitions", json!({"requestId":uuid::Uuid::now_v7(),"id":"worker","name":"Worker","description":"Test","source":{"kind":"blank","content":content}})).await;
    assert_eq!(status, StatusCode::OK, "{draft}");
    let (_, invalid) = request(
        &alice,
        "POST",
        "agent-definitions/worker/validate",
        json!({"expectedRevision":draft["revision"],"audience":["shared"]}),
    )
    .await;
    assert_eq!(invalid["findings"][0]["field"], "execution.parameters");
    content["execution"]["parameters"]["session"] = json!("session-one");
    let (status, saved) = request(&alice, "PUT", "agent-definitions/worker/draft", json!({"requestId":uuid::Uuid::now_v7(),"expectedRevision":draft["revision"],"content":content})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, published) = request(&alice, "POST", "agent-definitions/worker/publish", json!({"requestId":uuid::Uuid::now_v7(),"expectedRevision":saved["revision"],"digest":saved["draftDigest"],"audience":["shared"]})).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let (_, revision) = request(&alice, "GET", "agent-definitions/worker/draft", Value::Null).await;
    assert_eq!(
        revision["content"]["instructions"],
        "Literal ${PRIVATE_KEY}"
    );
    state.gateway = state
        .gateway
        .clone()
        .with_managed_templates(Default::default());
    let no_template = app(&state, fixture_subject("Alice"));
    let (_, denied) = request(
        &no_template,
        "POST",
        "agent-definitions/worker/validate",
        json!({"expectedRevision":published["revision"],"audience":["shared"]}),
    )
    .await;
    assert_eq!(denied["findings"][0]["code"], "template_unavailable");
}

pub(super) fn managed_state(store: &PlatformStore) -> AgentManagementState {
    let mut state = state(store);
    let template = json!({
        "id":"bounded", "name":"Bounded worker", "tenant":"test", "work_contexts":["shared"],
        "required_deployer_scopes":["operator:use"], "profile":"operator", "scopes":["operator:use"], "roles":[], "membership":"contributor",
        "models":["approved"], "tools":[], "resource_subscriptions":[],
        "parameters":{"session":{"label":"Session", "shape":{"kind":"identifier","maxLength":40}, "environment_variable":"VEOVEO_PARAM_SESSION"}},
        "workload":{"namespace":"agents", "config_map":"bounded-template", "config_digest":format!("sha256:{}", "b".repeat(64)), "image":format!("registry.test/kernel@sha256:{}", "a".repeat(64)),
            "database_secret":"agent-store", "storage_class":"local-path", "storage_gib":2, "cpu_millis":500, "memory_mib":1024,
            "model_secrets":[{"reference":"media_provider_api_key", "secret":"agent-model", "key":"api-key"}]}
    });
    state.gateway = state.gateway.clone().with_managed_templates(
        ManagedTemplateCatalog::from_json(&json!([template]).to_string(), &state.catalog.current())
            .unwrap(),
    );
    let mut model = fixture_model();
    model.api_key = "media_provider_api_key".parse().unwrap();
    state.models = Arc::new(vec![model]);
    state
}
