use veoveo_map_mcp::contract::{
    OptimizationTravelModel, TRAVEL_MODEL_ARTIFACT_VERSION, TravelLocationId, TravelModelArtifact,
    TravelModelMatrix, TravelVehicleTypeId,
};
use veoveo_optimization_mcp::domain::{
    TRAVEL_MODEL_ARTIFACT_VERSION as OPTIMIZATION_ARTIFACT_VERSION,
    TravelModelArtifact as OptimizationArtifact,
};

#[test]
fn map_package_is_the_optimization_wire_contract() {
    assert_eq!(TRAVEL_MODEL_ARTIFACT_VERSION, OPTIMIZATION_ARTIFACT_VERSION);
    let package = TravelModelArtifact {
        version: TRAVEL_MODEL_ARTIFACT_VERSION.to_owned(),
        map_resource_uri: Some(
            "map://travel-model/travel-model-018f6c6e-7b8a-7c01-8000-000000000001".to_owned(),
        ),
        model: OptimizationTravelModel {
            location_ids: vec![
                TravelLocationId::new("depot").unwrap(),
                TravelLocationId::new("customer").unwrap(),
            ],
            cost_matrices: vec![matrix()],
            transit_time_matrices: vec![matrix()],
        },
    };

    let optimization: OptimizationArtifact =
        serde_json::from_value(serde_json::to_value(package).unwrap()).unwrap();
    assert_eq!(optimization.version, OPTIMIZATION_ARTIFACT_VERSION);
    assert_eq!(optimization.model.location_ids.len(), 2);
    assert_eq!(
        optimization.model.cost_matrices[0].values,
        vec![0.0, 12.0, 10.0, 0.0]
    );
}

fn matrix() -> TravelModelMatrix {
    TravelModelMatrix {
        vehicle_type_id: TravelVehicleTypeId::new("truck").unwrap(),
        dimension: 2,
        values: vec![0.0, 12.0, 10.0, 0.0],
        unavailable_cells: Vec::new(),
    }
}
