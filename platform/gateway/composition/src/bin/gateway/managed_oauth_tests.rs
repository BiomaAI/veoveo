//! OAuth issuance routes qualify database-backed Agent client resolution.
use veoveo_agent_runtime::persistence::AgentRepository;

use axum::{Extension, Router, http::StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;
use veoveo_agent_runtime::gateway::{
    ManagedTemplateCatalog,
    http::{AgentManagementState, models::ModelConnection},
};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalogHandle};
#[path = "../../../../../../testing/fixtures/agent_catalog.rs"]
mod agent_catalog;
#[path = "../../../../../../testing/fixtures/catalog_admission.rs"]
mod catalog_fixture;
#[path = "../../../../../../testing/fixtures/work_context_authority.rs"]
mod work_context_authority;
fn fixture_subject(name: &str) -> AuthenticatedSubject {
    let mut subject = work_context_authority::subject(name);
    subject.access_token.session_family = None;
    subject
}
fn managed_state(store: &veoveo_platform_store::PlatformStore) -> AgentManagementState {
    let catalog = GatewayCatalogHandle::new(Arc::new(agent_catalog::fixture_catalog()));

    let template = json!({
        "id":"bounded", "name":"Bounded worker", "tenant":"test", "workContexts":["shared"],
        "requiredDeployerScopes":["operator:use"], "profile":"operator", "scopes":["operator:use"], "roles":[], "membership":"contributor",
        "models":["approved"], "tools":[], "resourceSubscriptions":[],
        "parameters":{"session":{"label":"Session", "shape":{"kind":"identifier","maxLength":40}, "environmentVariable":"VEOVEO_PARAM_SESSION"}},
        "workload":{"namespace":"agents", "configMap":"bounded-template", "configDigest":format!("sha256:{}", "b".repeat(64)), "image":format!("registry.test/kernel@sha256:{}", "a".repeat(64)),
            "databaseSecret":"agent-store", "storageClass":"local-path", "storageGib":2, "cpuMillis":500, "memoryMib":1024,
            "modelSecrets":[{"reference":"media_provider_api_key", "secret":"agent-model", "key":"api-key"}]}
    });
    let templates = Arc::new(
        ManagedTemplateCatalog::from_json(&json!([template]).to_string(), &catalog.current())
            .unwrap(),
    );
    let gateway = crate::bindings::gateway_state(store.clone(), templates.clone()).unwrap();
    let model:ModelConnection=serde_json::from_value(json!({"id":"approved","name":"Approved model","provider":"Fixture","tenant":"test","workContexts":["shared"],"baseUrl":"https://provider.test/v1","model":"model","apiKey":"media_provider_api_key","limits":{"maxOutputTokens":128,"maxCompletionCalls":4,"maxToolCalls":8,"deadlineSeconds":120}})).unwrap();
    let scope = veoveo_mcp_gateway::http::ModuleTaskScope::new();
    AgentManagementState {
        agent_control: veoveo_agent_runtime::AgentControl::new(store.clone()).unwrap(),
        gateway,
        catalog,
        templates,
        models: Arc::new(vec![model]),
        capabilities:
            veoveo_agent_runtime::gateway::capabilities::NativeAgentCapabilityReader::new(
                std::num::NonZeroU16::new(1).unwrap(),
                &veoveo_mcp_contract::PublicDeployment::new("https://workspace.test").unwrap(),
                veoveo_agent_runtime::gateway::capabilities::client_config(),
            )
            .unwrap()
            .shared(),
        stop: scope.cancellation_token(),
        scope,
        definition_limit: 10,
        instance_limits: veoveo_agent_runtime::gateway::http::instance_limits().unwrap(),
    }
}
fn app(state: &AgentManagementState, subject: AuthenticatedSubject) -> Router {
    veoveo_agent_runtime::gateway::http::router(state.clone())
        .unwrap()
        .layer(Extension(subject))
}
async fn request(app: &Router, method: &str, path: &str, value: Value) -> (StatusCode, Value) {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(format!("/admin/operator/{path}"))
                .header("content-type", "application/json")
                .header("authorization", "Bearer explicit-authoring-fixture")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}
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
async fn managed_token_http_route_resolves_durable_clients_before_resource_routing() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
        routing::post,
    };
    use parking_lot::RwLock;
    use std::num::NonZeroU32;
    use tower::ServiceExt;
    use veoveo_agent_runtime::persistence::instances::*;
    use veoveo_mcp_gateway::{GatewayRefreshDeliveryWindow, RefreshTokenDeliveryCipher};

    let db = crate::test_store::TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            crate::test_store::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let state = managed_state(&db.a);
    let _stop = state.stop.clone().drop_guard();
    tokio::time::timeout(std::time::Duration::from_secs(180), async {
        work_context_authority::setup(&db.a).await;
        let human = fixture_subject("Alice");
        let alice = app(&state, human.clone());
        let definition = published(&alice).await;
        let (status, _) = request(&alice, "POST", "agent-instances", json!({"requestId":uuid::Uuid::now_v7(),"id":"worker-one","name":"Worker","definition":"worker","revision":definition["publishedDigest"]})).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let actor = state
            .execution_authority(&"operator".parse().unwrap(), &human)
            .await
            .unwrap();
        let instance = AgentRepository::new(db.a.clone())
            .managed_agent(&actor, "worker-one")
            .await
            .unwrap();
        let owner = uuid::Uuid::now_v7();
        let claim = AgentRepository::new(db.a.clone())
            .claim_managed_agent_operation(instance.operation, owner)
            .await
            .unwrap()
            .unwrap()
            .claim(owner)
            .unwrap();
        AgentRepository::new(db.a.clone())
            .observe_managed_agent(&claim, ManagedAgentPhase::Credentials, None)
            .await
            .unwrap();
        AgentRepository::new(db.a.clone())
            .register_managed_agent_key(
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
            AgentRepository::new(db.a.clone())
                .observe_managed_agent(&claim, phase, None)
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
        for (case, resource) in [
            ("explicit_worker", Some(instance.identity.resource.as_str())),
            ("implicit_worker", None),
            ("other_resource", Some("https://veoveo.example/mcp/admin")),
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
            eprintln!("managed OAuth probe case={case} method=POST route=/oauth/token grant_type=client_credentials client_id=fixture status={}", response.status().as_u16());
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "managed OAuth probe case={case}");
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
        eprintln!("managed OAuth probe case=disabled_worker method=POST route=/oauth/token grant_type=client_credentials client_id=fixture status={}", response.status().as_u16());
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "managed OAuth probe case=disabled_worker");
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
        assert_eq!(
            body["error_description"],
            "client is not registered for this protected resource"
        );
    }).await.expect("managed OAuth route operation exceeded 180 seconds");
}
