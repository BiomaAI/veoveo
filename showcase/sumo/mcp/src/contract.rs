use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Vehicle {
    pub id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub speed_mps: f64,
    pub edge_id: String,
    pub heading_degrees: f64,
    pub x_m: f64,
    pub y_m: f64,
    pub length_m: f64,
    pub width_m: f64,
    pub height_m: f64,
    pub vehicle_class: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Signal {
    pub id: String,
    pub phase: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrafficState {
    pub simulation_time_s: f64,
    pub vehicle_count: usize,
    pub mean_speed_mps: f64,
    pub vehicles: Vec<Vehicle>,
    pub signals: Vec<Signal>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    pub edge_count: usize,
    pub signal_count: usize,
    pub edges: Vec<String>,
    pub signals: Vec<String>,
    pub origin_latitude: f64,
    pub origin_longitude: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgement {
    pub applied: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetSignalPhaseRequest {
    pub signal_id: String,
    #[schemars(range(min = 0))]
    pub phase: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RerouteVehicleRequest {
    pub vehicle_id: String,
    pub target_edge_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetEdgeSpeedRequest {
    pub edge_id: String,
    #[schemars(range(min = 0.0, max = 60.0))]
    pub speed_mps: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LaneRequest {
    pub lane_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunBatchRequest {
    #[schemars(range(min = 1, max = 100_000))]
    pub steps: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunBatchResult {
    pub steps_advanced: u32,
    pub final_simulation_time_s: f64,
    pub minimum_mean_speed_mps: f64,
    pub congestion_detected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OfflineOperation {
    GenerateNetwork,
    ComputeRoutes,
    OptimizeSignals,
}

impl OfflineOperation {
    pub const fn task_type(self) -> &'static str {
        match self {
            Self::GenerateNetwork => "generate_network",
            Self::ComputeRoutes => "compute_routes",
            Self::OptimizeSignals => "optimize_signals",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfflineOperationRequest {
    pub kind: String,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfflineOperationResult {
    pub operation: OfflineOperation,
    pub artifact: veoveo_mcp_contract::ArtifactMetadata,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", content = "input", rename_all = "snake_case")]
pub enum DurableOperation {
    RunBatch(RunBatchRequest),
    GenerateNetwork(OfflineOperationRequest),
    ComputeRoutes(OfflineOperationRequest),
    OptimizeSignals(OfflineOperationRequest),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DurableTaskRequest {
    pub operation: DurableOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_write_capability: Option<veoveo_mcp_contract::IssuedArtifactWriteCapability>,
    #[serde(default)]
    pub data_labels: std::collections::BTreeSet<veoveo_mcp_contract::DataLabelId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CongestionState {
    pub congested: bool,
    pub mean_speed_mps: f64,
    pub threshold_mps: f64,
    pub simulation_time_s: f64,
}
