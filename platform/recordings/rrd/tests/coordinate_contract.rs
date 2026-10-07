fn check_schema<T: schemars::JsonSchema>(baseline: &serde_json::Value, name: &str) {
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(T)).unwrap(),
        baseline[name],
        "{name}"
    );
}
#[test]
fn coordinate_contract_schemas_preserve_published_wire_shapes() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/coordinate-contract.schema.json");
    if std::env::var_os("UPDATE_RECORDING_RRD_FIXTURES").is_some() {
        let produced = serde_json::json!({
            "FrameKind": schemars::schema_for!(veoveo_rrd::FrameKind),
            "GeofenceId": schemars::schema_for!(veoveo_rrd::GeofenceId),
            "GeofenceRule": schemars::schema_for!(veoveo_rrd::GeofenceRule),
            "RrdFrameDefinition": schemars::schema_for!(veoveo_rrd::RrdFrameDefinition),
            "RrdGeofenceGeometry": schemars::schema_for!(veoveo_rrd::RrdGeofenceGeometry),
        });
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&produced).unwrap()),
        )
        .unwrap();
    }
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();

    check_schema::<veoveo_rrd::FrameKind>(&baseline, "FrameKind");
    check_schema::<veoveo_rrd::GeofenceId>(&baseline, "GeofenceId");
    check_schema::<veoveo_rrd::GeofenceRule>(&baseline, "GeofenceRule");
    check_schema::<veoveo_rrd::RrdFrameDefinition>(&baseline, "RrdFrameDefinition");
    check_schema::<veoveo_rrd::RrdGeofenceGeometry>(&baseline, "RrdGeofenceGeometry");
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
        GeofenceId::parse("geo:mission-1").unwrap().as_str(),
        "geo:mission-1"
    );
    assert!(GeofenceId::parse("bad/id").is_err());
    assert!(GeofenceId::parse("x".repeat(129)).is_err());
}
