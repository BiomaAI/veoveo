//! Real-store HTTP/MCP maintenance admission; no provider or image qualification.
use crate::{Server, Signing, app_support, client, rpc, sdk, support, template};
use rmcp::model::{ServerNotification, SubscriptionFilter};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;
use veoveo_computers::{ComputersStore, Reservation, api::*};
use veoveo_computers_mcp::{MaintenanceProfiles, MaintenanceTransition};
use veoveo_mcp_contract::LocalToolName;
use veoveo_task_runtime::TaskRuntime;

fn control() -> veoveo_mcp_contract::GatewayControlPlane {
    let mut config = app_support::control();
    for name in ["update_template", "resume_update"] {
        let tool = LocalToolName::new(name).unwrap();
        config.servers[0].tools.push(tool.clone());
        config.policies[0].rules[0].tools.insert(tool);
    }
    config
}

#[tokio::test]
async fn maintenance_http_mcp_retry_and_current_task_authority_share_one_fence() {
    let db = support::TestDb::new().await;
    app_support::identities(&db).await;
    support::policy::install(&db.a, control()).await;
    let source = template::retained_template(format!(
        "fixture.invalid/computer@sha256:{}",
        "f".repeat(64)
    ));
    let target = template::retained_template(format!(
        "fixture.invalid/computer@sha256:{}",
        "e".repeat(64)
    ));
    let profiles = MaintenanceProfiles::new(
        vec![source.clone(), target.clone()],
        vec![
            MaintenanceTransition {
                source_fingerprint: source.fingerprint(),
                target_fingerprint: target.fingerprint(),
            },
            MaintenanceTransition {
                source_fingerprint: source.fingerprint(),
                target_fingerprint: source.fingerprint(),
            },
        ],
    )
    .unwrap();
    let (left, _left_health) = app_support::application_on(db.a.clone(), true).await;
    let (right, _right_health) = app_support::application_on(db.b.clone(), false).await;
    let signing = Signing::new();
    let left = Server::new(left.with_maintenance(profiles.clone()).unwrap(), &signing).await;
    let right = Server::new(right.with_maintenance(profiles).unwrap(), &signing).await;
    let store = ComputersStore::new(db.a.clone(), Uuid::from_u128(100)).unwrap();
    let owner = support::owner("alice");
    let computer = store
        .reserve(
            &owner,
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development-retained".into(),
                template_fingerprint: source.fingerprint(),
            },
        )
        .await
        .unwrap()
        .computer_id;
    db.a.client().query("UPDATE ONLY $computer SET phase = 'ready', provider_resource_id = 'private-source-resource', process_id = 'private-source-process';")
        .bind(("computer", surrealdb::types::RecordId::new("computer", surrealdb::types::Uuid::from(computer))))
        .await.unwrap().check().unwrap();
    let client = client();
    let alice = signing.bearer("alice", "computers");
    let bob = signing.bearer("bob", "computers");
    let uri = maintenance_uri(computer);
    let state: MaintenanceState = client
        .get(format!(
            "{}/admin/computers/{computer}/maintenance",
            left.base
        ))
        .bearer_auth(&alice)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(state.can_update);
    assert_eq!(state.targets.len(), 2);
    assert!(state.active.is_none());
    let input = json!({"computerId":computer,"requestId":Uuid::now_v7()});
    let denied = rpc(
        &client,
        &left,
        &alice,
        "tools/call",
        json!({"name":"update_template","arguments":input}),
        false,
    )
    .await;
    assert!(denied.get("error").is_some());
    assert!(
        store
            .get(&owner, computer)
            .await
            .unwrap()
            .active_operation
            .is_none()
    );
    let rejected = client
        .post(format!(
            "{}/admin/computers/{computer}/update-template",
            left.base
        ))
        .bearer_auth(&alice)
        .json(&json!({"computerId":computer,"requestId":Uuid::now_v7(),"image":"untrusted"}))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    let created = client
        .post(format!(
            "{}/admin/computers/{computer}/update-template",
            left.base
        ))
        .bearer_auth(&alice)
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), reqwest::StatusCode::ACCEPTED);
    let operation: MaintenanceView = created.json().await.unwrap();
    assert_eq!(operation.target_template_id, "development");
    let retry = rpc(
        &client,
        &right,
        &alice,
        "tools/call",
        json!({"name":"update_template","arguments":input}),
        true,
    )
    .await;
    assert_eq!(
        retry["result"]["taskId"],
        operation.task_id.to_string(),
        "{retry}"
    );
    let id = operation.task_id.to_string();
    let peer = sdk(&right, alice.clone()).await;
    let mut stream = peer
        .listen(
            SubscriptionFilter::builder()
                .task_id(id.clone())
                .resource_subscription(uri.clone())
                .build(),
        )
        .await
        .unwrap();
    for _ in 0..2 {
        assert!(
            tokio::time::timeout(Duration::from_secs(5), stream.next())
                .await
                .unwrap()
                .unwrap()
                .is_some()
        );
    }
    for server in [&left, &right] {
        let state = rpc(
            &client,
            server,
            &alice,
            "resources/read",
            json!({"uri":uri}),
            true,
        )
        .await;
        assert!(state.get("error").is_none(), "{state}");
        let body = state["result"]["contents"][0]["text"].as_str().unwrap();
        let state: MaintenanceState = serde_json::from_str(body).unwrap();
        assert!(!state.can_update);
        assert_eq!(state.active, Some(operation.clone()));
        for private in [
            source.fingerprint(),
            target.fingerprint(),
            "private-source-resource".into(),
            "private-source-process".into(),
        ] {
            assert!(!body.contains(&private));
        }
        for method in ["tasks/get", "tasks/cancel"] {
            assert!(
                rpc(&client, server, &bob, method, json!({"taskId":id}), true)
                    .await
                    .get("error")
                    .is_some()
            );
        }
    }
    let mut denied = control();
    denied.policies[0].rules[0]
        .tools
        .remove(&LocalToolName::new("update_template").unwrap());
    support::policy::install(&db.b, denied).await;
    assert!(
        rpc(
            &client,
            &left,
            &alice,
            "tasks/get",
            json!({"taskId":id}),
            true
        )
        .await
        .get("error")
        .is_none()
    );
    assert!(
        rpc(
            &client,
            &right,
            &alice,
            "tasks/cancel",
            json!({"taskId":id}),
            true
        )
        .await
        .get("error")
        .is_some()
    );
    assert!(
        rpc(
            &client,
            &right,
            &alice,
            "tools/call",
            json!({"name":"update_template","arguments":input}),
            true
        )
        .await
        .get("error")
        .is_some()
    );
    support::policy::install(&db.b, control()).await;
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "maintenance-inspect");
    let claim = tasks
        .claim_observation(&id, Duration::from_secs(30))
        .await
        .unwrap();
    let ticket = store.begin_maintenance_step(&claim).await.unwrap();
    let original_dispatch = ticket.operation().steps()[0].dispatch_id;
    drop(ticket);
    let paused = store
        .pause_maintenance(
            &claim,
            veoveo_computers::maintenance::MaintenanceRecovery::BudgetExhausted,
        )
        .await
        .unwrap();
    tasks.release_observation(&claim).await.unwrap();
    let resource_peer = sdk(&right, alice.clone()).await;
    let mut resources = resource_peer
        .listen(
            SubscriptionFilter::builder()
                .resource_subscription(uri.clone())
                .build(),
        )
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), resources.next())
            .await
            .unwrap()
            .unwrap()
            .is_some()
    );
    let cancelled = rpc(
        &client,
        &left,
        &alice,
        "tasks/cancel",
        json!({"taskId":id}),
        true,
    )
    .await;
    assert!(cancelled.get("error").is_none(), "{cancelled}");
    let task = tasks.get(&id).await.unwrap().unwrap();
    assert!(task.cancel_requested_at.is_some());
    let event = tokio::time::timeout(Duration::from_secs(5), resources.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        ServerNotification::ResourceUpdatedNotification(_)
    ));
    resource_peer.cancel().await.unwrap();
    let url = format!(
        "{}/admin/computers/{computer}/maintenance/{}",
        left.base, paused.operation_id
    );
    let current: MaintenanceView = client
        .get(&url)
        .bearer_auth(&alice)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(current.can_resume);
    assert_eq!(current.pending_cancellation_at, task.cancel_requested_at);
    let mut resume = ResumeUpdateInput {
        computer_id: computer,
        task_id: paused.operation_id,
        request_id: Uuid::now_v7(),
        expected_updated_at: current.updated_at,
        acknowledged_cancellation_at: None,
    };
    assert_eq!(
        client
            .post(format!("{url}/resume"))
            .bearer_auth(&alice)
            .json(&resume)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::CONFLICT
    );
    resume.acknowledged_cancellation_at = current.pending_cancellation_at;
    let args = json!({"name":"resume_update","arguments":resume});
    assert!(
        rpc(&client, &right, &alice, "tools/call", args.clone(), false)
            .await
            .get("error")
            .is_some()
    );
    assert_ne!(
        client
            .post(format!("{url}/resume"))
            .bearer_auth(&bob)
            .json(&resume)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::ACCEPTED
    );
    let mut denied = control();
    denied.policies[0].rules[0]
        .tools
        .remove(&LocalToolName::new("resume_update").unwrap());
    support::policy::install(&db.a, denied).await;
    let denied: MaintenanceView = client
        .get(&url)
        .bearer_auth(&alice)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!denied.can_resume);
    assert_eq!(
        client
            .post(format!("{url}/resume"))
            .bearer_auth(&alice)
            .json(&resume)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    support::policy::install(&db.a, control()).await;
    let response = client
        .post(format!("{url}/resume"))
        .bearer_auth(&alice)
        .json(&resume)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
    let resumed: MaintenanceView = response.json().await.unwrap();
    assert_eq!(resumed.phase, MaintenancePhase::Stopping);
    assert!(!resumed.can_resume);
    assert_eq!(resumed.pending_cancellation_at, None);
    let retried = rpc(&client, &right, &alice, "tools/call", args, true).await;
    assert_eq!(retried["result"]["taskId"], id);
    let saved = store
        .maintenance(&owner, paused.operation_id)
        .await
        .unwrap();
    assert_eq!(saved.steps()[0].dispatch_id, original_dispatch);
    assert_eq!(saved.steps()[0].observation_reads, 0);
    assert_eq!(saved.target_instance_id, paused.target_instance_id);
    let mut denied = control();
    denied.policies[0].rules[1].effect = veoveo_mcp_contract::PolicyEffect::Deny;
    support::policy::install(&db.b, denied).await;
    tokio::time::timeout(Duration::from_secs(8), async {
        while let Ok(Some(_)) = stream.next().await {}
    })
    .await
    .expect("maintenance listener remained open after read revocation");
    peer.cancel().await.unwrap();
}
