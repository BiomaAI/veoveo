//! Real-store restart recovery preserves run fences and invalidates old access.
mod support;
use std::time::{Duration, Instant};
use uuid::Uuid;
use veoveo_computers::{Computer, ComputerActor, ComputerError, ReachedPhase, ReachedState};
use veoveo_platform_store::RecordId;

fn observed(before: &Computer, process: &str) -> ReachedState {
    ReachedState {
        provider_instance_id: before.provider_instance_id,
        computer_id: before.computer_id,
        replacement_instance_id: before.replacement_instance_id,
        template_fingerprint: before.template_fingerprint.clone(),
        resource_id: before.provider_resource_id.clone().unwrap(),
        process_id: process.into(),
        phase: ReachedPhase::Ready,
    }
}
fn record(table: &str, id: Uuid) -> RecordId {
    RecordId::new(table, surrealdb::types::Uuid::from(id))
}
#[tokio::test]
async fn host_restart_preserves_resource_and_owner_and_rejects_old_grants_and_observations() {
    let db = support::TestDb::new().await;
    let actor =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
    let (store, replica, id) = support::interactive::ready(&db, &actor).await;
    let before = store.get(actor.owner(), id).await.unwrap();
    let grant = store.issue_browser_grant(&actor, id).await.unwrap();
    let handle = store
        .redeem_browser_grant(&actor, &grant.token)
        .await
        .unwrap();
    store
        .record_observed_restart(&before, observed(&before, "restarted-run"), Instant::now())
        .await
        .unwrap();
    let after = replica.get(actor.owner(), id).await.unwrap();
    assert_eq!(after.owner, before.owner);
    assert_eq!(after.provider_resource_id, before.provider_resource_id);
    assert_eq!(after.template_fingerprint, before.template_fingerprint);
    assert_eq!(
        after.replacement_instance_id,
        before.replacement_instance_id
    );
    assert_eq!(after.process_id.as_deref(), Some("restarted-run"));
    assert!(store.renew_browser_grant(&handle, false).await.is_err());
    assert!(matches!(
        replica
            .record_observed_restart(&before, observed(&before, "stale-run"), Instant::now())
            .await,
        Err(ComputerError::StateConflict)
    ));
    let fresh = replica.issue_browser_grant(&actor, id).await.unwrap();
    let fresh = replica
        .redeem_browser_grant(&actor, &fresh.token)
        .await
        .unwrap();
    assert_eq!(
        replica
            .renew_browser_grant(&fresh, false)
            .await
            .unwrap()
            .computer()
            .process_id,
        after.process_id
    );
    let mut q =
        db.a.client()
            .query("SELECT * FROM outbox_event WHERE event_type = 'computer.run_observed';")
            .await
            .unwrap()
            .check()
            .unwrap();
    let events: Vec<surrealdb::types::Value> = q.take(0).unwrap();
    assert_eq!(events.len(), 1);
}
#[tokio::test]
async fn restart_observation_cannot_replace_identity_or_cross_an_operation_fence() {
    let db = support::TestDb::new().await;
    let actor =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
    let (store, _, id) = support::interactive::ready(&db, &actor).await;
    let before = store.get(actor.owner(), id).await.unwrap();
    for change in 0..6 {
        let mut seen = observed(&before, "new-run");
        match change {
            0 => seen.provider_instance_id = Uuid::now_v7(),
            1 => seen.resource_id = "another-resource".into(),
            2 => seen.replacement_instance_id = Some(Uuid::now_v7()),
            3 => seen.template_fingerprint = "0".repeat(64),
            4 => seen.phase = ReachedPhase::Stopped,
            _ => seen.computer_id = Uuid::now_v7(),
        }
        assert!(matches!(
            store
                .record_observed_restart(&before, seen, Instant::now())
                .await,
            Err(ComputerError::StateConflict)
        ));
    }
    assert!(
        store
            .record_observed_restart(
                &before,
                observed(&before, "new-run"),
                Instant::now() - Duration::from_secs(11)
            )
            .await
            .is_err()
    );
    assert!(
        store
            .record_observed_restart(
                &before,
                observed(&before, "new-run"),
                Instant::now() + Duration::from_secs(1)
            )
            .await
            .is_err()
    );
    db.a.client()
        .query("UPDATE ONLY $computer SET active_operation = $op;")
        .bind(("computer", record("computer", id)))
        .bind(("op", Uuid::now_v7()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        store
            .record_observed_restart(&before, observed(&before, "new-run"), Instant::now())
            .await,
        Err(ComputerError::OperationBusy)
    ));
    assert_eq!(
        store.get(actor.owner(), id).await.unwrap().process_id,
        before.process_id
    );
}
#[tokio::test]
async fn restart_observation_preserves_unresolved_command_or_file_slot() {
    let db = support::TestDb::new().await;
    let actor =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
    let (store, _, id) = support::interactive::ready(&db, &actor).await;
    let before = store.get(actor.owner(), id).await.unwrap();
    db.a.client()
        .query("CREATE $slot CONTENT {computer_id:$id, execution:$execution};")
        .bind(("slot", record("computer_execution_slot", id)))
        .bind(("id", id))
        .bind(("execution", record("computer_execution", Uuid::now_v7())))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        store
            .record_observed_restart(&before, observed(&before, "new-run"), Instant::now())
            .await,
        Err(ComputerError::OperationBusy)
    ));
    assert_eq!(store.get(actor.owner(), id).await.unwrap(), before);
}
