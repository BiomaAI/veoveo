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
        assert_eq!(CrsId::new(valid).unwrap().as_str(), valid);
        assert_eq!(DatumId::new(valid).unwrap().as_str(), valid);
        assert_eq!(EllipsoidId::new(valid).unwrap().as_str(), valid);
    }
    for invalid in [
        "",
        "EPSG/4326",
        "bad id",
        "crs://4326",
        "Ü",
        &"x".repeat(129),
    ] {
        assert!(CrsId::new(invalid).is_err());
        assert!(serde_json::from_value::<CrsId>(serde_json::json!(invalid)).is_err());
    }
}
