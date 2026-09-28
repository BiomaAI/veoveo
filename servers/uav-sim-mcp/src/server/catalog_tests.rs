//! Native catalog qualification: disposable SurrealDB, no simulator or GPU workload.
use super::{
    control_authority::{ControlCollection, VehicleControlAuthority, task_link},
    ownership::runtime_owner,
    task_index,
    test_support::identity,
};
use crate::{
    adapter::{Adapter, HttpAdapter},
    contract::*,
    uris,
};
use chrono::{TimeDelta, Utc};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use veoveo_map_mcp::contract::{
    MAP_ROUTE_HANDOFF_SCHEMA, MapMobilityProfileUri, MapRouteHandoff, MobilityProfileId,
    MobilityProfileVersion, RouteStatus, ValidationId, Wgs84Position as MapPosition,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, SubscriptionHub};
use veoveo_platform_store::PlatformStore;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskRuntime};
use veoveo_types::TaskId;

use crate::server::test_support::fixture;

pub(super) fn grant(identity: &GatewayInternalIdentity, key: &str) -> GrantVehicleControlRequest {
    GrantVehicleControlRequest {
        grant_id: ControlGrantId::new(key).unwrap(),
        session_id: SessionId::new("native-session").unwrap(),
        vehicle_id: VehicleId::new("vehicle-one").unwrap(),
        principal_key: identity.actor.id.to_string(),
        permissions: BTreeSet::from([
            VehicleControlPermission::Inspect,
            VehicleControlPermission::Plan,
            VehicleControlPermission::Execute,
        ]),
        map_mobility_profile_uri: MapMobilityProfileUri::new(
            MobilityProfileId::from_stable_key(b"native"),
            MobilityProfileVersion::FIRST,
        ),
        allow_planning_advisory: false,
        valid_from: Utc::now() - TimeDelta::hours(2),
        valid_until: None,
    }
}

pub(super) fn mission_request(key: &str) -> PrepareVehicleMissionRequest {
    let now = Utc::now();
    PrepareVehicleMissionRequest {
        session_id: SessionId::new("native-session").unwrap(),
        mission_id: MissionId::new(key).unwrap(),
        vehicle_id: VehicleId::new("vehicle-one").unwrap(),
        expected_world_revision_uri: veoveo_frames_mcp::contract::FrameWorldRevisionUri::new(
            &veoveo_frames_mcp::contract::FrameWorldId::new("native").unwrap(),
            &veoveo_frames_mcp::contract::FrameWorldRevisionId::new("revision-one").unwrap(),
        ),
        map_route: MapRouteHandoff {
            schema_profile: MAP_ROUTE_HANDOFF_SCHEMA.into(),
            route_uri: "map://route/native".into(),
            route_digest_sha256: "a".repeat(64),
            route_status: RouteStatus::Validated,
            mobility_profile_uri: MapMobilityProfileUri::new(
                MobilityProfileId::from_stable_key(b"native"),
                MobilityProfileVersion::FIRST,
            ),
            path: vec![
                MapPosition {
                    longitude_deg: -74.0,
                    latitude_deg: 40.0,
                    ellipsoidal_height_m: Some(100.0),
                },
                MapPosition {
                    longitude_deg: -74.1,
                    latitude_deg: 40.1,
                    ellipsoidal_height_m: Some(120.0),
                },
            ],
            validation_id: ValidationId::new(),
            validated_at: now,
            operational_snapshot_id: "snapshot-native".into(),
            base_release_ids: vec!["release-native".into()],
            restriction_ids: vec![],
            prepared_at: now,
        },
        speed_mps: 5.0,
        hold_seconds_at_destination: 0.0,
    }
}

fn server(store: &PlatformStore) -> super::service::UavSimMcp {
    let adapter = Arc::new(Adapter::Http(Box::new(
        HttpAdapter::new(
            "http://127.0.0.1:1/".parse().unwrap(),
            Duration::from_millis(50),
            Duration::from_millis(50),
            "native-fixture".into(),
            store.clone(),
            "uav-index",
        )
        .unwrap(),
    )));
    super::service::UavSimMcp::new(super::test_support::state(store, adapter, "discovery-test"))
}

