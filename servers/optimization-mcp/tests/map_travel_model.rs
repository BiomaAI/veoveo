use veoveo_map_mcp::contract::{
    MapTravelModelUri, OptimizationTravelModel, TravelLocationId, TravelModelArtifact,
    TravelModelId, TravelModelMatrix, TravelVehicleTypeId,
};
use veoveo_optimization_mcp::contract::decode_map_travel_model;

#[test]
fn map_package_is_the_optimization_wire_contract() {
    let package = TravelModelArtifact {
        version: veoveo_map_mcp::TravelModelArtifactVersion::V2,
        map_resource_uri: Some(MapTravelModelUri::new(
            TravelModelId::parse("travel-model-018f6c6e-7b8a-7c01-8000-000000000001").unwrap(),
        )),
        model: OptimizationTravelModel {
            location_ids: vec![
                TravelLocationId::parse("depot").unwrap(),
                TravelLocationId::parse("customer").unwrap(),
            ],
            cost_matrices: vec![matrix()],
            transit_time_matrices: vec![matrix()],
        },
    };

    let optimization = decode_map_travel_model(
        &serde_json::to_vec(&package).unwrap(),
        package.map_resource_uri.as_ref(),
    )
    .unwrap();
    assert_eq!(optimization.location_ids.len(), 2);
    let current = serde_json::to_value(&package).unwrap();
    for pointer in [
        "/mapResourceUri",
        "/model/locationIds",
        "/model/costMatrices/0/vehicleTypeId",
        "/model/costMatrices/0/unavailableCells",
    ] {
        if let Some(original) = current.pointer(pointer) {
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            let retired = match key {
                "mapResourceUri" => "map_resource_uri",
                "locationIds" => "location_ids",
                "vehicleTypeId" => "vehicle_type_id",
                "unavailableCells" => "unavailable_cells",
                _ => unreachable!(),
            };
            for mixed in [false, true] {
                let mut invalid = current.clone();
                let object = if parent.is_empty() {
                    invalid.as_object_mut().unwrap()
                } else {
                    invalid
                        .pointer_mut(parent)
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                };
                object.insert(retired.to_owned(), original.clone());
                if !mixed {
                    object.remove(key);
                }
                assert!(
                    decode_map_travel_model(
                        &serde_json::to_vec(&invalid).unwrap(),
                        package.map_resource_uri.as_ref()
                    )
                    .is_err()
                );
            }
        }
    }
    for (pointer, bad) in [
        (
            "/version",
            serde_json::json!("veoveo.ai/travel-model-artifact/v1"),
        ),
        ("/model/locationIds", serde_json::json!(["depot", "depot"])),
        ("/model/costMatrices/0/dimension", serde_json::json!(1)),
        ("/model/costMatrices/0/values/0", serde_json::json!(-1)),
        (
            "/model/costMatrices/0/unavailableCells",
            serde_json::json!([4]),
        ),
    ] {
        let mut invalid = current.clone();
        if let Some(field) = invalid.pointer_mut(pointer) {
            *field = bad;
        } else {
            invalid["model"]["costMatrices"][0]["unavailableCells"] = bad;
        }
        assert!(
            decode_map_travel_model(
                &serde_json::to_vec(&invalid).unwrap(),
                package.map_resource_uri.as_ref()
            )
            .is_err()
        );
    }
    let wrong_parent = MapTravelModelUri::new(TravelModelId::new());
    assert!(
        decode_map_travel_model(&serde_json::to_vec(&package).unwrap(), Some(&wrong_parent))
            .is_err()
    );

    assert_eq!(
        optimization.cost_matrices[0].values,
        vec![0.0, 12.0, 10.0, 0.0]
    );
}

fn matrix() -> TravelModelMatrix {
    TravelModelMatrix {
        vehicle_type_id: TravelVehicleTypeId::parse("truck").unwrap(),
        dimension: 2,
        values: vec![0.0, 12.0, 10.0, 0.0],
        unavailable_cells: Vec::new(),
    }
}

