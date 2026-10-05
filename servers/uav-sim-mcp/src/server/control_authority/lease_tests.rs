//! Native memory/RocksDB state transitions; no simulator or GPU dispatch.
use super::*;
use crate::server::{
    catalog_tests::{grant, mission_request},
    test_support::{fixture::StoreBackend, identity},
};
use std::time::Duration as Timeout;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(super) struct LeaseRecord {
    id: RecordId,
    tenant: RecordId,
    work_context: RecordId,
    session_id: String,
    vehicle_id: String,
    principal_key: String,
    mission_id: String,
    lease_token: String,
    expires_at: DateTime<Utc>,
    released_at: Option<DateTime<Utc>>,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn lease(
    authority: &VehicleControlAuthority,
    pilot: &GatewayInternalIdentity,
    plan: &VehicleMissionPlan,
) -> LeaseRecord {
    select_only(
        &authority.store,
        vehicle_lease_record_id(pilot, &plan.session_id, &plan.vehicle_id),
        "lease",
    )
    .await
    .unwrap()
}

fn uses_vehicle_index(node: &serde_json::Value) -> bool {
    (node["operator"] == "IndexScan"
        && node["attributes"]["index"] == "uav_mission_plan_vehicle_state")
        || node["children"]
            .as_array()
            .is_some_and(|children| children.iter().any(uses_vehicle_index))
}

#[tokio::test]
async fn native_two_replicas_admit_exactly_one_plan_per_vehicle() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(StoreBackend::RocksDb).await;
    tokio::time::timeout(Timeout::from_secs(120), async {
        let first = VehicleControlAuthority::new(db.a.clone());
        let second = VehicleControlAuthority::new(db.b.clone());
        let pilot = identity("lease-race", "operations", "pilot", &[]);
        first
            .grant(&pilot, grant(&pilot, "matching"))
            .await
            .unwrap();
        let peer = identity("lease-race", "operations", "peer", &[]);
        second
            .grant(&peer, grant(&peer, "peer-grant"))
            .await
            .unwrap();
        let mut previous_revision = None;
        let mut previous_guard = None;
        for iteration in 0..12 {
            let a = first
                .prepare_plan(&pilot, mission_request(&format!("first-{iteration}")))
                .await
                .unwrap();
            let b = second
                .prepare_plan(&peer, mission_request(&format!("second-{iteration}")))
                .await
                .unwrap();
            let a_draft = first
                .prepare_execution(&pilot, &a.plan_id, 0)
                .await
                .unwrap();
            let b_draft = second
                .prepare_execution(&peer, &b.plan_id, 0)
                .await
                .unwrap();
            let (a_tasks, a_task) = execution_test_support::task(&first, &pilot, &a).await;
            let (b_tasks, b_task) = execution_test_support::task(&second, &peer, &b).await;
            let (left, right) = tokio::join!(
                first.admit_execution(a_draft, &a_tasks, &a_task),
                second.admit_execution(b_draft, &b_tasks, &b_task)
            );
            assert_ne!(
                left.is_ok(),
                right.is_ok(),
                "one vehicle admits exactly one contender: {left:?}, {right:?}"
            );
            let (executing, guard, loser, loser_owner) = match (left, right) {
                (Ok((plan, guard)), Err(_)) => (plan, guard, b, &peer),
                (Err(_), Ok((plan, guard))) => (plan, guard, a, &pilot),
                _ => unreachable!(),
            };
            let active = lease(&first, &pilot, &executing).await;
            if iteration == 0 {
                let (tenant, context) = context_records(&pilot).unwrap();
                let mut response =
                    db.b.client()
                        .query(include_str!("queries/tests/executing_plans_explain.surql"))
                        .bind(("tenant", tenant))
                        .bind(("work_context", context))
                        .bind(("simulation_session", executing.session_id.to_string()))
                        .bind(("vehicle", executing.vehicle_id.to_string()))
                        .await
                        .unwrap()
                        .check()
                        .unwrap();
                let explain = response
                    .take::<surrealdb::types::Value>(0)
                    .unwrap()
                    .into_json_value();
                assert!(uses_vehicle_index(&explain), "{explain}");
            }
            if let Some(previous) = previous_revision {
                assert!(active.revision > previous);
            }
            if let Some(old) = previous_guard.as_ref() {
                assert!(
                    first
                        .finish_execution(old, execution::Settlement::Completed)
                        .await
                        .is_err()
                );
                assert_eq!(
                    lease(&first, &pilot, &executing).await,
                    active,
                    "stale finalization must preserve the new lease"
                );
            }
            assert_eq!(
                first
                    .visible_plan(loser_owner, false, &loser.plan_id)
                    .await
                    .unwrap()
                    .unwrap(),
                loser
            );
            assert!(matches!(
                execution_test_support::begin(&second, loser_owner, &loser.plan_id, 0).await,
                Err(ControlAuthorityError::VehicleBusy(_))
            ));
            assert_eq!(
                lease(&first, &pilot, &executing).await,
                active,
                "busy admission must roll back its lease write"
            );
            first
                .finish_execution(&guard, execution::Settlement::Completed)
                .await
                .unwrap();
            let released = lease(&first, &pilot, &executing).await;
            assert!(released.released_at.is_some());
            assert!(released.revision > active.revision);
            first
                .finish_execution(&guard, execution::Settlement::Completed)
                .await
                .unwrap();
            assert_eq!(
                lease(&first, &pilot, &executing).await,
                released,
                "duplicate settlement is a no-op"
            );
            previous_revision = Some(released.revision);
            previous_guard = Some(guard);
        }
    })
    .await
    .expect("RocksDB vehicle contention exceeded 120 seconds");
}

