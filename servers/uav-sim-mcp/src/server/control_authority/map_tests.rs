//! Native authorization tests use an isolated database and never dispatch a simulator.
use super::*;
use crate::server::{
    catalog_tests::{grant, mission_request},
    test_support::{fixture::TestDb, identity},
};
use std::time::Duration as Timeout;
use veoveo_map_mcp::contract::{MobilityProfileId, MobilityProfileVersion, RouteStatus};

#[test]
fn map_status_and_handoff_policy_stay_explicit() {
    let pilot = identity("native", "operations", "pilot", &[]);
    let mut request = mission_request("policy");
    let now = Utc::now();
    let granted = grant(&pilot, "policy");
    let mut granted = VehicleControlGrant {
        grant_id: granted.grant_id,
        session_id: granted.session_id,
        vehicle_id: granted.vehicle_id,
        principal_key: granted.principal_key,
        permissions: granted.permissions,
        map_mobility_profile_uri: granted.map_mobility_profile_uri,
        allow_planning_advisory: false,
        valid_from: granted.valid_from,
        valid_until: None,
        created_by: "admin".into(),
        revoked_at: None,
        revoked_by: None,
        revision: 0,
        created_at: now,
        updated_at: now,
    };
    assert!(validate_map_handoff(&request, &granted).is_ok());
    for status in [
        RouteStatus::Stale,
        RouteStatus::Invalidated,
        RouteStatus::Unavailable,
    ] {
        request.map_route.route_status = status;
        assert!(RouteRequirement::new(&request.map_route).is_err());
        assert!(validate_map_handoff(&request, &granted).is_err());
    }
    request.map_route.route_status = RouteStatus::PlanningAdvisory;
    assert!(validate_map_handoff(&request, &granted).is_err());
    granted.allow_planning_advisory = true;
    assert!(validate_map_handoff(&request, &granted).is_ok());
    let good = request.clone();
    request.map_route.path[0].ellipsoidal_height_m = None;
    assert!(validate_map_handoff(&request, &granted).is_err());
    request = good.clone();
    request.map_route.path[0].latitude_deg = 91.0;
    assert!(validate_map_handoff(&request, &granted).is_err());
    request = good.clone();
    request.map_route.validated_at -= Duration::minutes(6);
    assert!(validate_map_handoff(&request, &granted).is_err());
    request = good.clone();
    request.map_route.path.truncate(1);
    assert!(validate_map_handoff(&request, &granted).is_err());
    let mut wire = serde_json::to_value(&good).unwrap();
    wire["map_route"]["path"][0]["unexpected"] = true.into();
    assert!(serde_json::from_value::<PrepareVehicleMissionRequest>(wire).is_err());
}

#[tokio::test]
async fn native_route_grants_filter_profiles_and_advisory_before_limit() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = TestDb::new().await;
    tokio::time::timeout(Timeout::from_secs(120), async {
        let authority = VehicleControlAuthority::new(db.a.clone());
        let reader = VehicleControlAuthority::new(db.b.clone());
        let pilot = identity("route-grants", "operations", "pilot", &[]);
        let mut request = mission_request("route-profile");
        request.map_route.route_status = RouteStatus::PlanningAdvisory;
        let wrong_profile = MapMobilityProfileUri::new(
            MobilityProfileId::from_stable_key(b"other"),
            MobilityProfileVersion::FIRST,
        );
        for i in 0..110 {
            let mut excluded = grant(&pilot, &format!("early-{i:03}"));
            if i % 2 == 0 {
                excluded.map_mobility_profile_uri = wrong_profile.clone();
                excluded.allow_planning_advisory = true;
            }
            authority.grant(&pilot, excluded).await.unwrap();
        }
        assert!(matches!(
            reader.prepare_plan(&pilot, request.clone()).await,
            Err(ControlAuthorityError::Forbidden)
        ));
        let mut admitted = grant(&pilot, "matching");
        admitted.allow_planning_advisory = true;
        let granted = authority.grant(&pilot, admitted).await.unwrap();
        assert_eq!(
            granted.map_mobility_profile_uri,
            request.map_route.mobility_profile_uri
        );
        let plan = reader.prepare_plan(&pilot, request).await.unwrap();
        assert_eq!(
            reader
                .visible_plan(&pilot, false, &plan.plan_id)
                .await
                .unwrap()
                .unwrap(),
            plan
        );
        assert_eq!(
            reader
                .visible_grant(&pilot, false, &granted.grant_id)
                .await
                .unwrap()
                .unwrap(),
            granted
        );
        authority
            .revoke(
                &pilot,
                RevokeVehicleControlRequest {
                    grant_id: granted.grant_id,
                    expected_revision: 0,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            reader.begin_execution(&pilot, &plan.plan_id, 0).await,
            Err(ControlAuthorityError::Forbidden)
        ));
        let mut replacement = grant(&pilot, "replacement");
        replacement.allow_planning_advisory = true;
        authority.grant(&pilot, replacement).await.unwrap();
        let (executing, guard) = reader
            .begin_execution(&pilot, &plan.plan_id, 0)
            .await
            .unwrap();
        assert_eq!(executing.state, MissionPlanLifecycle::Executing);
        reader.finish_execution(&guard, true).await.unwrap();
        assert_eq!(
            reader
                .visible_plan(&pilot, false, &plan.plan_id)
                .await
                .unwrap()
                .unwrap()
                .state,
            MissionPlanLifecycle::Completed
        );
    })
    .await
    .expect("route grant selection exceeded 120 seconds");
}

