//! Granted public discovery and lifecycle retain original Task and owner recovery.
use crate::{Server, Signing, client, command_support, rpc, support, template};
use serde_json::json;
use std::time::Instant;
use uuid::Uuid;
use veoveo_computers::api::*;
use veoveo_computers_mcp::{Application, CapacityHealth, NamedTemplate, RuntimeAccess, Templates};
use veoveo_task_runtime::TaskRuntime;

#[tokio::test]
async fn granted_collection_receives_revocation_and_exact_listener_loses_authority() {
    use rmcp::model::{ServerNotification, SubscriptionFilter};
    use std::time::Duration;
    let db = support::TestDb::new().await;
    let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = store
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let signing = Signing::new();
    let (_health, receiver) = tokio::sync::watch::channel(CapacityHealth {
        availability: CapacityAvailability::ComputeUnavailable,
        observed_at: Instant::now(),
    });
    let server = Server::new(
        Application::new(
            store.clone(),
            TaskRuntime::new(db.a.clone(), "computers", "granted-listener"),
            Templates::new(vec![], None).unwrap(),
            receiver,
            RuntimeAccess::unavailable(),
        )
        .unwrap(),
        &signing,
    )
    .await;
    let token = signing.identity(
        support::identity(agent.owner()),
        "computers",
        chrono::Utc::now() + chrono::TimeDelta::minutes(2),
    );
    let peer = crate::sdk(&server, token).await;
    let uri = format!("computer://computers/{computer}");
    let mut collection = peer
        .listen(
            SubscriptionFilter::builder()
                .resource_subscription("computer://computers")
                .build(),
        )
        .await
        .unwrap();
    let mut exact = peer
        .listen(
            SubscriptionFilter::builder()
                .resource_subscription(uri.clone())
                .build(),
        )
        .await
        .unwrap();
    for stream in [&mut collection, &mut exact] {
        let first = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(
            first,
            ServerNotification::ResourceUpdatedNotification(_)
        ));
    }
    // More than one authority window must remain usable without a new listener.
    assert!(
        tokio::time::timeout(Duration::from_secs(6), exact.next())
            .await
            .is_err()
    );
    store
        .revoke_automation_grant(
            &owner,
            &RevokeAutomationGrantInput {
                computer_id: computer,
                grant_id: grant.grant_id,
            },
        )
        .await
        .unwrap();
    let changed = tokio::time::timeout(Duration::from_secs(5), collection.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(changed, ServerNotification::ResourceUpdatedNotification(ref update) if update.params.uri == "computer://computers")
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Ok(Some(_)) = exact.next().await {}
    })
    .await
    .expect("revoked exact listener stayed open");
    collection.cancel().await.unwrap();
    peer.cancel().await.unwrap();
}

