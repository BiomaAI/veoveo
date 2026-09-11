//! Public admission uses a real Artifact capability service and independent stores.
//! Native provider effects are qualified by the separate worker fixture.
use crate::{Server, Signing, client, command_support, rpc, support, template};
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use uuid::Uuid;
use veoveo_computers::{ComputerActor, api::*};
use veoveo_computers_mcp::{Application, CapacityHealth, NamedTemplate, RuntimeAccess, Templates};
use veoveo_mcp_contract::*;
use veoveo_task_runtime::TaskRuntime;

struct ArtifactServer(tokio::task::JoinHandle<()>);
impl Drop for ArtifactServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn availability(
    State(available): State<Arc<AtomicBool>>,
    request: Request,
    next: Next,
) -> Response {
    if !available.load(Ordering::SeqCst) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    next.run(request).await
}

#[tokio::test]
async fn public_command_admission_repairs_one_task_with_actual_artifact_authority() {
    let db = support::TestDb::new().await;
    let (a, b, owner, old_agent, computer) = support::automation::setup(&db).await;
    let mut identity = support::identity(old_agent.owner());
    identity.authority.output_policy = support::automation::control().work_contexts[0]
        .output_policy
        .clone();
    let agent = ComputerActor::from_verified(&identity).unwrap();
    let selected = template::retained_template(format!(
        "fixture.invalid/computer@sha256:{}",
        "f".repeat(64)
    ));
    db.a.client()
        .query("UPDATE ONLY $computer SET template_fingerprint = $fingerprint;")
        .bind(("computer", command_support::computer_record(computer)))
        .bind(("fingerprint", selected.fingerprint()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let signing = Signing::new();
    let ready = Arc::new(AtomicBool::new(false));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let artifacts = veoveo_artifact_service::ArtifactService::new(
        veoveo_artifact_service::SurrealArtifactRepository::new(db.a.clone()),
        veoveo_artifact_service::ObjectStoreConfig::Memory
            .build()
            .unwrap(),
    );
    let auth = veoveo_artifact_service::PlaneAuthenticator::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
        vec![ServerSlug::new("computers").unwrap()],
        signing.trust.clone(),
    );
    let router: Router = veoveo_artifact_service::http::router(
        veoveo_artifact_service::http::AppState::new(artifacts, auth),
    )
    .layer(middleware::from_fn_with_state(ready.clone(), availability));
    let _artifacts = ArtifactServer(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let (_health, health) = tokio::sync::watch::channel(CapacityHealth {
        availability: CapacityAvailability::ComputeUnavailable,
        observed_at: Instant::now(),
    });
    let app = |store, platform| {
        Application::new(
            store,
            TaskRuntime::new(platform, "computers", "public-admission"),
            Templates::new(
                vec![NamedTemplate::new("development".into(), selected.clone()).unwrap()],
                Some(selected.fingerprint()),
            )
            .unwrap(),
            health.clone(),
            RuntimeAccess::unavailable(),
        )
        .unwrap()
        .with_execution(
            Arc::new(command_support::keys()),
            veoveo_artifact_client::HttpArtifactPlane::new(&endpoint),
            [selected.fingerprint()].into(),
        )
        .unwrap()
    };
    let left = Server::new(app(a.clone(), db.a.clone()), &signing).await;
    let right = Server::new(app(b, db.b.clone()), &signing).await;
    let expires = chrono::Utc::now() + chrono::TimeDelta::minutes(2);
    let owner_token = signing.identity(support::identity(owner.owner()), "computers", expires);
    let agent_token = signing.identity(identity, "computers", expires);
    let client = client();
    let grant_input = support::automation::input(computer);
    let issued = rpc(
        &client,
        &left,
        &owner_token,
        "tools/call",
        json!({"name":"grant_automation","arguments":grant_input}),
        false,
    )
    .await;
    let grant: AutomationGrantResult =
        serde_json::from_value(issued["result"]["structuredContent"].clone()).unwrap();
    assert_eq!(
        issued["result"]["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["type"] == "resource_link")
            .count(),
        1
    );
    let issue_retry = client
        .post(format!(
            "{}/admin/computers/{computer}/automation",
            right.base
        ))
        .bearer_auth(&owner_token)
        .json(&grant_input)
        .send()
        .await
        .unwrap();
    assert_eq!(issue_retry.status(), StatusCode::OK);
    assert_eq!(
        issue_retry.json::<AutomationGrantResult>().await.unwrap(),
        grant
    );
    let inventory = client
        .get(format!(
            "{}/admin/computers/{computer}/automation",
            left.base
        ))
        .bearer_auth(&owner_token)
        .send()
        .await
        .unwrap()
        .json::<AutomationGrantCollection>()
        .await
        .unwrap();
    assert!(inventory.can_grant && inventory.can_revoke);
    assert_eq!(inventory.grants.len(), 1);
    assert_eq!(
        inventory.limits.maximum_execution_seconds,
        support::automation::POLICY.maximum_execution_seconds
    );
    let mut input = json!({
        "computerId":computer,"grantId":grant.grant.grant_id,"requestId":Uuid::now_v7(),
        "arguments":["/bin/printf","private-public-command-argument"],"directory":".",
        "environment":{"PRIVATE_TOKEN":"private-public-command-environment"},
        "stdin":STANDARD.encode(vec![0xff; 1024 * 1024]),
        "limits":{"maximumSeconds":30,"maximumOutputBytes":1024,"onInterruption":"stop_computer"}
    });
    let missing = rpc(
        &client,
        &left,
        &agent_token,
        "tools/call",
        json!({"name":"execute","arguments":input}),
        false,
    )
    .await;
    assert!(missing.get("error").is_some());
    assert!(a.pending_commands(None, 100).await.unwrap().is_empty());
    for directory in ["../outside", "/etc", "nested/../../outside"] {
        let mut invalid = input.clone();
        invalid["directory"] = directory.into();
        let response = rpc(
            &client,
            &left,
            &agent_token,
            "tools/call",
            json!({"name":"execute","arguments":invalid}),
            true,
        )
        .await;
        assert_eq!(response["result"]["isError"], true);
        assert!(a.pending_commands(None, 100).await.unwrap().is_empty());
    }
    let first = rpc(
        &client,
        &left,
        &agent_token,
        "tools/call",
        json!({"name":"execute","arguments":input}),
        true,
    )
    .await;
    let id = first["result"]["taskId"]
        .as_str()
        .expect("original Task receipt")
        .to_owned();
    let pending = a.pending_commands(None, 100).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].actor().principal_key,
        agent.owner().principal_key
    );
    assert!(
        pending[0]
            .output_capability_request(&command_support::keys())
            .unwrap()
            .is_some()
    );
    ready.store(true, Ordering::SeqCst);
    let retry = rpc(
        &client,
        &right,
        &agent_token,
        "tools/call",
        json!({"name":"execute","arguments":input}),
        true,
    )
    .await;
    assert_eq!(retry["result"]["taskId"], id);
    let pending = a.pending_commands(None, 100).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert!(
        pending[0]
            .output_capability_request(&command_support::keys())
            .unwrap()
            .is_none(),
        "actual Artifact issuance was not attached"
    );
    input["arguments"][1] = "changed".into();
    let conflict = rpc(
        &client,
        &right,
        &agent_token,
        "tools/call",
        json!({"name":"execute","arguments":input}),
        true,
    )
    .await;
    assert_eq!(conflict["result"]["isError"], true);
    let task = rpc(
        &client,
        &right,
        &owner_token,
        "tasks/get",
        json!({"taskId":id}),
        true,
    )
    .await;
    for response in [&first, &retry, &conflict, &task] {
        let serialized = response.to_string();
        for secret in [
            "private-public-command-argument",
            "private-public-command-environment",
            "ciphertext",
            "bearer_token",
            "////",
        ] {
            assert!(!serialized.contains(secret));
        }
    }
    let revoked = client
        .post(format!(
            "{}/admin/computers/{computer}/automation/{}/revoke",
            right.base, grant.grant.grant_id
        ))
        .bearer_auth(&owner_token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::OK);
    assert!(
        revoked
            .json::<AutomationGrantResult>()
            .await
            .unwrap()
            .grant
            .revoked_at
            .is_some()
    );
    assert!(
        rpc(
            &client,
            &left,
            &agent_token,
            "tasks/get",
            json!({"taskId":id}),
            true
        )
        .await
        .get("error")
        .is_some()
    );
    for token in [&owner_token, &agent_token] {
        let resource = rpc(
            &client,
            &left,
            token,
            "resources/read",
            json!({"uri":grant.result_uri}),
            true,
        )
        .await;
        assert_eq!(resource.get("error").is_none(), token == &owner_token);
    }
}
