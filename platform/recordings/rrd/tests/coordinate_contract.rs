#[test]
fn coordinate_contract_schemas_preserve_published_wire_shapes() {
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/coordinate-contract.schema.json")).unwrap();
    macro_rules! check {
        ($ty:ty, $name:literal) => {
            assert_eq!(
                serde_json::to_value(schemars::schema_for!($ty)).unwrap(),
                baseline[$name],
                "{}",
                $name
            );
        };
    }
    check!(veoveo_rrd::FrameKind, "FrameKind");
    check!(veoveo_rrd::GeofenceId, "GeofenceId");
    check!(veoveo_rrd::GeofenceRule, "GeofenceRule");
    check!(veoveo_rrd::RrdFrameDefinition, "RrdFrameDefinition");
    check!(veoveo_rrd::RrdGeofenceGeometry, "RrdGeofenceGeometry");
}

#[test]
fn recorded_geofences_keep_their_distinct_vocabulary() {
    use veoveo_rrd::{GeofenceId, GeofenceRule};
    assert_eq!(
        serde_json::to_value(GeofenceRule::MustStayInside).unwrap(),
        "must_stay_inside"
    );
    assert_eq!(
        serde_json::to_value(GeofenceRule::MustStayOutside).unwrap(),
        "must_stay_outside"
    );
    assert!(serde_json::from_str::<GeofenceRule>("\"must_remain_inside\"").is_err());
    assert_eq!(
        GeofenceId::new("geo:mission-1").unwrap().as_str(),
        "geo:mission-1"
    );
    assert!(GeofenceId::new("bad/id").is_err());
    assert!(GeofenceId::new("x".repeat(129)).is_err());
}