async fn task(
    tasks: &TaskRuntime,
    identity: &GatewayInternalIdentity,
    plan: &VehicleMissionPlan,
) -> veoveo_task_runtime::TaskSnapshot {
    tasks
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: runtime_owner(identity),
            server: "uav-sim".into(),
            task_type: "execute_vehicle_mission_plan".into(),
            request: serde_json::to_value(ExecuteVehicleMissionPlanRequest {
                plan_id: plan.plan_id.clone(),
                expected_revision: plan.revision,
            })
            .unwrap(),
            recovery_class: RecoveryClass::InterruptedIndeterminate,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::from([task_link::retention_pin()]),
        })
        .await
        .unwrap()
        .snapshot
}

async fn admitted_task(
    tasks: &TaskRuntime,
    identity: &GatewayInternalIdentity,
    plan: &VehicleMissionPlan,
) -> veoveo_task_runtime::TaskSnapshot {
    let authority = VehicleControlAuthority::new(tasks.platform_store().clone());
    let draft = authority
        .prepare_execution(identity, &plan.plan_id, plan.revision)
        .await
        .unwrap();
    let task = task(tasks, identity, plan).await;
    let (_, guard) = authority
        .admit_execution(draft, tasks, &task)
        .await
        .unwrap();
    // The catalog fixture never dispatches physical work. Settle admission before
    // creating another mission for the same vehicle.
    authority.abort_execution(&guard).await.unwrap();
    task
}

