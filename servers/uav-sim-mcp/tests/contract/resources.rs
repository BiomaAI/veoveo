use veoveo_uav_sim_mcp::{contract::*, uris};

fn wire(resource: &UavResource) -> String {
    serde_json::to_value(resource)
        .unwrap()
        .as_str()
        .unwrap()
        .into()
}
fn hex_json(json: &str) -> String {
    json.bytes().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn every_declared_resource_round_trips_through_the_public_contract() {
    for uri in [
        "uav-sim://docs",
        "uav-sim://docs/agents",
        "uav-sim://docs/design",
        "uav-sim://contract",
        "ui://uav-sim/live.html",
        "uav-sim://sessions",
        "uav-sim://session/S_1.2-3",
        "uav-sim://session/s/world",
        "uav-sim://session/s/tiles",
        "uav-sim://session/s/vehicles",
        "uav-sim://session/s/vehicle/v",
        "uav-sim://session/s/recordings",
        "uav-sim://session/s/live-cameras",
        "uav-sim://session/s/live-camera/c",
        "uav-sim://session/s/stream-products",
        "uav-sim://session/s/stream-product/p",
        "uav-sim://session/s/live-views",
        "uav-sim://session/s/live-view/v",
        "uav-sim://control-grants",
        "uav-sim://control-grant/g",
        "uav-sim://mission-plans",
        "uav-sim://mission-plan/p",
        "uav-sim://missions",
        "uav-sim://mission/m",
        "uav-sim://usage",
        "uav-sim://usage/task/0195e2ec-54a1-7000-8000-000000000001",
    ] {
        let resource = UavResource::parse(uri).unwrap();
        assert_eq!(wire(&resource), uri);
        assert_eq!(
            serde_json::from_value::<UavResource>(uri.into()).unwrap(),
            resource
        );
    }
    let session = SessionId::new("session-A.1").unwrap();
    assert_eq!(
        uris::vehicle(&session, &VehicleId::new("vehicle-B").unwrap()).as_str(),
        "uav-sim://session/session-A.1/vehicle/vehicle-B"
    );
    assert_eq!(
        UavResource::parse(uris::world(&session).as_str()).unwrap(),
        UavResource::World(session)
    );
}

#[test]
fn cursor_v1_bytes_and_positions_are_preserved() {
    let mission = UavMissionCursor::new(MissionId::new("m").unwrap()).unwrap();
    let plan = UavPlanCursor::new(MissionPlanId::new("p").unwrap()).unwrap();
    let grant = UavGrantCursor::new(None, ControlGrantId::new("g").unwrap()).unwrap();
    let session = LiveSessionId::new("s").unwrap();
    let view = UavLiveViewCursor::new(session.clone(), LiveViewId::new("v").unwrap()).unwrap();
    let usage = UavUsageCursor::new(UavUsagePosition {
        created_at: "2026-09-28T12:00:00Z".parse().unwrap(),
        task_id: "0195e2ec-54a1-7000-8000-000000000001".parse().unwrap(),
    })
    .unwrap();
    for (cursor, expected) in [
        (
            mission.as_str(),
            r#"{"version":1,"collection":"uav-sim://missions","position":"m"}"#,
        ),
        (
            plan.as_str(),
            r#"{"version":1,"collection":"uav-sim://mission-plans","position":"p"}"#,
        ),
        (
            grant.as_str(),
            r#"{"version":1,"collection":"uav-sim://control-grants","position":"g"}"#,
        ),
        (
            view.as_str(),
            r#"{"version":1,"collection":"uav-sim://session/s/live-views","position":"v"}"#,
        ),
        (
            usage.as_str(),
            r#"{"version":1,"collection":"uav-sim://usage","position":{"created_at":"2026-09-28T12:00:00Z","task_id":"0195e2ec-54a1-7000-8000-000000000001"}}"#,
        ),
    ] {
        assert_eq!(cursor, hex_json(expected));
    }
    for resource in [
        UavResource::Missions {
            cursor: Some(mission.clone()),
        },
        UavResource::MissionPlans { cursor: Some(plan) },
        UavResource::ControlGrants {
            cursor: Some(grant),
        },
        UavResource::LiveViews {
            session,
            cursor: Some(view),
        },
        UavResource::Usage {
            cursor: Some(usage),
        },
    ] {
        assert_eq!(UavResource::parse(&wire(&resource)).unwrap(), resource);
        assert!(!resource.is_subscribable());
    }
    assert_eq!(
        UavMissionCursor::parse(mission.as_str())
            .unwrap()
            .position()
            .as_str(),
        "m"
    );
}

#[test]
fn cursors_cannot_move_between_families_or_session_parents() {
    let session = SessionId::new("s").unwrap();
    let grant = UavGrantCursor::new(Some(&session), ControlGrantId::new("g").unwrap()).unwrap();
    assert_eq!(
        grant.as_str(),
        hex_json(
            r#"{"version":1,"collection":"uav-sim://control-grants?active_session=s","position":"g"}"#
        )
    );
    assert_eq!(
        UavGrantCursor::parse(Some(&session), grant.as_str()).unwrap(),
        grant
    );
    assert!(UavGrantCursor::parse(None, grant.as_str()).is_err());
    assert!(
        UavGrantCursor::parse(Some(&SessionId::new("other").unwrap()), grant.as_str()).is_err()
    );
    assert!(
        serde_json::to_value(UavResource::ControlGrants {
            cursor: Some(grant)
        })
        .is_err()
    );
    let plan = UavPlanCursor::new(MissionPlanId::new("p").unwrap()).unwrap();
    assert!(UavMissionCursor::parse(plan.as_str()).is_err());
    assert!(UavUsageCursor::parse(plan.as_str()).is_err());
    assert!(UavGrantCursor::parse(None, plan.as_str()).is_err());
    let view = UavLiveViewCursor::new(
        LiveSessionId::new("s").unwrap(),
        LiveViewId::new("v").unwrap(),
    )
    .unwrap();
    let other = LiveSessionId::new("other").unwrap();
    assert!(UavLiveViewCursor::parse(&other, view.as_str()).is_err());
    assert!(
        UavResource::parse(&format!(
            "uav-sim://session/other/live-views?cursor={}",
            view.as_str()
        ))
        .is_err()
    );
    assert!(
        serde_json::to_value(UavResource::LiveViews {
            session: other,
            cursor: Some(view)
        })
        .is_err()
    );
}

#[test]
fn cursor_admission_rejects_invalid_versions_shapes_ids_and_size() {
    for json in [
        r#"{"version":2,"collection":"uav-sim://missions","position":"m"}"#,
        r#"{"version":1,"collection":"uav-sim://missions","position":".."}"#,
        r#"{"version":1,"collection":"uav-sim://missions","position":"m","extra":true}"#,
        r#"{"version":1,"collection":"uav-sim://missions","position":42}"#,
        r#"{"collection":"uav-sim://missions","position":"m"}"#,
    ] {
        assert!(UavMissionCursor::parse(hex_json(json)).is_err());
    }
    for value in ["", "not-hex", "00", &"a".repeat(2049)] {
        assert!(UavMissionCursor::parse(value).is_err());
    }
    for task in ["0195e2ec-54a1-4000-8000-000000000001", "not-a-task"] {
        let json = format!(
            r#"{{"version":1,"collection":"uav-sim://usage","position":{{"created_at":"2026-09-28T12:00:00Z","task_id":"{task}"}}}}"#
        );
        assert!(UavUsageCursor::parse(hex_json(&json)).is_err());
        assert!(UavResource::parse(&format!("uav-sim://usage/task/{task}")).is_err());
    }
    let task = "0195e2ec-54a1-4000-8000-000000000001".parse().unwrap();
    assert!(uris::usage_task(task).is_err());
}

#[test]
fn resource_admission_rejects_ambiguous_routes_and_components() {
    for uri in [
        "uav-sim://session/.",
        "uav-sim://session/..",
        "uav-sim://session/%2e%2e",
        "uav-sim://session/s/../other",
        "uav-sim://session/s/",
        "uav-sim://session//world",
        "uav-sim://session/s/world/extra",
        "uav-sim://session/s%2fother",
        "uav-sim://session/%73",
        "uav-sim://session/s%00",
        "uav-sim://session/s%zz",
        "uav-sim://session/s%FF",
        "uav-sim://session/s?cursor=00",
        "uav-sim://missions?",
        "uav-sim://missions?cursor=",
        "uav-sim://missions?cursor=00&cursor=00",
        "uav-sim://missions?cursor=00&%63ursor=00",
        "uav-sim://missions?offset=1",
        "uav-sim://missions#secret",
        "uav-sim://user:password@missions",
        "uav-sim://missions:42",
        "uav-sim://docs/other",
        "uav-sim://docs/design/extra",
        "other://missions",
        "uav-sim://SESSION/s",
        "UAV-SIM://session/s",
        "uav-sim://session/{session_id}",
    ] {
        let error = UavResource::parse(uri).unwrap_err();
        assert!(
            !error.to_string().contains(uri),
            "errors must redact URI inputs"
        );
        assert!(
            serde_json::from_value::<UavResource>(uri.into()).is_err(),
            "{uri}"
        );
    }
}

#[test]
fn relative_identifiers_are_rejected_at_construction_and_retained_json_admission() {
    macro_rules! check { ($($ty:ty),+ $(,)?) => { $(
        for value in [".", "..", "", "a/b", "a?b", "a#b", "a%b", "a b", "é"] {
            assert!(<$ty>::new(value).is_err(), "{} {value}", stringify!($ty));
            assert!(serde_json::from_value::<$ty>(value.into()).is_err());
        }
        assert!(<$ty>::new("A_1.-b").is_ok());
    )+ }; }
    check!(
        SessionId,
        VehicleId,
        MissionId,
        MissionPlanId,
        ControlGrantId,
        RecordingKey,
        LiveSessionId,
        LiveCameraId,
        LiveViewId,
        LiveEntityId,
        LiveViewerInstanceId,
        LiveStreamProductId
    );
    assert!(
        serde_json::from_value::<SessionRequest>(serde_json::json!({"session_id":".."})).is_err()
    );
    assert!(
        serde_json::from_value::<OpenLiveViewRequest>(serde_json::json!({
            "session_id":"s", "camera_id":"..", "viewer_instance_id":"viewer"
        }))
        .is_err()
    );
}

#[test]
fn generic_live_view_addresses_preserve_other_provider_schemes_without_normalization() {
    assert!(LiveViewUri::new("ground-sim://session/s/live-view/v").is_ok());
    assert!(
        LiveViewUri::new(
            uris::live_view(
                &LiveSessionId::new("s").unwrap(),
                &LiveViewId::new("v").unwrap()
            )
            .as_str()
        )
        .is_ok()
    );
    for uri in [
        "https://session/s/live-view/v",
        "ground+sim://session/s/live-view/v",
        "ground.sim://session/s/live-view/v",
        "ground-sim://session/a/../s/live-view/v",
        "ground-sim://session/%73/live-view/v",
        "ground-sim://session/s/live-view/v?",
        "ground-sim://session/s/live-view/v#secret",
    ] {
        assert!(LiveViewUri::new(uri).is_err());
    }
}
