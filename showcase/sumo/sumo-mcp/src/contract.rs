mod task_kind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use task_kind::SumoTaskKind;
use veoveo_types::TaskTypeDefinition;

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
    /// Traffic-light ID from the `signals` list returned by `describe_scenario`.
    pub signal_id: String,
    /// Zero-based phase index within that signal's program.
    #[schemars(range(min = 0))]
    pub phase: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RerouteVehicleRequest {
    /// Vehicle ID from the `vehicles` list returned by `query_state`.
    pub vehicle_id: String,
    /// Destination edge ID from the `edges` list returned by `describe_scenario`.
    pub target_edge_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetEdgeSpeedRequest {
    /// Edge ID from the `edges` list returned by `describe_scenario`.
    pub edge_id: String,
    /// Speed limit in metres per second.
    #[schemars(range(min = 0.0, max = 60.0))]
    pub speed_mps: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LaneRequest {
    /// SUMO lane ID in `{edge_id}_{index}` form, where the edge ID comes from
    /// `describe_scenario` and the index counts lanes from 0.
    pub lane_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunBatchRequest {
    /// Number of simulation steps to advance.
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
    pub fn task_type(self) -> veoveo_types::TaskTypeName {
        match self {
            Self::GenerateNetwork => SumoTaskKind::GenerateNetwork.name(),
            Self::ComputeRoutes => SumoTaskKind::ComputeRoutes.name(),
            Self::OptimizeSignals => SumoTaskKind::OptimizeSignals.name(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfflineOperationRequest {
    pub kind: String,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema)]
#[serde(try_from = "OfflineOperationResultWire")]
pub struct OfflineOperationResult(veoveo_types::Checked<OfflineOperationResultWire>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OfflineOperationResultWire {
    result_uri: veoveo_artifact_contract::ArtifactUri,
    operation: OfflineOperation,
    artifact: veoveo_artifact_contract::ArtifactMetadata,
}
impl OfflineOperationResult {
    pub fn new(
        operation: OfflineOperation,
        artifact: veoveo_artifact_contract::ArtifactMetadata,
    ) -> Self {
        Self::try_from(OfflineOperationResultWire {
            result_uri: artifact.artifact_uri.clone(),
            operation,
            artifact,
        })
        .expect("offline product address derives from its admitted Artifact")
    }
    pub fn operation(&self) -> OfflineOperation {
        self.0.operation
    }
    pub fn result_uri(&self) -> &veoveo_artifact_contract::ArtifactUri {
        &self.0.result_uri
    }
    pub fn artifact(&self) -> &veoveo_artifact_contract::ArtifactMetadata {
        &self.0.artifact
    }
}
impl veoveo_types::Check for OfflineOperationResultWire {
    type Error = &'static str;
    fn check(&self) -> Result<(), Self::Error> {
        if self.result_uri != self.artifact.artifact_uri {
            return Err("offline product address differs from its Artifact");
        }
        Ok(())
    }
}
impl TryFrom<OfflineOperationResultWire> for OfflineOperationResult {
    type Error = &'static str;
    fn try_from(value: OfflineOperationResultWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl Serialize for OfflineOperationResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
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
    pub artifact_write_capability: Option<veoveo_artifact_contract::IssuedArtifactWriteCapability>,
    #[serde(default)]
    pub data_labels: std::collections::BTreeSet<veoveo_types::DataLabelId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CongestionState {
    pub congested: bool,
    pub mean_speed_mps: f64,
    pub threshold_mps: f64,
    pub simulation_time_s: f64,
}