#[tokio::test]
async fn native_expiry_does_not_release_executing_vehicle_authority() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Timeout::from_secs(60), async {
        let authority = VehicleControlAuthority::new(db.a.clone());
        let pilot = identity("expiry", "operations", "pilot", &[]);
        authority
            .grant(&pilot, grant(&pilot, "matching"))
            .await
            .unwrap();
        let a = authority
            .prepare_plan(&pilot, mission_request("first"))
            .await
            .unwrap();
        let b = authority
            .prepare_plan(&pilot, mission_request("second"))
            .await
            .unwrap();
        let (_, guard) = execution_test_support::begin(&authority, &pilot, &a.plan_id, 0)
            .await
            .unwrap();
        let lease_id = vehicle_lease_record_id(&pilot, &a.session_id, &a.vehicle_id);
        db.b.client()
            .query(include_str!("queries/tests/expire_lease.surql"))
            .bind(("lease", lease_id))
            .await
            .unwrap()
            .check()
            .unwrap();
        let expired = lease(&authority, &pilot, &a).await;
        assert!(matches!(
            execution_test_support::begin(&authority, &pilot, &b.plan_id, 0).await,
            Err(ControlAuthorityError::VehicleBusy(_))
        ));
        assert_eq!(lease(&authority, &pilot, &a).await, expired);
        authority
            .finish_execution(&guard, execution::Settlement::NotDispatched)
            .await
            .unwrap();
        let (_, next) = execution_test_support::begin(&authority, &pilot, &b.plan_id, 0)
            .await
            .unwrap();
        authority
            .finish_execution(&next, execution::Settlement::Completed)
            .await
            .unwrap();
    })
    .await
    .expect("expired executing lease qualification exceeded 60 seconds");
}

