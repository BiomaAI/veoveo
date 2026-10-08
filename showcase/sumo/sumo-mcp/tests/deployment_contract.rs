use std::{fs, path::PathBuf};

use serde_json::Value;
use veoveo_mcp_contract::GatewayControlPlane;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .expect("SUMO MCP crate lives under <repository>/showcase/sumo")
        .to_owned()
}

#[test]
fn sumo_control_plane_satisfies_the_gateway_contract() {
    let path = repository_root().join("showcase/sumo/deploy/gateway.json");
    let text = fs::read_to_string(&path).expect("read SUMO control plane");
    serde_json::from_str::<GatewayControlPlane>(&text)
        .expect("decode SUMO control plane")
        .validate(&veoveo_gateway_catalog::registry().unwrap())
        .expect("validate SUMO control plane");

    assert!(
        !text.contains("\"notifications\""),
        "SUMO still uses the generic notification capability"
    );
    let value: Value = serde_json::from_str(&text).expect("decode SUMO control plane as JSON");
    for server in value["servers"].as_array().expect("servers array") {
        let capabilities = server["capabilities"]
            .as_object()
            .expect("capability object");
        assert!(
            capabilities
                .get("resourcesListChanged")
                .and_then(Value::as_bool)
                .is_some(),
            "current capability field must be advertised"
        );
        assert!(!capabilities.contains_key("resources_list_changed"));
        if capabilities
            .get("resourcesListChanged")
            .and_then(Value::as_bool)
            == Some(true)
        {
            assert_eq!(
                capabilities.get("resources").and_then(Value::as_bool),
                Some(true),
                "{} claims resource list changes without resources",
                server["slug"]
            );
        }
    }
}

fn qualify_wire<T>(wire: Value, retired: &[(&str, &str)])
where
    T: serde::de::DeserializeOwned + serde::Serialize + schemars::JsonSchema,
{
    let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(schema["additionalProperties"], false);
    let names: std::collections::BTreeSet<_> = wire.as_object().unwrap().keys().collect();
    assert_eq!(
        properties.keys().collect::<std::collections::BTreeSet<_>>(),
        names
    );
    let admitted: T = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(admitted).unwrap(), wire);
    for required in schema["required"].as_array().unwrap() {
        let mut missing = wire.clone();
        missing
            .as_object_mut()
            .unwrap()
            .remove(required.as_str().unwrap());
        assert!(serde_json::from_value::<T>(missing).is_err());
    }
    for (current, old) in retired {
        assert!(properties.contains_key(*current));
        assert!(!properties.contains_key(*old));
        let mut mixed = wire.clone();
        mixed[*old] = mixed[*current].clone();
        assert!(serde_json::from_value::<T>(mixed.clone()).is_err());
        mixed.as_object_mut().unwrap().remove(*current);
        assert!(serde_json::from_value::<T>(mixed).is_err());
    }
    let mut unknown = wire;
    unknown["unrecognized"] = serde_json::json!(true);
    assert!(serde_json::from_value::<T>(unknown).is_err());
}

#[test]
fn sumo_public_contract_admits_current_wire_and_refuses_retired_mixed_and_missing_fields() {
    use veoveo_sumo_mcp::contract::*;
    use veoveo_types::TaskTypeDefinition;
    let vehicle = serde_json::json!({"id":"vehicle","latitude":1.0,"longitude":2.0,"speedMps":3.0,"edgeId":"edge","headingDegrees":4.0,"xM":5.0,"yM":6.0,"lengthM":7.0,"widthM":8.0,"heightM":9.0,"vehicleClass":"passenger"});
    qualify_wire::<Vehicle>(
        vehicle.clone(),
        &[
            ("speedMps", "speed_mps"),
            ("edgeId", "edge_id"),
            ("headingDegrees", "heading_degrees"),
            ("xM", "x_m"),
            ("yM", "y_m"),
            ("lengthM", "length_m"),
            ("widthM", "width_m"),
            ("heightM", "height_m"),
            ("vehicleClass", "vehicle_class"),
        ],
    );
    qualify_wire::<TrafficState>(
        serde_json::json!({"simulationTimeS":1.0,"vehicleCount":1,"meanSpeedMps":3.0,"vehicles":[vehicle],"signals":[{"id":"signal","phase":1}]}),
        &[
            ("simulationTimeS", "simulation_time_s"),
            ("vehicleCount", "vehicle_count"),
            ("meanSpeedMps", "mean_speed_mps"),
        ],
    );
    qualify_wire::<Scenario>(
        serde_json::json!({"name":"fixture","edgeCount":1,"signalCount":1,"edges":["edge"],"signals":["signal"],"originLatitude":1.0,"originLongitude":2.0}),
        &[
            ("edgeCount", "edge_count"),
            ("signalCount", "signal_count"),
            ("originLatitude", "origin_latitude"),
            ("originLongitude", "origin_longitude"),
        ],
    );
    qualify_wire::<SetSignalPhaseRequest>(
        serde_json::json!({"signalId":"signal","phase":0}),
        &[("signalId", "signal_id")],
    );
    qualify_wire::<RerouteVehicleRequest>(
        serde_json::json!({"vehicleId":"vehicle","targetEdgeId":"edge"}),
        &[
            ("vehicleId", "vehicle_id"),
            ("targetEdgeId", "target_edge_id"),
        ],
    );
    qualify_wire::<SetEdgeSpeedRequest>(
        serde_json::json!({"edgeId":"edge","speedMps":8.0}),
        &[("edgeId", "edge_id"), ("speedMps", "speed_mps")],
    );
    qualify_wire::<LaneRequest>(
        serde_json::json!({"laneId":"edge_0"}),
        &[("laneId", "lane_id")],
    );
    qualify_wire::<RunBatchResult>(
        serde_json::json!({"stepsAdvanced":50,"finalSimulationTimeS":50.0,"minimumMeanSpeedMps":2.0,"congestionDetected":true}),
        &[
            ("stepsAdvanced", "steps_advanced"),
            ("finalSimulationTimeS", "final_simulation_time_s"),
            ("minimumMeanSpeedMps", "minimum_mean_speed_mps"),
            ("congestionDetected", "congestion_detected"),
        ],
    );
    qualify_wire::<CongestionState>(
        serde_json::json!({"congested":true,"meanSpeedMps":2.0,"thresholdMps":5.0,"simulationTimeS":50.0}),
        &[
            ("meanSpeedMps", "mean_speed_mps"),
            ("thresholdMps", "threshold_mps"),
            ("simulationTimeS", "simulation_time_s"),
        ],
    );
    assert_eq!(SumoTaskKind::RunBatch.name().as_str(), "run_batch");
}