#[tokio::test]
async fn native_sql_pages_and_lookups_preserve_authority_beyond_previous_caps() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = fixture::TestDb::new().await;
    tokio::time::timeout(Duration::from_secs(120), async {
        let writer = VehicleControlAuthority::new(db.a.clone());
        let reader = VehicleControlAuthority::new(db.b.clone());
        let pilot = identity("uav-index", "operations", "pilot", &[]);
        let peer = identity("uav-index", "operations", "peer", &[]);
        let foreign = identity("uav-foreign", "operations", "pilot", &[]);
        let context = identity("uav-index", "elsewhere", "pilot", &[]);
        let server = server(&db.b);
        let before = server
            .resource_descriptors_for_identity(&pilot)
            .await
            .unwrap();
        for i in 0..520 {
            let mut request = grant(&pilot, &format!("old-{i:04}"));
            request.valid_until = Some(Utc::now() - TimeDelta::hours(1));
            writer.grant(&pilot, request).await.unwrap();
        }
        for i in 0..102 {
            writer
                .grant(&pilot, grant(&pilot, &format!("visible-{i:04}")))
                .await
                .unwrap();
        }
        for other in [&peer, &foreign, &context] {
            writer
                .grant(other, grant(other, "other-visible"))
                .await
                .unwrap();
        }
        let mut revoked = grant(&pilot, "revoked");
        revoked.vehicle_id = VehicleId::new("vehicle-revoked").unwrap();
        writer.grant(&pilot, revoked).await.unwrap();
        writer
            .revoke(
                &pilot,
                RevokeVehicleControlRequest {
                    grant_id: ControlGrantId::new("revoked").unwrap(),
                    expected_revision: 0,
                },
            )
            .await
            .unwrap();
        let session = SessionId::new("native-session").unwrap();
        let vehicle = VehicleId::new("vehicle-one").unwrap();
        let admitted = reader
            .require_permission(
                &pilot,
                &session,
                &vehicle,
                VehicleControlPermission::Execute,
            )
            .await
            .unwrap();
        assert!(admitted.grant_id.as_str().starts_with("visible-"));
        assert!(
            reader
                .require_permission(&pilot, &session, &vehicle, VehicleControlPermission::Abort)
                .await
                .is_err()
        );
        assert!(
            reader
                .require_permission(
                    &pilot,
                    &session,
                    &VehicleId::new("vehicle-revoked").unwrap(),
                    VehicleControlPermission::Execute
                )
                .await
                .is_err()
        );
        assert!(
            reader
                .require_permission(
                    &pilot,
                    &SessionId::new("other-session").unwrap(),
                    &vehicle,
                    VehicleControlPermission::Execute
                )
                .await
                .is_err()
        );
        assert_eq!(
            reader
                .inspectable_vehicles(
                    &pilot,
                    &session,
                    &[vehicle.clone(), VehicleId::new("vehicle-revoked").unwrap()]
                )
                .await
                .unwrap(),
            BTreeSet::from([vehicle])
        );
        let active = reader
            .grants_page(&pilot, false, Some(&session), None)
            .await
            .unwrap();
        assert_eq!(active.items.len(), 100);
        assert_eq!(active.items[0].grant_id.as_str(), "visible-0000");
        let cursor = active.next_cursor.unwrap();
        let after = UavGrantCursor::parse(Some(&session), &cursor).unwrap();
        let last = reader
            .grants_page(&pilot, false, Some(&session), Some(after.position()))
            .await
            .unwrap();
        assert_eq!(last.items.len(), 2);
        assert!(last.next_cursor.is_none());
        assert!(UavGrantCursor::parse(None, &cursor).is_err());
        assert!(UavGrantCursor::parse(Some(&SessionId::new("other").unwrap()), &cursor).is_err());
        let mut count = 0;
        let mut after = None;
        loop {
            let page = reader
                .grants_page(&pilot, false, None, after.as_ref())
                .await
                .unwrap();
            count += page.items.len();
            let Some(cursor) = page.next_cursor else {
                break;
            };
            after = Some(
                UavGrantCursor::parse(None, &cursor)
                    .unwrap()
                    .position()
                    .clone(),
            );
            assert!(count <= 623);
        }
        assert_eq!(count, 623);
        assert!(
            reader
                .visible_grant(
                    &pilot,
                    false,
                    &ControlGrantId::new("other-visible").unwrap()
                )
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .visible_grant(&pilot, true, &ControlGrantId::new("other-visible").unwrap())
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            reader
                .complete_ids(&pilot, false, ControlCollection::Grants, "VISIBLE-0101")
                .await
                .unwrap(),
            ["visible-0101"]
        );
        assert!(
            reader
                .complete_ids(&pilot, false, ControlCollection::Grants, "other-visible")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            reader
                .complete_ids(&pilot, false, ControlCollection::Grants, "' OR true --")
                .await
                .unwrap()
                .is_empty()
        );

        let tasks = TaskRuntime::new(db.a.clone(), "uav-sim", "catalog-writer");
        let task_reader = TaskRuntime::new(db.b.clone(), "uav-sim", "catalog-reader");
        let mut plans = Vec::new();
        let mut snapshots = Vec::new();
        for i in 0..103 {
            let plan = writer
                .prepare_plan(&pilot, mission_request(&format!("mission-{i:04}")))
                .await
                .unwrap();
            snapshots.push(admitted_task(&tasks, &pilot, &plan).await);
            plans.push(plan);
        }
        let not_executed = writer
            .prepare_plan(&pilot, mission_request("unexecuted"))
            .await
            .unwrap();
        for other in [&peer, &foreign, &context] {
            let plan = writer
                .prepare_plan(other, mission_request("hidden-mission"))
                .await
                .unwrap();
            admitted_task(&tasks, other, &plan).await;
            assert!(
                reader
                    .visible_plan(&pilot, false, &plan.plan_id)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        let mut labeled = pilot.clone();
        labeled
            .actor
            .data_labels
            .insert(veoveo_types::DataLabelId::new("restricted").unwrap());
        let hidden = writer
            .prepare_plan(&pilot, mission_request("classified-mission"))
            .await
            .unwrap();
        let hidden_task = admitted_task(&tasks, &labeled, &hidden).await;
        let mut different_profile = pilot.clone();
        different_profile.profile =
            veoveo_mcp_contract::GatewayProfileId::new("other-profile").unwrap();
        let profile_task = task(&tasks, &different_profile, &hidden).await;
        for denied in [&peer, &foreign, &different_profile] {
            assert!(
                task_index::task(&db.b, denied, snapshots[0].task_id)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            task_index::task(&db.b, &pilot, hidden_task.task_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            task_index::task(&db.b, &pilot, profile_task.task_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            task_index::mission(&db.b, &pilot, &not_executed.mission_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            task_index::mission(&db.b, &pilot, &MissionId::new("hidden-mission").unwrap())
                .await
                .unwrap()
                .is_none()
        );
        let page = reader.plans_page(&pilot, false, None).await.unwrap();
        assert_eq!(page.items.len(), 100);
        let after = UavPlanCursor::parse(page.next_cursor.unwrap()).unwrap();
        assert_eq!(
            reader
                .plans_page(&pilot, false, Some(after.position()))
                .await
                .unwrap()
                .items
                .len(),
            5
        );
        assert_eq!(
            reader
                .complete_ids(
                    &pilot,
                    false,
                    ControlCollection::Plans,
                    &plans[102].plan_id.to_string().to_uppercase()
                )
                .await
                .unwrap(),
            [plans[102].plan_id.to_string()]
        );
        let page = task_index::missions_page(&db.b, &pilot, None)
            .await
            .unwrap();
        assert_eq!(page.items.len(), 100);
        assert_eq!(page.items[0], "uav-sim://mission/mission-0000");
        let after = UavMissionCursor::parse(page.next_cursor.unwrap()).unwrap();
        assert_eq!(
            task_index::missions_page(&db.b, &pilot, Some(after.position()))
                .await
                .unwrap()
                .items
                .len(),
            3
        );
        // A later unadmitted attempt must not replace the mission's linked Task.
        let _rejected = task(&tasks, &pilot, &plans[0]).await;
        assert_eq!(
            task_index::mission(&db.b, &pilot, &plans[0].mission_id)
                .await
                .unwrap()
                .unwrap()
                .task_id,
            snapshots[0].task_id
        );
        assert_eq!(
            task_index::complete(
                &db.b,
                &pilot,
                task_index::CompletionDomain::Missions,
                "MISSION-0102"
            )
            .await
            .unwrap(),
            ["mission-0102"]
        );
        assert_eq!(
            task_index::complete(
                &db.b,
                &pilot,
                task_index::CompletionDomain::Missions,
                "mission-"
            )
            .await
            .unwrap()
            .len(),
            101
        );
        assert!(
            task_index::complete(
                &db.b,
                &pilot,
                task_index::CompletionDomain::Missions,
                "classified"
            )
            .await
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            task_index::complete(
                &db.b,
                &pilot,
                task_index::CompletionDomain::Tasks,
                &snapshots[102].task_id.to_string().to_uppercase()
            )
            .await
            .unwrap(),
            [snapshots[102].task_id.to_string()]
        );
        let page = task_index::usage_page(&task_reader, &pilot, None)
            .await
            .unwrap();
        assert_eq!(page.items.len(), 100);
        let after = UavUsageCursor::parse(page.next_cursor.unwrap()).unwrap();
        // Task ownership admits the same actor's other Work Context, preserving the
        // shared runtime read profile. Mission resources use their plan's context.
        let tail = task_index::usage_page(&task_reader, &pilot, Some(after.position()))
            .await
            .unwrap();
        assert_eq!(tail.items.len(), 5);
        assert!(tail.next_cursor.is_none());
        let after = server
            .resource_descriptors_for_identity(&pilot)
            .await
            .unwrap();
        assert_eq!(
            before.iter().map(|r| &r.uri).collect::<Vec<_>>(),
            after.iter().map(|r| &r.uri).collect::<Vec<_>>()
        );
        assert!(after.iter().any(|r| r.uri == uris::MISSIONS));
        assert!(
            !after.iter().any(|r| r.uri.starts_with("uav-sim://mission/")
                || r.uri.starts_with("uav-sim://control-grant/")
                || r.uri.starts_with("uav-sim://mission-plan/")
                || r.uri.starts_with("uav-sim://usage/task/"))
        );
        let explain = task_index::explain_mission(&db.b, &pilot, &plans[102].mission_id)
            .await
            .unwrap();

        assert!(
            explain.contains("task_uav_plan"),
            "mission lookup did not use its plan index: {explain}"
        );
        assert!(
            explain.contains("uav_mission_plan_mission"),
            "mission lookup did not use its mission index: {explain}"
        );
        let hub = Arc::new(SubscriptionHub::new());
        let mut contents = hub.listen();
        let mut lists = hub.listen_resource_list_changes();
        let stop = tokio_util::sync::CancellationToken::new();
        let observer = tokio::spawn(super::service::resources::observe(
            db.b.clone(),
            hub,
            stop.clone(),
        ));
        tokio::time::timeout(Duration::from_secs(10), contents.recv())
            .await
            .unwrap()
            .unwrap();
        writer
            .grant(&pilot, grant(&pilot, "notification-grant"))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), contents.recv())
            .await
            .unwrap()
            .unwrap();
        let notification_plan = writer
            .prepare_plan(&pilot, mission_request("notification-mission"))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), contents.recv())
            .await
            .unwrap()
            .unwrap();
        task(&tasks, &pilot, &notification_plan).await;
        tokio::time::timeout(Duration::from_secs(10), contents.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            lists.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
        stop.cancel();
        observer.await.unwrap();
        let mut stream_reader = pilot.clone();
        stream_reader.actor.scopes.insert(UavScope::Stream.into());
        assert!(
            server
                .resource_descriptors_for_identity(&stream_reader)
                .await
                .unwrap()
                .iter()
                .any(|r| r.uri == uris::LIVE_APP_URI)
        );
    })
    .await
    .expect("UAV native catalog qualification exceeded 120 seconds");
}