#[tokio::test]
async fn native_execution_rechecks_grant_after_command_lease_acquisition() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = TestDb::new().await;
    tokio::time::timeout(Timeout::from_secs(60), async {
        let authority = VehicleControlAuthority::new(db.a.clone());
        let writer = VehicleControlAuthority::new(db.b.clone());
        let pilot = identity("admission", "operations", "pilot", &[]);
        let granted = authority
            .grant(&pilot, grant(&pilot, "matching"))
            .await
            .unwrap();
        let plan = authority
            .prepare_plan(&pilot, mission_request("revoked-between-reads"))
            .await
            .unwrap();
        let record_id = scoped_record_id("uav_vehicle_mission_plan", &pilot, plan.plan_id.as_str());
        let record = authority.plan_record(&record_id).await.unwrap();
        authority
            .require_route_permission(
                &pilot,
                &plan.session_id,
                &plan.vehicle_id,
                VehicleControlPermission::Execute,
                &plan.map_route,
            )
            .await
            .unwrap();
        let lease = authority.acquire_lease(&pilot, &plan).await.unwrap();
        writer
            .revoke(
                &pilot,
                RevokeVehicleControlRequest {
                    grant_id: granted.grant_id,
                    expected_revision: 0,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            authority
                .admit_execution(&pilot, record, plan.clone(), lease.clone())
                .await,
            Err(ControlAuthorityError::Conflict)
        ));
        let retained = authority
            .visible_plan(&pilot, false, &plan.plan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained, plan);
        let released: LeaseRecord = select_only(&db.a, lease.record_id, "lease").await.unwrap();
        assert!(released.released_at.is_some());
    })
    .await
    .expect("execution admission exceeded 60 seconds");
}

#[tokio::test]
async fn native_selected_plan_metadata_and_grant_profile_fail_closed() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = TestDb::new().await;
    tokio::time::timeout(Timeout::from_secs(60), async {
        let authority = VehicleControlAuthority::new(db.a.clone());
        let pilot = identity("retained", "operations", "pilot", &[]);
        authority.grant(&pilot, grant(&pilot, "matching")).await.unwrap();
        let plan = authority.prepare_plan(&pilot, mission_request("retained")).await.unwrap();
        let record_id = scoped_record_id("uav_vehicle_mission_plan", &pilot, plan.plan_id.as_str());
        let good = authority.plan_record(&record_id).await.unwrap();
        assert_eq!(plan_view(&good).unwrap(), plan);
        let mut bad = Vec::new();
        macro_rules! corrupt {
            ($field:ident, $value:expr) => {{
                let mut row = good.clone(); row.$field = $value; bad.push(row);
            }};
        }
        corrupt!(plan_id, "different".into());
        corrupt!(mission_id, "different".into());
        corrupt!(principal_key, "different".into());
        corrupt!(session_id, "different".into());
        corrupt!(vehicle_id, "different".into());
        corrupt!(map_route_uri, "map://route/different".into());
        corrupt!(map_route_digest_sha256, "b".repeat(64));
        corrupt!(map_mobility_profile_uri, "map://mobility-profile/invalid/1".into());
        corrupt!(state, "executing".into());
        corrupt!(revision, 1);
        corrupt!(expires_at, good.expires_at + Duration::seconds(1));
        corrupt!(created_at, good.created_at + Duration::seconds(1));
        corrupt!(updated_at, good.updated_at + Duration::seconds(1));
        for row in bad { assert!(plan_view(&row).is_err()); }
        let mut wrong_id = good.clone();
        wrong_id.id = RecordId::new("uav_vehicle_mission_plan", "wrong");
        assert!(visible_plan_view(&wrong_id, &pilot).is_err());
        db.b.client().query("UPDATE ONLY $record SET map_mobility_profile_uri = 'map://mobility-profile/invalid/1';")
            .bind(("record", record_id)).await.unwrap().check().unwrap();
        assert!(authority.visible_plan(&pilot, false, &plan.plan_id).await.is_err());
        assert!(authority.plans_page(&pilot, false, None).await.is_err());
        assert!(authority.begin_execution(&pilot, &plan.plan_id, 0).await.is_err());
        let peer = identity("retained", "operations", "peer", &[]);
        assert!(authority.plans_page(&peer, false, None).await.unwrap().items.is_empty());
        let bad_grant = scoped_record_id("uav_vehicle_control_grant", &pilot, "matching");
        db.b.client().query("UPDATE ONLY $record SET map_mobility_profile_uri = 'map://mobility-profile/invalid/1';")
            .bind(("record", bad_grant)).await.unwrap().check().unwrap();
        assert!(authority.grants_page(&pilot, false, None, None).await.is_err());
        assert!(authority.grants_page(&peer, false, None, None).await.unwrap().items.is_empty());
    }).await.expect("retained Map metadata qualification exceeded 60 seconds");
}