#[tokio::test]
async fn named_start_and_stop_share_public_discovery_retry_and_owner_recovery() {
    for action in [Action::Start, Action::Stop] {
        let db = support::TestDb::new().await;
        let (a, b, owner, agent, computer) = support::automation::setup(&db).await;
        let selected = template::retained_template(format!(
            "fixture.invalid/computer@sha256:{}",
            "f".repeat(64)
        ));
        db.a.client()
            .query("UPDATE ONLY $computer SET template_fingerprint = $fingerprint, phase = $phase;")
            .bind(("computer", command_support::computer_record(computer)))
            .bind(("fingerprint", selected.fingerprint()))
            .bind((
                "phase",
                if action == Action::Start {
                    "stopped"
                } else {
                    "ready"
                },
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        let (health, receiver) = tokio::sync::watch::channel(CapacityHealth {
            availability: CapacityAvailability::Available,
            observed_at: Instant::now(),
        });
        let app = |store, platform| {
            Application::new(
                store,
                TaskRuntime::new(platform, "computers", "named-lifecycle"),
                Templates::new(
                    vec![NamedTemplate::new("development".into(), selected.clone()).unwrap()],
                    Some(selected.fingerprint()),
                )
                .unwrap(),
                receiver.clone(),
                RuntimeAccess::unavailable(),
            )
            .unwrap()
        };
        let signing = Signing::new();
        let left = Server::new(app(a.clone(), db.a.clone()), &signing).await;
        let right = Server::new(app(b, db.b.clone()), &signing).await;
        let expires = chrono::Utc::now() + chrono::TimeDelta::minutes(2);
        let owner_token = signing.identity(support::identity(owner.owner()), "computers", expires);
        let agent_token = signing.identity(support::identity(agent.owner()), "computers", expires);
        let client = client();
        let mut grant_input = support::automation::input(computer);
        grant_input.permissions = [
            AutomationPermission::Read,
            AutomationPermission::Start,
            AutomationPermission::Stop,
        ]
        .into();
        grant_input.execution_limits = None;
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
        let uri = format!("computer://computers/{computer}");
        let resource = rpc(
            &client,
            &right,
            &agent_token,
            "resources/read",
            json!({"uri":uri}),
            false,
        )
        .await;
        let view: ComputerView =
            serde_json::from_str(resource["result"]["contents"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(view.access_mode, ComputerAccessMode::Granted);
        assert_eq!(view.granted_access[0].grant_id, grant.grant.grant_id);
        assert!(
            !view.can_connect && !view.can_delete && !view.can_create && !view.can_transfer_files
        );
        assert_eq!(view.can_start, action == Action::Start);
        assert_eq!(view.can_stop, action == Action::Stop);
        let collection = client
            .get(format!("{}/admin/computers", right.base))
            .bearer_auth(&agent_token)
            .send()
            .await
            .unwrap();
        assert_eq!(collection.status(), 200);
        let snapshot: ComputerSnapshot = collection.json().await.unwrap();
        assert_eq!(snapshot.computers.len(), 1);
        assert_eq!(snapshot.computers[0].computer_id, computer);
        let name = if action == Action::Start {
            "start"
        } else {
            "stop"
        };
        let input = json!({"computerId":computer,"requestId":Uuid::now_v7(),"grantId":grant.grant.grant_id});
        let first = rpc(
            &client,
            &left,
            &agent_token,
            "tools/call",
            json!({"name":name,"arguments":input}),
            true,
        )
        .await;
        let task = first["result"]["taskId"]
            .as_str()
            .expect("named lifecycle Task");
        let stored = TaskRuntime::new(db.a.clone(), "computers", "inspect")
            .get(task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.owner, *agent.owner());
        health
            .send(CapacityHealth {
                availability: CapacityAvailability::ComputeUnavailable,
                observed_at: Instant::now(),
            })
            .unwrap();
        let retry = client
            .post(format!("{}/admin/computers/{computer}/{name}", right.base))
            .bearer_auth(&agent_token)
            .json(&json!({"requestId":input["requestId"],"grantId":grant.grant.grant_id}))
            .send()
            .await
            .unwrap();
        assert_eq!(retry.status(), 202);
        assert_eq!(
            retry
                .json::<OperationReceipt>()
                .await
                .unwrap()
                .task_id
                .to_string(),
            task
        );
        let mut ungranted = input.clone();
        ungranted.as_object_mut().unwrap().remove("grantId");
        let denied = rpc(
            &client,
            &left,
            &agent_token,
            "tools/call",
            json!({"name":name,"arguments":ungranted}),
            true,
        )
        .await;
        assert_eq!(denied["result"]["isError"], true);
        for token in [&owner_token, &agent_token] {
            let read = rpc(
                &client,
                &right,
                token,
                "tasks/get",
                json!({"taskId":task}),
                true,
            )
            .await;
            assert!(read.get("error").is_none(), "{read}");
        }
        a.revoke_automation_grant(
            &owner,
            &RevokeAutomationGrantInput {
                computer_id: computer,
                grant_id: grant.grant.grant_id,
            },
        )
        .await
        .unwrap();
        let retry = rpc(
            &client,
            &left,
            &agent_token,
            "tools/call",
            json!({"name":name,"arguments":input}),
            true,
        )
        .await;
        assert!(retry.get("error").is_some(), "{retry}");
        let read = rpc(
            &client,
            &right,
            &agent_token,
            "resources/read",
            json!({"uri":uri}),
            false,
        )
        .await;
        assert!(read.get("error").is_some());
        for method in ["tasks/get", "tasks/cancel"] {
            let denied = rpc(
                &client,
                &right,
                &agent_token,
                method,
                json!({"taskId":task}),
                true,
            )
            .await;
            assert!(denied.get("error").is_some());
            let recovered = rpc(
                &client,
                &left,
                &owner_token,
                method,
                json!({"taskId":task}),
                true,
            )
            .await;
            assert!(recovered.get("error").is_none(), "{recovered}");
        }
        let stored = TaskRuntime::new(db.a.clone(), "computers", "inspect")
            .get(task)
            .await
            .unwrap()
            .unwrap();
        assert!(stored.cancel_requested_at.is_some());
        assert_eq!(stored.owner, *agent.owner());
    }
}
