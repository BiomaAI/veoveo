use veoveo_map_mcp::contract::*;
use veoveo_types::Sha256Digest;

fn builder() -> MapRouteHandoffBuilder {
    let now = "2026-10-04T00:00:00Z".parse().unwrap();
    MapRouteHandoffBuilder {
        schema_profile: MapRouteHandoffSchema::V1,
        route_uri: MapRouteUri::new(RouteId::new()),
        route_digest_sha256: Sha256Digest::from_bytes([0xab; 32]),
        route_status: RouteStatus::Validated,
        mobility_profile_uri: MapMobilityProfileUri::new(
            MobilityProfileId::new(),
            MobilityProfileVersion::FIRST,
        ),
        path: vec![
            Wgs84Position::new(-74., 40., None).unwrap(),
            Wgs84Position::new(-73., 41., None).unwrap(),
        ],
        validation_id: ValidationId::new(),
        validated_at: now,
        operational_snapshot_id: OperationalSnapshotId::new(),
        base_release_ids: vec![DatasetReleaseId::new()],
        restriction_ids: vec![RestrictionId::new()],
        prepared_at: now,
    }
}

#[test]
fn handoff_keeps_owner_types_and_the_existing_wire_profile() {
    let input = builder();
    let handoff = input.clone().build().unwrap();
    assert_eq!(handoff.route_uri(), &input.route_uri);
    assert_eq!(handoff.route_digest_sha256(), &input.route_digest_sha256);
    assert_eq!(
        handoff.operational_snapshot_id(),
        &input.operational_snapshot_id
    );
    let wire = serde_json::to_value(&handoff).unwrap();
    assert_eq!(wire["schema_profile"], MAP_ROUTE_HANDOFF_SCHEMA);
    assert_eq!(wire["route_digest_sha256"], "ab".repeat(32));
    assert_eq!(
        serde_json::from_value::<MapRouteHandoff>(wire).unwrap(),
        handoff
    );
    // Map supports ground routes; UAV alone requires ellipsoidal height for actuation.
    assert!(
        handoff
            .path()
            .iter()
            .all(|position| position.ellipsoidal_height_m.is_none())
    );
    let mut advisory = input;
    advisory.route_status = RouteStatus::PlanningAdvisory;
    assert!(advisory.build().is_ok());
}

#[test]
fn handoff_decode_rejects_forged_identity_provenance_and_relationships() {
    let handoff = builder().build().unwrap();
    let wire = serde_json::to_value(&handoff).unwrap();
    for (field, bad) in [
        (
            "schema_profile",
            serde_json::json!("veoveo.ai/map-route-handoff/v2"),
        ),
        (
            "route_uri",
            serde_json::json!(format!("{}?extra=true", handoff.route_uri())),
        ),
        (
            "route_uri",
            serde_json::json!(MapRasterUri::new(RasterProductId::new())),
        ),
        ("route_digest_sha256", serde_json::json!("AB".repeat(32))),
        (
            "route_digest_sha256",
            serde_json::json!(handoff.route_digest_sha256()),
        ),
        ("validation_id", serde_json::json!(RouteId::new())),
        (
            "operational_snapshot_id",
            serde_json::json!(DatasetReleaseId::new()),
        ),
        ("base_release_ids", serde_json::json!([])),
        (
            "base_release_ids",
            serde_json::json!([handoff.base_release_ids()[0], handoff.base_release_ids()[0]]),
        ),
        (
            "restriction_ids",
            serde_json::json!([handoff.restriction_ids()[0], handoff.restriction_ids()[0]]),
        ),
        ("prepared_at", serde_json::json!("2026-10-03T23:59:59Z")),
        ("route_status", serde_json::json!("stale")),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = bad;
        assert!(
            serde_json::from_value::<MapRouteHandoff>(invalid).is_err(),
            "{field}"
        );
    }
}

#[test]
fn handoff_builder_checks_geometry_and_route_status_before_use() {
    for status in [
        RouteStatus::Stale,
        RouteStatus::Invalidated,
        RouteStatus::Unavailable,
    ] {
        let mut input = builder();
        input.route_status = status;
        assert_eq!(input.build().unwrap_err(), MapRouteHandoffError::Status);
    }
    for path in [
        vec![],
        vec![Wgs84Position::new(0., 0., None).unwrap()],
        vec![Wgs84Position::new(0., 0., None).unwrap(); 2],
        vec![
            Wgs84Position {
                longitude_deg: f64::NAN,
                latitude_deg: 0.,
                ellipsoidal_height_m: None
            };
            2
        ],
        vec![
            Wgs84Position {
                longitude_deg: 0.,
                latitude_deg: 91.,
                ellipsoidal_height_m: None
            };
            2
        ],
        vec![Wgs84Position::new(0., 0., None).unwrap(); 10_001],
    ] {
        let mut input = builder();
        input.path = path;
        assert_eq!(input.build().unwrap_err(), MapRouteHandoffError::Path);
    }
    let mut invalid = serde_json::to_value(builder().build().unwrap()).unwrap();
    invalid["path"][0]["latitude_deg"] = 91.into();
    assert!(serde_json::from_value::<MapRouteHandoff>(invalid).is_err());
}
