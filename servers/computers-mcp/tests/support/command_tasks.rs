//! Public Tasks read/cancel uses current domain authority across service replicas.
use crate::{Server, Signing, client, command_support, rpc, sdk, support};
use rmcp::model::{ServerNotification, SubscriptionFilter};
use serde_json::json;
use std::time::{Duration, Instant};
use veoveo_computers::{ComputersStore, api::*};
use veoveo_computers_mcp::{Application, CapacityHealth, RuntimeAccess, Templates};
use veoveo_task_runtime::TaskRuntime;

#[tokio::test]
async fn command_task_reads_and_cancellation_preserve_actual_actor_and_owner_authority() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let claim = command_support::queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    let id = claim.snapshot.task_id.to_string();
    let signing = Signing::new();
    let (health, receiver) = tokio::sync::watch::channel(CapacityHealth {
        availability: CapacityAvailability::ComputeUnavailable,
        observed_at: Instant::now(),
    });
    let app = |store: ComputersStore, platform| {
        Application::new(
            store,
            TaskRuntime::new(platform, "computers", "task-wire"),
            Templates::new(vec![], None).unwrap(),
            receiver.clone(),
            RuntimeAccess::unavailable(),
        )
        .unwrap()
    };
    let left = Server::new(app(a.clone(), db.a.clone()), &signing).await;
    let right = Server::new(app(b, db.b.clone()), &signing).await;
    let expires = chrono::Utc::now() + chrono::TimeDelta::minutes(2);
    let actor_token = signing.identity(support::identity(agent.owner()), "computers", expires);
    let owner_token = signing.identity(support::identity(owner.owner()), "computers", expires);
    let outsider = signing.bearer("bob", "computers");
    let client = client();
    for server in [&left, &right] {
        for token in [&actor_token, &owner_token] {
            let response = rpc(
                &client,
                server,
                token,
                "tasks/get",
                json!({"taskId":id}),
                true,
            )
            .await;
            assert!(response.get("error").is_none(), "{response}");
            let output = response.to_string();
            for private in [
                "private-dispatch-argument",
                "private-command-environment-fixture",
                "private-command-stdin-fixture",
                "ciphertext",
            ] {
                assert!(!output.contains(private));
            }
        }
        assert!(
            rpc(
                &client,
                server,
                &outsider,
                "tasks/get",
                json!({"taskId":id}),
                true
            )
            .await
            .get("error")
            .is_some()
        );
    }
    let agent_peer = sdk(&left, actor_token.clone()).await;
    let owner_peer = sdk(&right, owner_token.clone()).await;
    let filter = SubscriptionFilter::builder().task_id(id.clone()).build();
    let mut agent_updates = agent_peer.listen(filter.clone()).await.unwrap();
    let mut owner_updates = owner_peer.listen(filter).await.unwrap();
    for stream in [&mut agent_updates, &mut owner_updates] {
        let baseline = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let ServerNotification::TaskStatusNotification(update) = baseline else {
            panic!("missing Task baseline");
        };
        assert_eq!(update.params.task.task.task_id, id);
    }
    TaskRuntime::new(db.a.clone(), "computers", &claim.lease_owner)
        .transition(
            &id,
            veoveo_task_runtime::TaskTransition::Waiting {
                message: "Waiting for command preparation".into(),
                progress: 0.0,
            },
        )
        .await
        .unwrap();
    for stream in [&mut agent_updates, &mut owner_updates] {
        let next = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let ServerNotification::TaskStatusNotification(update) = next else {
            panic!("missing Task update");
        };
        assert_eq!(update.params.task.task.task_id, id);
    }
    a.revoke_automation_grant(
        &owner,
        &RevokeAutomationGrantInput {
            computer_id: computer,
            grant_id: grant.grant_id,
        },
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(8), async {
        while let Ok(Some(_)) = agent_updates.next().await {}
    })
    .await
    .expect("revoked command subscription remained open");
    assert!(
        rpc(
            &client,
            &right,
            &actor_token,
            "tasks/get",
            json!({"taskId":id}),
            true
        )
        .await
        .get("error")
        .is_some()
    );
    let cancelled = rpc(
        &client,
        &right,
        &owner_token,
        "tasks/cancel",
        json!({"taskId":id}),
        true,
    )
    .await;
    assert!(cancelled.get("error").is_none(), "{cancelled}");
    let snapshot = TaskRuntime::new(db.a.clone(), "computers", "inspect")
        .get(&id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.owner, *agent.owner());
    assert!(snapshot.cancel_requested_at.is_some());
    let update = tokio::time::timeout(Duration::from_secs(5), owner_updates.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        update,
        ServerNotification::TaskStatusNotification(_)
    ));
    owner_updates.cancel().await.unwrap();
    agent_peer.cancel().await.unwrap();
    owner_peer.cancel().await.unwrap();
    drop(health);
}
