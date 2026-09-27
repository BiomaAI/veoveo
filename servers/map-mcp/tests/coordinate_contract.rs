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
    check!(veoveo_map_mcp::contract::CrsId, "CrsId");
    check!(veoveo_map_mcp::contract::DatumId, "DatumId");
    check!(veoveo_map_mcp::contract::EllipsoidId, "EllipsoidId");
    check!(
        veoveo_map_mcp::contract::ProjectedPosition,
        "ProjectedPosition"
    );
    check!(
        veoveo_map_mcp::contract::TransformCrsOutput,
        "TransformCrsOutput"
    );
    check!(
        veoveo_map_mcp::contract::TransformCrsRequest,
        "TransformCrsRequest"
    );
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
