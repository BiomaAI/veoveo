fn check_schema<T: schemars::JsonSchema>(baseline: &serde_json::Value, name: &str) {
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(T)).unwrap(),
        baseline[name],
        "{name}"
    );
}
#[test]
fn coordinate_contract_schemas_preserve_published_wire_shapes() {
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/coordinate-contract.schema.json")).unwrap();

    check_schema::<veoveo_map_mcp::contract::CrsId>(&baseline, "CrsId");
    check_schema::<veoveo_map_mcp::contract::DatumId>(&baseline, "DatumId");
    check_schema::<veoveo_map_mcp::contract::EllipsoidId>(&baseline, "EllipsoidId");
    check_schema::<veoveo_map_mcp::contract::ProjectedPosition>(&baseline, "ProjectedPosition");
    check_schema::<veoveo_map_mcp::contract::TransformCrsOutput>(&baseline, "TransformCrsOutput");
    check_schema::<veoveo_map_mcp::contract::TransformCrsRequest>(&baseline, "TransformCrsRequest");
}

#[test]
fn geodetic_names_preserve_the_existing_admission_profile() {
    use veoveo_map_mcp::contract::{CrsId, DatumId, EllipsoidId};
    for valid in ["EPSG:4326", "WGS84", "GRS80", ".", "..", &"x".repeat(128)] {
        assert_eq!(CrsId::parse(valid).unwrap().as_str(), valid);
        assert_eq!(DatumId::parse(valid).unwrap().as_str(), valid);
        assert_eq!(EllipsoidId::parse(valid).unwrap().as_str(), valid);
    }
    for invalid in [
        "",
        "EPSG/4326",
        "bad id",
        "crs://4326",
        "Ü",
        &"x".repeat(129),
    ] {
        assert!(CrsId::parse(invalid).is_err());
        assert!(serde_json::from_value::<CrsId>(serde_json::json!(invalid)).is_err());
    }
}

#[test]
fn transform_inputs_reject_unknown_keys_at_request_and_position() {
    use veoveo_map_mcp::contract::TransformCrsRequest;
    let wire = serde_json::json!({
        "sourceCrs": "EPSG:4326", "targetCrs": "EPSG:3857",
        "positions": [{"crs": "EPSG:4326", "x": -89.2, "y": 13.7}]
    });
    let schema = serde_json::to_value(schemars::schema_for!(TransformCrsRequest)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&wire));
    assert!(serde_json::from_value::<TransformCrsRequest>(wire.clone()).is_ok());
    for pointer in ["", "/positions/0"] {
        let mut invalid = wire.clone();
        invalid.pointer_mut(pointer).unwrap()["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<TransformCrsRequest>(invalid.clone()).is_err());
        assert!(!validator.is_valid(&invalid));
    }
}
