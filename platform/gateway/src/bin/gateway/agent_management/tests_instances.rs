use super::{tests::*, tests_templates::managed_state, *};
use serde_json::{Value, json};
use veoveo_platform_store::agent_management::instances::ManagedAgentLimits;

async fn published(app: &Router) -> Value {
    let (_, choices) = request(app, "GET", "agent-templates", Value::Null).await;
    let (_, authoring) = request(app, "GET", "agent-authoring", Value::Null).await;
    let content = json!({"model":authoring["models"][0]["reference"],"instructions":"A managed worker", "tools":[],"budgets":authoring["models"][0]["limits"],
        "execution":{"kind":"managed","template":"bounded","templateRevision":choices[0]["revision"],"parameters":{"session":"session-one"},"resourceSubscriptions":[]}});
    let (status, draft) = request(app, "POST", "agent-definitions", json!({"requestId":uuid::Uuid::now_v7(),"id":"worker","name":"Worker","description":"Test","source":{"kind":"blank","content":content}})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, definition) = request(app, "POST", "agent-definitions/worker/publish", json!({"requestId":uuid::Uuid::now_v7(),"expectedRevision":draft["revision"],"digest":draft["draftDigest"],"audience":["shared"]})).await;
    assert_eq!(status, StatusCode::OK, "{definition}");
    definition
}

#[tokio::test]
async fn instance_routes_reserve_capacity_recover_retries_and_keep_owner_control() {
    let db = crate::test_store::TestDb::new().await;
    crate::workspace::tests::setup(&db.a).await;
    let mut state = managed_state(&db.a);
    let _stop = state.stop.clone().drop_guard();
    state.instance_limits = ManagedAgentLimits {
        instances: 1,
        storage_gib: 2,
    };
    let alice = app(&state, fixture_subject("Alice"));
    let bob = app(&state, fixture_subject("Bob"));
    let definition = published(&alice).await;
    let actor = authority::admit(
        &state,
        "operator".into(),
        fixture_subject("Alice"),
        Action::AgentDefinitionsRead,
    )
    .await
    .unwrap();
    let before = db.a.agent_management_head(&actor.authority).await.unwrap();
    let body = json!({"requestId":uuid::Uuid::now_v7(),"id":"worker-one","name":"Worker one","definition":"worker","revision":definition["publishedDigest"]});
    let (status, operation) = request(&alice, "POST", "agent-instances", body.clone()).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{operation}");
    assert_eq!(operation["phase"], "queued");
    let after = db.a.agent_management_head(&actor.authority).await.unwrap();
    assert!(after > before);
    let bob_actor = authority::admit(
        &state,
        "operator".into(),
        fixture_subject("Bob"),
        Action::AgentDefinitionsRead,
    )
    .await
    .unwrap();
    assert_eq!(
        db.a.agent_management_head(&bob_actor.authority)
            .await
            .unwrap(),
        before
    );
    let (_, replay) = request(&alice, "POST", "agent-instances", body.clone()).await;
    assert_eq!(replay, operation);
    let mut changed = body.clone();
    changed["name"] = json!("Another worker");
    assert_eq!(
        request(&alice, "POST", "agent-instances", changed).await.0,
        StatusCode::CONFLICT
    );
    let mut another = body.clone();
    another["requestId"] = json!(uuid::Uuid::now_v7());
    another["id"] = json!("worker-two");
    assert_eq!(
        request(&alice, "POST", "agent-instances", another).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let (_, instance) = request(&alice, "GET", "agent-instances/worker-one", Value::Null).await;
    assert_eq!(instance["desired"], "running");
    assert_eq!(instance["observed"], "queued");
    assert_eq!(instance["activeGeneration"], 0);
    assert!(instance["activeRevision"].is_null());
    for hidden in [
        "database_secret",
        "private_key",
        "registry.test",
        "agent-store",
    ] {
        assert!(!instance.to_string().contains(hidden));
    }
    let (_, page) = request(&alice, "GET", "agent-instances?limit=1", Value::Null).await;
    assert_eq!(page["next"], "worker-one");
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        request(&bob, "GET", "agent-instances/worker-one", Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let path = format!("agent-operations/{}", operation["id"].as_str().unwrap());
    assert_eq!(
        request(&alice, "GET", &path, Value::Null).await.1,
        operation
    );
    assert_eq!(
        request(&bob, "GET", &path, Value::Null).await.0,
        StatusCode::NOT_FOUND
    );
    let pause = json!({"requestId":uuid::Uuid::now_v7(),"expectedGeneration":1,"change":{"kind":"state","desired":"paused"}});
    assert_eq!(
        request(&bob, "PATCH", "agent-instances/worker-one", pause.clone())
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, paused) =
        request(&alice, "PATCH", "agent-instances/worker-one", pause.clone()).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(paused["generation"], 2);
    assert_eq!(
        request(&alice, "PATCH", "agent-instances/worker-one", pause.clone())
            .await
            .1,
        paused
    );
    let mut stale = pause.clone();
    stale["requestId"] = json!(uuid::Uuid::now_v7());
    assert_eq!(
        request(&alice, "PATCH", "agent-instances/worker-one", stale)
            .await
            .0,
        StatusCode::CONFLICT
    );
    state.gateway = state
        .gateway
        .clone()
        .with_managed_templates(Default::default());
    let removed = app(&state, fixture_subject("Alice"));
    assert_eq!(
        request(&removed, "POST", "agent-instances", body).await.1["id"],
        operation["id"]
    );
    assert_eq!(
        request(&removed, "PATCH", "agent-instances/worker-one", pause)
            .await
            .1,
        paused
    );
    assert_eq!(request(&removed, "PATCH", "agent-instances/worker-one", json!({"requestId":uuid::Uuid::now_v7(),"expectedGeneration":2,"change":{"kind":"state","desired":"running"}})).await.0, StatusCode::FORBIDDEN);
    let (status, stopped) = request(
        &removed,
        "PATCH",
        "agent-instances/worker-one",
        json!({"requestId":uuid::Uuid::now_v7(),"expectedGeneration":2,"change":{"kind":"stop"}}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(stopped["generation"], 3);
    let (status, archived) = request(&removed, "PATCH", "agent-instances/worker-one", json!({"requestId":uuid::Uuid::now_v7(),"expectedGeneration":3,"change":{"kind":"state","desired":"archived"}})).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(archived["generation"], 4);
    let (_, instance) = request(&removed, "GET", "agent-instances/worker-one", Value::Null).await;
    assert_eq!(instance["desired"], "archived");
    assert_eq!(instance["storageGib"], 2);
}