#[tokio::test]
async fn native_plan_and_lease_writes_roll_back_together() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Timeout::from_secs(60), async {
        let authority = VehicleControlAuthority::new(db.a.clone());
        let pilot = identity("rollback", "operations", "pilot", &[]);
        authority
            .grant(&pilot, grant(&pilot, "matching"))
            .await
            .unwrap();
        let prepared = authority
            .prepare_plan(&pilot, mission_request("rollback"))
            .await
            .unwrap();
        db.b.client()
            .query(include_str!("queries/tests/reject_admission.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            execution_test_support::begin(&authority, &pilot, &prepared.plan_id, 0)
                .await
                .is_err()
        );
        assert_eq!(
            authority
                .visible_plan(&pilot, false, &prepared.plan_id)
                .await
                .unwrap()
                .unwrap(),
            prepared
        );
        let mut response =
            db.b.client()
                .query(include_str!("queries/tests/lease_record_ids.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
        assert!(response.take::<Vec<RecordId>>(0).unwrap().is_empty());
        db.b.client()
            .query(include_str!("queries/tests/remove_admission_event.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        let (executing, guard) =
            execution_test_support::begin(&authority, &pilot, &prepared.plan_id, 0)
                .await
                .unwrap();
        let active = lease(&authority, &pilot, &executing).await;
        db.b.client()
            .query(include_str!("queries/tests/reject_settlement.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            authority
                .finish_execution(&guard, execution::Settlement::Completed)
                .await
                .is_err()
        );
        assert_eq!(lease(&authority, &pilot, &executing).await, active);
        assert_eq!(
            authority
                .visible_plan(&pilot, false, &executing.plan_id)
                .await
                .unwrap()
                .unwrap(),
            executing
        );
        db.b.client()
            .query(include_str!("queries/tests/remove_settlement_event.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        authority
            .finish_execution(&guard, execution::Settlement::Completed)
            .await
            .unwrap();
    })
    .await
    .expect("transaction rollback qualification exceeded 60 seconds");
}

#[tokio::test]
async fn native_admission_rechecks_all_plan_metadata_and_revision_exhaustion() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Timeout::from_secs(60), async {
        let authority = VehicleControlAuthority::new(db.a.clone());
        let pilot = identity("guards", "operations", "pilot", &[]);
        authority
            .grant(&pilot, grant(&pilot, "matching"))
            .await
            .unwrap();
        let a = authority
            .prepare_plan(&pilot, mission_request("first"))
            .await
            .unwrap();
        let (_, guard) = execution_test_support::begin(&authority, &pilot, &a.plan_id, 0)
            .await
            .unwrap();
        authority
            .finish_execution(&guard, execution::Settlement::Completed)
            .await
            .unwrap();
        let b = authority
            .prepare_plan(&pilot, mission_request("second"))
            .await
            .unwrap();
        let draft = authority
            .prepare_execution(&pilot, &b.plan_id, 0)
            .await
            .unwrap();
        let before = lease(&authority, &pilot, &a).await;
        let plan_id = scoped_record_id("uav_vehicle_mission_plan", &pilot, b.plan_id.as_str());
        db.b.client()
            .query(include_str!("queries/tests/change_plan_vehicle.surql"))
            .bind(("record", plan_id.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let (tasks, task) = execution_test_support::task(&authority, &pilot, draft.plan()).await;
        assert!(matches!(
            authority.admit_execution(draft, &tasks, &task).await,
            Err(ControlAuthorityError::Conflict)
        ));
        assert_eq!(lease(&authority, &pilot, &a).await, before);
        db.b.client()
            .query(include_str!("queries/tests/restore_plan_vehicle.surql"))
            .bind(("record", plan_id))
            .bind(("vehicle", b.vehicle_id.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        db.b.client()
            .query(include_str!("queries/tests/set_plan_revision.surql"))
            .bind(("record", before.id.clone()))
            .bind(("revision", i64::MAX))
            .await
            .unwrap()
            .check()
            .unwrap();
        let exhausted = lease(&authority, &pilot, &a).await;
        assert!(matches!(
            execution_test_support::begin(&authority, &pilot, &b.plan_id, 0).await,
            Err(ControlAuthorityError::Conflict)
        ));
        assert_eq!(lease(&authority, &pilot, &a).await, exhausted);
        assert_eq!(
            authority
                .visible_plan(&pilot, false, &b.plan_id)
                .await
                .unwrap()
                .unwrap(),
            b
        );
    })
    .await
    .expect("admission guard qualification exceeded 60 seconds");
}
