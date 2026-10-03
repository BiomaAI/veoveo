//! Native correlation, retention and admission rollback; no physical simulator work.
use super::*;
use crate::server::{
    catalog_tests::{grant, mission_request},
    task_index,
    test_support::{fixture::TestDb, identity},
};
use std::time::Duration as Timeout;
use veoveo_task_runtime::{TaskFailure, TaskStatus, TaskTransition};

async fn assert_mission_hidden(
    store: &PlatformStore,
    pilot: &GatewayInternalIdentity,
    mission: &crate::contract::MissionId,
) {
    assert!(
        task_index::mission(store, pilot, mission)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        task_index::missions_page(store, pilot, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        task_index::complete(
            store,
            pilot,
            task_index::CompletionDomain::Missions,
            mission.as_str()
        )
        .await
        .unwrap()
        .is_empty()
    );
}

#[tokio::test]
async fn native_mission_read_uses_the_admitted_task_and_retains_unresolved_identity() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = TestDb::new().await;
    tokio::time::timeout(Timeout::from_secs(90), async {
        let authority = VehicleControlAuthority::new(db.a.clone());
        let pilot = identity("task-link", "operations", "pilot", &[]);
        authority
            .grant(&pilot, grant(&pilot, "grant"))
            .await
            .unwrap();
        let plan = authority
            .prepare_plan(&pilot, mission_request("mission"))
            .await
            .unwrap();
        let first = authority
            .prepare_execution(&pilot, &plan.plan_id, 0)
            .await
            .unwrap();
        let second = authority
            .prepare_execution(&pilot, &plan.plan_id, 0)
            .await
            .unwrap();
        let (tasks, admitted) = execution_test_support::task(&authority, &pilot, &plan).await;
        // A queued request supplies no proof of physical admission.
        assert_mission_hidden(&db.b, &pilot, &plan.mission_id).await;
        let (_, guard) = authority
            .admit_execution(first, &tasks, &admitted)
            .await
            .unwrap();
        // A slower request can create its Task after another request already admitted this plan.
        let (_, rejected) = execution_test_support::task(&authority, &pilot, &plan).await;
        assert!(
            authority
                .admit_execution(second, &tasks, &rejected)
                .await
                .is_err()
        );
        assert_eq!(
            task_index::mission(&db.b, &pilot, &plan.mission_id)
                .await
                .unwrap()
                .unwrap()
                .task_id,
            admitted.task_id
        );
        for outsider in [
            identity("task-link", "elsewhere", "pilot", &[]),
            identity("foreign", "operations", "pilot", &[]),
            identity("task-link", "operations", "peer", &[]),
        ] {
            assert!(
                task_index::mission(&db.b, &outsider, &plan.mission_id)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        // A damaged pointer must not resurrect the newer rejected attempt.
        db.b.client()
            .query("UPDATE ONLY $link SET task = $wrong;")
            .bind(("link", task_link::record(admitted.task_id)))
            .bind((
                "wrong",
                veoveo_platform_store::task_record_id(rejected.task_id),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_mission_hidden(&db.b, &pilot, &plan.mission_id).await;
        db.b.client()
            .query("UPDATE ONLY $link SET task = $right;")
            .bind(("link", task_link::record(admitted.task_id)))
            .bind((
                "right",
                veoveo_platform_store::task_record_id(admitted.task_id),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        tasks
            .claim(admitted.task_id, Timeout::from_secs(60))
            .await
            .unwrap();
        let interrupted = tasks
            .transition(
                admitted.task_id,
                TaskTransition::Failed(TaskFailure::interrupted_indeterminate()),
            )
            .await
            .unwrap();
        assert!(
            !authority
                .task_retention_releasable(&interrupted)
                .await
                .unwrap()
        );
        let unadmitted = authority
            .prepare_plan(&pilot, mission_request("unadmitted"))
            .await
            .unwrap();
        // A damaged request must not redirect retention checks to another prepared plan.
        db.b.client()
            .query("UPDATE ONLY $task SET request.input.plan_id = $plan;")
            .bind((
                "task",
                veoveo_platform_store::task_record_id(admitted.task_id),
            ))
            .bind(("plan", unadmitted.plan_id.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let damaged = tasks.get(admitted.task_id).await.unwrap().unwrap();
        assert!(authority.task_retention_releasable(&damaged).await.is_err());
        db.b.client()
            .query("UPDATE ONLY $task SET request.input.plan_id = $plan;")
            .bind((
                "task",
                veoveo_platform_store::task_record_id(admitted.task_id),
            ))
            .bind(("plan", plan.plan_id.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        db.b.client()
            .query("UPDATE ONLY $task SET retention_expires_at = time::now() - 1s;")
            .bind((
                "task",
                veoveo_platform_store::task_record_id(admitted.task_id),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(tasks.prune_expired().await.unwrap().is_empty());
        assert_eq!(
            tasks.get(admitted.task_id).await.unwrap().unwrap().status,
            TaskStatus::Failed
        );
        authority
            .finish_execution(&guard, execution::Settlement::Completed)
            .await
            .unwrap();
        assert!(
            authority
                .task_retention_releasable(&interrupted)
                .await
                .unwrap()
        );
        tasks
            .acknowledge_retention_pin(admitted.task_id, &task_link::retention_pin())
            .await
            .unwrap();
        assert_eq!(tasks.prune_expired().await.unwrap(), vec![admitted.task_id]);
    })
    .await
    .expect("Task correlation qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_cancelled_task_and_link_failure_cannot_partially_admit_a_mission() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = TestDb::new().await;
    tokio::time::timeout(Timeout::from_secs(90), async {
        let authority = VehicleControlAuthority::new(db.a.clone());
        let pilot = identity("task-guard", "operations", "pilot", &[]);
        authority.grant(&pilot, grant(&pilot, "grant")).await.unwrap();
        let plan = authority.prepare_plan(&pilot, mission_request("mission")).await.unwrap();
        let draft = authority.prepare_execution(&pilot, &plan.plan_id, 0).await.unwrap();
        let (tasks, cancelled) = execution_test_support::task(&authority, &pilot, &plan).await;
        let terminal = tasks.cancel(cancelled.task_id).await.unwrap();
        assert!(matches!(authority.admit_execution(draft, &tasks, &cancelled).await, Err(ControlAuthorityError::Conflict)));
        assert!(authority.task_retention_releasable(&terminal).await.unwrap());
        assert_eq!(authority.visible_plan(&pilot, false, &plan.plan_id).await.unwrap().unwrap(), plan);
        let (tasks, task) = execution_test_support::task(&authority, &pilot, &plan).await;
        db.b.client().query("DEFINE EVENT reject_link ON TABLE uav_mission_execution WHEN $event = 'CREATE' THEN { THROW 'native link failure'; };")
            .await.unwrap().check().unwrap();
        let draft = authority.prepare_execution(&pilot, &plan.plan_id, 0).await.unwrap();
        assert!(authority.admit_execution(draft, &tasks, &task).await.is_err());
        assert_eq!(authority.visible_plan(&pilot, false, &plan.plan_id).await.unwrap().unwrap(), plan);
        assert_eq!(tasks.get(task.task_id).await.unwrap().unwrap(), task);
        let mut empty = db.b.client().query("SELECT VALUE id FROM uav_mission_execution; SELECT VALUE id FROM uav_vehicle_command_lease;")
            .await.unwrap().check().unwrap();
        assert!(empty.take::<Vec<RecordId>>(0).unwrap().is_empty());
        assert!(empty.take::<Vec<RecordId>>(1).unwrap().is_empty());
        db.b.client().query("REMOVE EVENT reject_link ON TABLE uav_mission_execution;").await.unwrap().check().unwrap();
        let draft = authority.prepare_execution(&pilot, &plan.plan_id, 0).await.unwrap();
        let (_, guard) = authority.admit_execution(draft, &tasks, &task).await.unwrap();
        assert_eq!(guard.task_id(), task.task_id);
        authority.abort_execution(&guard).await.unwrap();
    }).await.expect("Task admission rollback qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_pin_reconciliation_filters_before_its_page_limit() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = TestDb::new().await;
    tokio::time::timeout(Timeout::from_secs(120), async {
        let authority = VehicleControlAuthority::new(db.a.clone());
        let pilot = identity("pin-pages", "operations", "pilot", &[]);
        authority
            .grant(&pilot, grant(&pilot, "grant"))
            .await
            .unwrap();
        let active = authority
            .prepare_plan(&pilot, mission_request("active"))
            .await
            .unwrap();
        let (_, _guard) = execution_test_support::begin(&authority, &pilot, &active.plan_id, 0)
            .await
            .unwrap();
        let prepared = authority
            .prepare_plan(&pilot, mission_request("unadmitted"))
            .await
            .unwrap();
        let mut expected = Vec::new();
        for i in 0..211 {
            let plan = if i < 110 { &active } else { &prepared };
            let (tasks, task) = execution_test_support::task(&authority, &pilot, plan).await;
            tasks
                .claim(task.task_id, Timeout::from_secs(30))
                .await
                .unwrap();
            tasks
                .transition(
                    task.task_id,
                    TaskTransition::Failed(TaskFailure::interrupted_indeterminate()),
                )
                .await
                .unwrap();
            if i >= 110 {
                expected.push(task.task_id);
            }
        }
        let before = Utc::now();
        let first = authority.settled_task_ids(None, before).await.unwrap();
        assert_eq!(first.len(), 100);
        let second = authority
            .settled_task_ids(first.last().copied(), before)
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        assert!(
            authority
                .settled_task_ids(second.last().copied(), before)
                .await
                .unwrap()
                .is_empty()
        );
        let actual: BTreeSet<_> = first.into_iter().chain(second).collect();
        assert_eq!(actual, expected.into_iter().collect());
    })
    .await
    .expect("retention page qualification exceeded 120 seconds");
}
