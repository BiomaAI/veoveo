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

async fn publish_current_template(app: &Router, session: &str) -> Value {
    let (_, choices) = request(app, "GET", "agent-templates", Value::Null).await;
    let (_, definition) = request(app, "GET", "agent-definitions/worker", Value::Null).await;
    let (_, draft) = request(app, "GET", "agent-definitions/worker/draft", Value::Null).await;
    let mut content = draft["content"].clone();
    content["execution"]["templateRevision"] = choices[0]["revision"].clone();
    content["execution"]["parameters"]["session"] = json!(session);
    let (status, saved) = request(app, "PUT", "agent-definitions/worker/draft", json!({"requestId":uuid::Uuid::now_v7(),"expectedRevision":definition["revision"],"content":content})).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let (status, published) = request(app, "POST", "agent-definitions/worker/publish", json!({"requestId":uuid::Uuid::now_v7(),"expectedRevision":saved["revision"],"digest":saved["draftDigest"],"audience":["shared"]})).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    published
}

#[tokio::test]
async fn approved_image_adoption_preserves_bindings_and_replays_after_template_removal() {
    use veoveo_mcp_gateway::managed_agents::ManagedTemplateCatalog;
    let db = crate::test_store::TestDb::new().await;
    crate::workspace::tests::setup(&db.a).await;
    let mut state = managed_state(&db.a);
    let _stop = state.stop.clone().drop_guard();
    let alice = app(&state, fixture_subject("Alice"));
    let definition = published(&alice).await;
    let (status, _) = request(&alice, "POST", "agent-instances", json!({"requestId":uuid::Uuid::now_v7(),"id":"worker-one","name":"Worker","definition":"worker","revision":definition["publishedDigest"]})).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let actor = authority::admit(
        &state,
        "operator".into(),
        fixture_subject("Alice"),
        Action::AgentDefinitionsRead,
    )
    .await
    .unwrap();
    let before =
        db.a.managed_agent(&actor.authority, "worker-one")
            .await
            .unwrap();
    let original = state
        .gateway
        .managed_templates()
        .get(&wire::AgentTemplateId::new("bounded").unwrap())
        .unwrap()
        .clone();
    let new_image = format!("registry.test/kernel@sha256:{}", "c".repeat(64));
    let install = |state: &mut AgentManagementState, template: wire::RuntimeTemplate| {
        state.gateway = state.gateway.clone().with_managed_templates(
            ManagedTemplateCatalog::from_json(
                &serde_json::to_string(&vec![template]).unwrap(),
                &state.catalog.current(),
            )
            .unwrap(),
        );
        app(state, fixture_subject("Alice"))
    };
    let change = |revision: &Value| json!({"requestId":uuid::Uuid::now_v7(),"expectedGeneration":1,"change":{"kind":"revision","revision":revision["publishedDigest"]}});

    // A new approved template cannot silently reinterpret retained storage or
    // configuration. Each candidate still passes ordinary publication validation.
    for field in ["storage", "config", "authority"] {
        let mut template = original.clone();
        template.workload.image = new_image.clone();
        match field {
            "storage" => template.workload.storage_gib += 1,
            "config" => {
                template.workload.config_digest =
                    veoveo_mcp_contract::Sha256Digest::from_hex("d".repeat(64)).unwrap()
            }
            "authority" => {
                template.membership = veoveo_mcp_contract::WorkContextMembershipLevel::Viewer
            }
            _ => unreachable!(),
        }
        let revised = install(&mut state, template);
        let candidate = publish_current_template(&revised, "session-one").await;
        assert_eq!(
            request(
                &revised,
                "PATCH",
                "agent-instances/worker-one",
                change(&candidate)
            )
            .await
            .0,
            StatusCode::CONFLICT,
            "{field}"
        );
        assert_eq!(
            db.a.managed_agent(&actor.authority, "worker-one")
                .await
                .unwrap()
                .resources,
            before.resources
        );
    }

    let mut approved = original;
    approved.workload.image = new_image.clone();
    let upgraded = install(&mut state, approved);
    let different_target = publish_current_template(&upgraded, "session-two").await;
    assert_eq!(
        request(
            &upgraded,
            "PATCH",
            "agent-instances/worker-one",
            change(&different_target)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let candidate = publish_current_template(&upgraded, "session-one").await;
    assert_eq!(
        db.a.managed_agent(&actor.authority, "worker-one")
            .await
            .unwrap()
            .requested_revision,
        before.requested_revision,
        "publication does not upgrade an instance"
    );
    let body = change(&candidate);
    let (status, operation) = request(
        &upgraded,
        "PATCH",
        "agent-instances/worker-one",
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{operation}");
    assert_eq!(operation["generation"], 2);
    let after =
        db.a.managed_agent(&actor.authority, "worker-one")
            .await
            .unwrap();
    assert_eq!(after.identity, before.identity);
    assert_eq!(after.principal, before.principal);
    assert_eq!(after.public_key, before.public_key);
    assert_eq!(after.active_revision, before.active_revision);
    let mut resources = before.resources;
    resources.image = new_image;
    assert_eq!(after.resources, resources);

    state.gateway = state
        .gateway
        .clone()
        .with_managed_templates(Default::default());
    let removed = app(&state, fixture_subject("Alice"));
    assert_eq!(
        request(
            &removed,
            "PATCH",
            "agent-instances/worker-one",
            body.clone()
        )
        .await
        .1,
        operation
    );
    let mut different = body;
    different["change"]["revision"] = definition["publishedDigest"].clone();
    assert_eq!(
        request(&removed, "PATCH", "agent-instances/worker-one", different)
            .await
            .0,
        StatusCode::CONFLICT
    );
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

#[tokio::test]
async fn managed_dispatch_rechecks_model_generation_epoch_and_revocation() {
    use veoveo_platform_store::agent_management::instances::*;
    let db = crate::test_store::TestDb::new().await;
    crate::workspace::tests::setup(&db.a).await;
    let state = managed_state(&db.a);
    let _stop = state.stop.clone().drop_guard();
    let human = fixture_subject("Alice");
    let alice = app(&state, human.clone());
    let definition = published(&alice).await;
    let (status, _) = request(&alice, "POST", "agent-instances", json!({"requestId":uuid::Uuid::now_v7(),"id":"worker-one","name":"Worker","definition":"worker","revision":definition["publishedDigest"]})).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let actor = authority::admit(
        &state,
        "operator".into(),
        human.clone(),
        Action::AgentDefinitionsRead,
    )
    .await
    .unwrap();
    let instance =
        db.a.managed_agent(&actor.authority, "worker-one")
            .await
            .unwrap();
    let owner = uuid::Uuid::now_v7();
    let claim =
        db.a.claim_managed_agent_operation(instance.operation, owner)
            .await
            .unwrap()
            .unwrap()
            .claim(owner)
            .unwrap();
    db.a.observe_managed_agent(&claim, ManagedAgentPhase::Credentials, None)
        .await
        .unwrap();
    db.a.register_managed_agent_key(
        &claim,
        ManagedAgentPublicKey {
            kid: "test".into(),
            n: "public-modulus".into(),
            e: "AQAB".into(),
        },
    )
    .await
    .unwrap();
    for phase in [
        ManagedAgentPhase::Storage,
        ManagedAgentPhase::Draining,
        ManagedAgentPhase::Workload,
        ManagedAgentPhase::Ready,
    ] {
        db.a.observe_managed_agent(&claim, phase, None)
            .await
            .unwrap();
    }
    assert!(
        db.a.automated_authority_for_oauth_client("test", "shared", &instance.identity.client_id)
            .await
            .unwrap()
            .is_some()
    );
    let mut service = human.clone();
    service.principal.kind = veoveo_mcp_contract::PrincipalKind::Service;
    service.principal.id = format!(
        "{}#{}",
        instance.identity.issuer, instance.identity.client_id
    )
    .parse()
    .unwrap();
    service.principal.issuer = instance.identity.issuer.parse().unwrap();
    service.principal.subject = instance.identity.client_id.parse().unwrap();
    service.principal.groups.clear();
    service.actor = service.principal.clone();
    service.authority.provenance = veoveo_mcp_contract::InvocationProvenance::Automated;
    service.access_token.oauth_client_id = instance.identity.client_id.parse().unwrap();
    service.access_token.issuer = service.principal.issuer.clone();
    service.access_token.subject = service.principal.subject.clone();
    service.access_token.audience = instance.identity.resource.parse().unwrap();
    service.access_token.invocation_mode = veoveo_mcp_contract::InvocationMode::Automated;
    service.access_token.initiator = None;
    service.access_token.managed_agent = Some(wire::ManagedAgentToken {
        instance: wire::AgentManagedInstanceId::new("worker-one").unwrap(),
        generation: 1,
        epoch: 1,
    });
    let runtime = veoveo_agent_runtime::AgentRuntime::register(
        db.a.clone(),
        veoveo_agent_runtime::AgentSpec {
            tenant_key: "test".into(),
            agent_key: "worker-one".into(),
            display_name: "Worker".into(),
            profile: "operator".into(),
            authority: db
                .a
                .automated_authority_for_oauth_client(
                    "test",
                    "shared",
                    &instance.identity.client_id,
                )
                .await
                .unwrap()
                .unwrap(),
            manifest: veoveo_platform_store::OpenObject::default(),
            memory_database: "memory.duckdb".into(),
        },
        veoveo_agent_runtime::AgentInstanceId::new(),
    )
    .await
    .unwrap();
    runtime
        .acquire_lease(std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let body = json!({"generation":1,"epoch":1,"leaseOwner":runtime.instance_id().as_uuid(),"leaseFence":runtime.lease_fence().unwrap()});
    let worker = app(&state, service.clone());
    assert_eq!(
        request(&worker, "POST", "agent-runtime/dispatch", body.clone())
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        request(&alice, "POST", "agent-runtime/dispatch", body.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &worker,
            "POST",
            "agent-runtime/dispatch",
            json!({"generation":2,"epoch":1,"leaseOwner":runtime.instance_id().as_uuid(),"leaseFence":runtime.lease_fence().unwrap()})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let mut stale_lease = body.clone();
    stale_lease["leaseOwner"] = json!(uuid::Uuid::now_v7());
    assert_eq!(
        request(&worker, "POST", "agent-runtime/dispatch", stale_lease)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let mut removed_model = state.clone();
    removed_model.models = Arc::new(vec![]);
    assert_eq!(
        request(
            &app(&removed_model, service.clone()),
            "POST",
            "agent-runtime/dispatch",
            body.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(request(&alice, "PATCH", "agent-instances/worker-one", json!({"requestId":uuid::Uuid::now_v7(),"expectedGeneration":1,"change":{"kind":"state","desired":"paused"}})).await.0, StatusCode::ACCEPTED);
    assert_eq!(
        request(&worker, "POST", "agent-runtime/dispatch", body.clone())
            .await
            .0,
        StatusCode::NO_CONTENT,
        "pause drains the admitted episode"
    );
    assert_eq!(request(&alice, "PATCH", "agent-instances/worker-one", json!({"requestId":uuid::Uuid::now_v7(),"expectedGeneration":2,"change":{"kind":"stop"}})).await.0, StatusCode::ACCEPTED);
    assert_eq!(
        request(&worker, "POST", "agent-runtime/dispatch", body.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    service.access_token.managed_agent.as_mut().unwrap().epoch = 2;
    assert_eq!(
        request(
            &app(&state, service),
            "POST",
            "agent-runtime/dispatch",
            body
        )
        .await
        .0,
        StatusCode::FORBIDDEN,
        "credential rotation cannot replace the episode epoch"
    );
    db.a.client()
        .query("UPDATE ONLY $principal SET enabled = false;")
        .bind(("principal", instance.principal))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        db.a.automated_authority_for_oauth_client("test", "shared", &instance.identity.client_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn managed_token_http_route_resolves_durable_clients_before_resource_routing() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
        routing::post,
    };
    use parking_lot::RwLock;
    use std::num::NonZeroU32;
    use tower::ServiceExt;
    use veoveo_mcp_gateway::{GatewayRefreshDeliveryWindow, RefreshTokenDeliveryCipher};
    use veoveo_platform_store::agent_management::instances::*;

    let db = crate::test_store::TestDb::new().await;
    crate::workspace::tests::setup(&db.a).await;
    let state = managed_state(&db.a);
    let _stop = state.stop.clone().drop_guard();
    let human = fixture_subject("Alice");
    let alice = app(&state, human.clone());
    let definition = published(&alice).await;
    let (status, _) = request(&alice, "POST", "agent-instances", json!({"requestId":uuid::Uuid::now_v7(),"id":"worker-one","name":"Worker","definition":"worker","revision":definition["publishedDigest"]})).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let actor = authority::admit(
        &state,
        "operator".into(),
        human,
        Action::AgentDefinitionsRead,
    )
    .await
    .unwrap();
    let instance =
        db.a.managed_agent(&actor.authority, "worker-one")
            .await
            .unwrap();
    let owner = uuid::Uuid::now_v7();
    let claim =
        db.a.claim_managed_agent_operation(instance.operation, owner)
            .await
            .unwrap()
            .unwrap()
            .claim(owner)
            .unwrap();
    db.a.observe_managed_agent(&claim, ManagedAgentPhase::Credentials, None)
        .await
        .unwrap();
    db.a.register_managed_agent_key(
        &claim,
        ManagedAgentPublicKey {
            kid: "test".into(),
            n: "public-modulus".into(),
            e: "AQAB".into(),
        },
    )
    .await
    .unwrap();
    for phase in [
        ManagedAgentPhase::Storage,
        ManagedAgentPhase::Draining,
        ManagedAgentPhase::Workload,
    ] {
        db.a.observe_managed_agent(&claim, phase, None)
            .await
            .unwrap();
    }
    let catalog = state.catalog.current();
    assert!(
        catalog
            .oauth_client(&instance.identity.client_id.parse().unwrap())
            .is_none()
    );
    let app_state = crate::runtime::AppState {
        catalog: state.catalog.clone(),
        gateway_state: state.gateway.clone(),
        http: Arc::new(RwLock::new(reqwest::Client::new())),
        public_base_url: "https://veoveo.example".into(),
        refresh_delivery_cipher: RefreshTokenDeliveryCipher::new(&[42; 32]).unwrap(),
        refresh_delivery_window: GatewayRefreshDeliveryWindow::from_seconds(
            NonZeroU32::new(10).unwrap(),
        )
        .unwrap(),
    };
    let tokens = Router::new()
        .route("/oauth/token", post(crate::oauth::token_endpoint))
        .with_state(app_state);
    // Both explicit and implicit resource routing must reach assertion validation.
    // The fixture deliberately has no private key: issuance is installed acceptance.
    for resource in [
        Some(instance.identity.resource.as_str()),
        None,
        Some("https://veoveo.example/mcp/admin"),
    ] {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "client_credentials")
            .append_pair("client_id", &instance.identity.client_id);
        if let Some(resource) = resource {
            form.append_pair("resource", resource);
        }
        let response = tokens
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/oauth/token")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(form.finish()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
        assert_eq!(body["error"], "invalid_client");
        assert_eq!(
            body["error_description"],
            if resource.is_some_and(|r| r.ends_with("/admin")) {
                "client is not registered for this protected resource"
            } else {
                "client authentication failed"
            }
        );
    }
    let (status, _) = request(
        &alice,
        "POST",
        "agent-definitions/worker/disable",
        json!({"requestId":uuid::Uuid::now_v7(),"expectedRevision":definition["revision"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", "client_credentials")
        .append_pair("client_id", &instance.identity.client_id)
        .append_pair("resource", &instance.identity.resource);
    let response = tokens
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/token")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(form.finish()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(
        body["error_description"],
        "client is not registered for this protected resource"
    );
}
