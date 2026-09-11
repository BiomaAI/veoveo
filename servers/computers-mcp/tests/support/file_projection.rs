//! Public wire authority and actual-caller Artifact receipts; native byte effects
//! are covered by native_files, not synthesized by this projection fixture.
use crate::{Server, Signing, client, command_support, rpc, support, template};
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use uuid::Uuid;
use veoveo_computers::{ComputerActor, api::*, files::FileOutcome};
use veoveo_computers_mcp::{Application, CapacityHealth, NamedTemplate, RuntimeAccess, Templates};
use veoveo_mcp_contract::*;
use veoveo_task_runtime::{TaskRuntime, TaskTransition};

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
async fn export_admission_repairs_actual_caller_access_and_keeps_task_authority_current() {
    exercise(false).await;
}
#[tokio::test]
async fn import_admission_repairs_actual_caller_access_and_keeps_task_authority_current() {
    exercise(true).await;
}
async fn exercise(import: bool) {
    let db = support::TestDb::new().await;
    let (a, b, owner, old_agent, computer) = support::automation::setup(&db).await;
    let mut control = support::automation::control();
    for name in ["transfer_file", "update_template"] {
        let name = LocalToolName::new(name).unwrap();
        control.servers[0].tools.push(name.clone());
        control.policies[0].rules[0].tools.insert(name);
    }
    support::policy::install(&db.a, control).await;
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
        .query("UPDATE ONLY $computer SET template_fingerprint=$fingerprint;")
        .bind(("computer", command_support::computer_record(computer)))
        .bind(("fingerprint", selected.fingerprint()))
        .await
        .unwrap()
        .check()
        .unwrap();
    // The gateway normally materializes this current Work Context. Read
    // capabilities require that projection as well as the signed caller.
    let context_id = veoveo_platform_store::deterministic_work_context_id("test", "computers-test")
        .unwrap()
        .record_id();
    let context = veoveo_platform_store::WorkContextRecord {
        id: context_id.clone(),
        tenant: veoveo_platform_store::deterministic_tenant_id("test")
            .unwrap()
            .record_id(),
        context_key: "computers-test".into(),
        title: "Computers tests".into(),
        policy_revision: "test-1".into(),
        output_policy: veoveo_platform_store::WorkContextOutputPolicyRecord {
            owner_kind: veoveo_platform_store::ArtifactGrantSubjectKind::Principal,
            owner_key: "https://computers.test#alice".into(),
            initial_grants: vec![],
            classification: None,
            data_labels: vec![],
        },
        memberships: vec![veoveo_platform_store::WorkContextMembershipRuleRecord {
            level: veoveo_platform_store::WorkContextMembershipLevel::Contributor,
            principals: vec![],
            groups: vec![],
            roles: vec![],
            oauth_clients: vec!["console".into(), "service".into(), "delegated".into()],
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _: Option<veoveo_platform_store::WorkContextRecord> =
        db.a.client()
            .create(context_id)
            .content(context)
            .await
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
        availability: CapacityAvailability::Available,
        observed_at: Instant::now(),
    });
    let app = |store, platform| {
        Application::new(
            store,
            TaskRuntime::new(platform, "computers", "file-projection"),
            Templates::new(
                vec![NamedTemplate::new("development".into(), selected.clone()).unwrap()],
                Some(selected.fingerprint()),
            )
            .unwrap(),
            health.clone(),
            RuntimeAccess::unavailable(),
        )
        .unwrap()
        .with_files(
            Arc::new(command_support::keys()),
            veoveo_artifact_client::HttpArtifactPlane::new(&endpoint),
            [selected.fingerprint()].into(),
        )
        .unwrap()
    };
    let left = Server::new(app(a.clone(), db.a.clone()), &signing).await;
    let right = Server::new(app(b.clone(), db.b.clone()), &signing).await;
    let expires = chrono::Utc::now() + chrono::TimeDelta::minutes(3);
    let owner_token = signing.identity(support::identity(owner.owner()), "computers", expires);
    let agent_token = signing.identity(identity, "computers", expires);
    let foreign = signing.bearer("bob", "computers");
    let client = client();
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let artifact_id = Uuid::now_v7();
    let transfer = if import {
        json!({"kind":"import","artifactId":artifact_id,"path":"private-public-transfer-path"})
    } else {
        json!({"kind":"export","path":"private-public-transfer-path","filename":"output.bin","mediaType":"application/octet-stream"})
    };
    let input = json!({"computerId":computer,"grantId":grant.grant_id,"requestId":Uuid::now_v7(),
        "transfer":transfer,
        "limits":{"maximumSeconds":30,"maximumBytes":1024,"onInterruption":"stop_computer"}});
    let initial: ComputerView = client
        .get(format!("{}/admin/computers/{computer}", left.base))
        .bearer_auth(&owner_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(initial.can_transfer_files && !initial.busy && initial.can_stop);
    let missing = rpc(
        &client,
        &left,
        &agent_token,
        "tools/call",
        json!({"name":"transfer_file","arguments":input}),
        false,
    )
    .await;
    assert!(missing.get("error").is_some());
    assert!(
        a.pending_file_transfers(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    let first = rpc(
        &client,
        &left,
        &agent_token,
        "tools/call",
        json!({"name":"transfer_file","arguments":input}),
        true,
    )
    .await;
    let id = first["result"]["taskId"]
        .as_str()
        .expect("file Task receipt");
    let pending = a.pending_file_transfers(None, 100).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert!(
        pending[0]
            .file_capability_request(&command_support::keys())
            .unwrap()
            .is_some()
    );
    ready.store(true, Ordering::SeqCst);
    let retry = client
        .post(format!("{}/admin/computers/{computer}/files", right.base))
        .bearer_auth(&agent_token)
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::ACCEPTED);
    let receipt: FileTransferView = retry.json().await.unwrap();
    assert_eq!(receipt.task_id.to_string(), id);
    assert!(receipt.can_cancel);
    let active: ComputerView = client
        .get(format!("{}/admin/computers/{computer}", left.base))
        .bearer_auth(&owner_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        active.active_execution,
        Some(ComputerExecution::File {
            task_id: receipt.task_id
        })
    );
    assert!(active.busy && active.can_stop && !active.can_transfer_files);
    db.a.client()
        .query("UPDATE ONLY $computer SET phase='stopped';")
        .bind(("computer", command_support::computer_record(computer)))
        .await
        .unwrap()
        .check()
        .unwrap();
    let stopped: ComputerView = client
        .get(format!("{}/admin/computers/{computer}", right.base))
        .bearer_auth(&owner_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(stopped.busy && !stopped.can_start && !stopped.can_transfer_files);
    db.a.client()
        .query("UPDATE ONLY $computer SET phase='ready';")
        .bind(("computer", command_support::computer_record(computer)))
        .await
        .unwrap()
        .check()
        .unwrap();

    assert!(
        a.pending_file_transfers(None, 100).await.unwrap()[0]
            .file_capability_request(&command_support::keys())
            .unwrap()
            .is_none()
    );
    let mut changed = input.clone();
    changed["transfer"]["path"] = "changed".into();
    let conflict = rpc(
        &client,
        &right,
        &agent_token,
        "tools/call",
        json!({"name":"transfer_file","arguments":changed}),
        true,
    )
    .await;
    assert_eq!(conflict["result"]["isError"], true);
    for token in [&owner_token, &agent_token] {
        let reply = rpc(
            &client,
            &right,
            token,
            "tasks/get",
            json!({"taskId":id}),
            true,
        )
        .await;
        assert!(reply.get("error").is_none(), "{reply}");
        assert!(!reply.to_string().contains("private-public-transfer-path"));
    }
    let denied = rpc(
        &client,
        &right,
        &foreign,
        "tasks/get",
        json!({"taskId":id}),
        true,
    )
    .await;
    assert!(denied.get("error").is_some());
    let wrong_parent = client
        .get(format!(
            "{}/admin/computers/{}/files/{id}",
            right.base,
            Uuid::now_v7()
        ))
        .bearer_auth(&owner_token)
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_parent.status(), StatusCode::NOT_FOUND);
    let uri = format!("computer://transfers/{id}");
    assert!(
        rpc(
            &client,
            &right,
            &owner_token,
            "resources/read",
            json!({"uri":uri}),
            true
        )
        .await
        .get("error")
        .is_some()
    );
    // Qualify projection of a verified domain receipt without claiming native I/O.
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "file-projection-worker");
    let claim = tasks
        .claim_observation(id, Duration::from_secs(60))
        .await
        .unwrap();
    let ticket = a
        .begin_file_dispatch(&claim, &command_support::keys())
        .await
        .unwrap();
    let exit = ticket
        .observe_file_result(Ok(veoveo_computer_execution::FileReceipt {
            bytes: 32,
            sha256: [7; 32],
        }))
        .unwrap();
    let completed = a
        .complete_file_result(&claim, exit, Some(artifact_id))
        .await
        .unwrap();
    let Some(FileOutcome::Completed(result)) = completed.outcome() else {
        panic!("file result")
    };
    tasks
        .transition(
            id,
            TaskTransition::Succeeded {
                message: "File transferred".into(),
                result: json!({"content":[],"structuredContent":result,"isError":false}),
            },
        )
        .await
        .unwrap();
    for token in [&owner_token, &agent_token] {
        let reply = rpc(
            &client,
            &right,
            token,
            "resources/read",
            json!({"uri":uri}),
            true,
        )
        .await;
        assert!(reply.get("error").is_none(), "{reply}");
        let returned: FileTransferResult =
            serde_json::from_str(reply["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(returned, result);
    }
    b.revoke_automation_grant(
        &owner,
        &RevokeAutomationGrantInput {
            computer_id: computer,
            grant_id: grant.grant_id,
        },
    )
    .await
    .unwrap();
    for method in ["tasks/get", "tasks/cancel"] {
        assert!(
            rpc(
                &client,
                &left,
                &agent_token,
                method,
                json!({"taskId":id}),
                true
            )
            .await
            .get("error")
            .is_some()
        );
    }
    assert!(
        rpc(
            &client,
            &left,
            &agent_token,
            "resources/read",
            json!({"uri":uri}),
            true
        )
        .await
        .get("error")
        .is_some()
    );
    assert!(
        rpc(
            &client,
            &left,
            &owner_token,
            "resources/read",
            json!({"uri":uri}),
            true
        )
        .await
        .get("error")
        .is_none()
    );
    assert_eq!(agent.owner().principal_key, completed.actor().principal_key);
    // A direct owner requires no agent grant. Cancellation uses the same Task
    // across HTTP and MCP, and a foreign parent cannot cancel it.
    let mut owned = input.clone();
    owned["grantId"] = serde_json::Value::Null;
    owned["requestId"] = Uuid::now_v7().to_string().into();
    let accepted = client
        .post(format!("{}/admin/computers/{computer}/files", right.base))
        .bearer_auth(&owner_token)
        .json(&owned)
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let owned: FileTransferView = accepted.json().await.unwrap();
    let failed = client
        .post(format!(
            "{}/admin/computers/{}/files/{}/cancel",
            left.base,
            Uuid::now_v7(),
            owned.task_id
        ))
        .bearer_auth(&owner_token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(failed.status(), StatusCode::NOT_FOUND);
    assert!(
        tasks
            .get(&owned.task_id.to_string())
            .await
            .unwrap()
            .unwrap()
            .cancel_requested_at
            .is_none()
    );
    let cancelled = client
        .post(format!(
            "{}/admin/computers/{computer}/files/{}/cancel",
            left.base, owned.task_id
        ))
        .bearer_auth(&owner_token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(cancelled.status(), StatusCode::OK);
    let cancelled: FileTransferView = cancelled.json().await.unwrap();
    assert!(cancelled.cancellation_requested_at.is_some());
    assert!(!cancelled.can_cancel);
    assert!(
        rpc(
            &client,
            &left,
            &owner_token,
            "tasks/get",
            json!({"taskId":owned.task_id}),
            true
        )
        .await
        .get("error")
        .is_none()
    );

    for value in [first, conflict, serde_json::to_value(receipt).unwrap()] {
        let text = value.to_string();
        for secret in [
            "private-public-transfer-path",
            "ciphertext",
            "bearer_token",
            "artifact_access",
        ] {
            assert!(!text.contains(secret));
        }
    }
}