#[test]
fn map_owns_address_building_and_template_expansion() {
    use iri_string::template::simple_context::SimpleContext;
    use veoveo_types::{ResourceAddress, ResourceTemplateUri};
    for id in [
        TravelModelId::new(),
        TravelModelId::from_stable_key(b"fixture"),
    ] {
        let address = MapTravelModelUri::new(id.clone());
        assert_eq!(address.id(), &id);
        assert_eq!(MapTravelModelUri::parse(address.as_str()).unwrap(), address);
        assert_eq!(
            <MapTravelModelUri as ResourceAddress>::parse(&address.to_uri().unwrap()).unwrap(),
            address
        );
        assert_eq!(
            serde_json::from_value::<MapTravelModelUri>(serde_json::json!(address.as_str()))
                .unwrap(),
            address
        );
        let mut context = SimpleContext::new();
        context.insert("travel_model_id", id.to_string());
        assert_eq!(
            ResourceTemplateUri::new(veoveo_map_mcp::uris::TRAVEL_MODEL_TEMPLATE)
                .unwrap()
                .expand(&context)
                .unwrap(),
            address.to_uri().unwrap()
        );
        for bad in [
            format!("{}?cursor=x", address),
            format!("{}#fragment", address),
            format!("{}/extra", address),
            address.to_string().replace("map://", "optimization://"),
            address.to_string().replace("travel-model/", "matrix/"),
        ] {
            assert!(MapTravelModelUri::parse(bad).is_err());
        }
    }
    for suffix in [
        "0195dabe-7777-7abc-0def-000000000001",
        "0195dabe-7777-4abc-8def-000000000001",
        "0195DABE-7777-7ABC-8DEF-000000000001",
        "0195dabe77777abc8def000000000001",
    ] {
        assert!(TravelModelId::parse(format!("travel-model-{suffix}")).is_err());
        assert!(
            MapTravelModelUri::parse(format!("map://travel-model/travel-model-{suffix}")).is_err()
        );
    }
    for bad in [
        "map://travel-model/travel-1",
        "map://travel-model/%",
        "map://travel-model/%2F",
        "map://travel-model/..",
        "map://travel-model/travel-model%2D0195dabe-7777-7abc-8def-000000000001",
    ] {
        assert!(MapTravelModelUri::parse(bad).is_err());
    }
}

#[test]
fn travel_collection_positions_round_trip_and_reject_ambiguous_selection() {
    use iri_string::template::simple_context::SimpleContext;
    use veoveo_map_mcp::contract::{MapTravelModelCursor, MapTravelModelsUri};
    use veoveo_types::{ResourceAddress, ResourceTemplateUri, TaskId};
    let task: TaskId = "0195dabe-7777-7abc-8def-000000000001".parse().unwrap();
    let cursor = MapTravelModelCursor::new(task).unwrap();
    assert_eq!(
        MapTravelModelCursor::parse(cursor.as_str())
            .unwrap()
            .task_id(),
        task
    );
    for cursor in [None, Some(cursor)] {
        let uri = MapTravelModelsUri::new(cursor.clone());
        assert_eq!(MapTravelModelsUri::parse(uri.as_str()).unwrap(), uri);
        assert_eq!(
            serde_json::from_value::<MapTravelModelsUri>(serde_json::json!(uri.as_str())).unwrap(),
            uri
        );
        let mut context = SimpleContext::new();
        if let Some(cursor) = cursor {
            context.insert("cursor", cursor.as_str().to_owned());
        }
        assert_eq!(
            ResourceTemplateUri::new(MapTravelModelsUri::TEMPLATE)
                .unwrap()
                .expand(&context)
                .unwrap(),
            uri.to_uri().unwrap()
        );
    }
    for bad in [
        "map://travel-models/",
        "map://travel-models?",
        "map://travel-models?cursor=",
        "map://travel-models?cursor=a&cursor=a",
        "map://travel-models?ignored=1",
        "map://travel-models#fragment",
        "other://travel-models",
    ] {
        assert!(MapTravelModelsUri::parse(bad).is_err());
    }
    assert!(MapTravelModelCursor::parse("0".repeat(1025)).is_err());
    assert!(
        MapTravelModelCursor::new("0195dabe-7777-4abc-8def-000000000001".parse().unwrap()).is_err()
    );
}
