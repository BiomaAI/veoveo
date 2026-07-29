use std::{collections::BTreeSet, fmt};

use chrono::{DateTime, NaiveDateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{ArtifactMetadata, PrincipalId, WorkContextId};

use super::{
    DatasetReleaseId, MobilityProfileId, OperationalSnapshotId, RouteConstraints, RouteDataPolicy,
    RouteEndpoint, TravelModelId,
};

pub const TRAVEL_MODEL_ARTIFACT_VERSION: &str = "veoveo.io/travel-model-artifact/v1";
pub const MAX_TRAVEL_MODEL_LOCATIONS: usize = 128;
pub const MAX_TRAVEL_MODEL_VEHICLE_TYPES: usize = 64;
pub const MAX_TRAVEL_MODEL_CELLS: usize = 1_048_576;

macro_rules! travel_key {
    ($name:ident, $label:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        #[schemars(with = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, TravelModelContractError> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > 128
                    || value.trim() != value
                    || value.chars().any(|character| {
                        !(character.is_ascii_alphanumeric()
                            || matches!(character, '-' | '_' | '.' | ':'))
                    })
                {
                    return Err(TravelModelContractError::InvalidKey($label));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = TravelModelContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

travel_key!(TravelLocationId, "travel location id");
travel_key!(TravelVehicleTypeId, "travel vehicle type id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum TravelCostMetric {
    #[default]
    Duration,
    Distance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TravelTimeModel {
    #[default]
    Static,
    InvariantLocalDeparture {
        local_time: NaiveDateTime,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TravelModelLocation {
    pub location_id: TravelLocationId,
    pub endpoint: RouteEndpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TravelModelVehicleType {
    pub vehicle_type_id: TravelVehicleTypeId,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BuildTravelModelRequest {
    pub locations: Vec<TravelModelLocation>,
    pub vehicle_types: Vec<TravelModelVehicleType>,
    pub departure_time: DateTime<Utc>,
    #[serde(default)]
    pub cost_metric: TravelCostMetric,
    #[serde(default)]
    pub time_model: TravelTimeModel,
    #[serde(default = "default_prioritize_bidirectional")]
    pub prioritize_bidirectional: bool,
    pub constraints: RouteConstraints,
    pub data_policy: RouteDataPolicy,
}

impl BuildTravelModelRequest {
    pub fn validate(&self) -> Result<(), TravelModelContractError> {
        if self.locations.is_empty() || self.locations.len() > MAX_TRAVEL_MODEL_LOCATIONS {
            return Err(TravelModelContractError::InvalidSize(format!(
                "locations must contain 1..={MAX_TRAVEL_MODEL_LOCATIONS} entries"
            )));
        }
        if self.vehicle_types.is_empty()
            || self.vehicle_types.len() > MAX_TRAVEL_MODEL_VEHICLE_TYPES
        {
            return Err(TravelModelContractError::InvalidSize(format!(
                "vehicle_types must contain 1..={MAX_TRAVEL_MODEL_VEHICLE_TYPES} entries"
            )));
        }
        let cells = self
            .locations
            .len()
            .checked_mul(self.locations.len())
            .and_then(|value| value.checked_mul(self.vehicle_types.len()))
            .ok_or_else(|| {
                TravelModelContractError::InvalidSize(
                    "travel-model matrix dimensions overflow".to_owned(),
                )
            })?;
        if cells > MAX_TRAVEL_MODEL_CELLS {
            return Err(TravelModelContractError::InvalidSize(format!(
                "travel model contains {cells} cells and exceeds {MAX_TRAVEL_MODEL_CELLS}"
            )));
        }
        let location_ids = self
            .locations
            .iter()
            .map(|location| &location.location_id)
            .collect::<BTreeSet<_>>();
        if location_ids.len() != self.locations.len() {
            return Err(TravelModelContractError::DuplicateKey(
                "location_id".to_owned(),
            ));
        }
        let vehicle_type_ids = self
            .vehicle_types
            .iter()
            .map(|vehicle_type| &vehicle_type.vehicle_type_id)
            .collect::<BTreeSet<_>>();
        if vehicle_type_ids.len() != self.vehicle_types.len() {
            return Err(TravelModelContractError::DuplicateKey(
                "vehicle_type_id".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TravelModelMatrix {
    pub vehicle_type_id: TravelVehicleTypeId,
    pub dimension: u32,
    pub values: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unavailable_cells: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationTravelModel {
    pub location_ids: Vec<TravelLocationId>,
    pub cost_matrices: Vec<TravelModelMatrix>,
    pub transit_time_matrices: Vec<TravelModelMatrix>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TravelModelArtifact {
    pub version: String,
    pub map_resource_uri: Option<String>,
    pub model: OptimizationTravelModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TravelModelProfileProvenance {
    pub vehicle_type_id: TravelVehicleTypeId,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: u64,
    pub base_release_ids: BTreeSet<DatasetReleaseId>,
    pub operational_snapshot_id: OperationalSnapshotId,
    pub planner_version: String,
    pub cost_model_version: String,
    pub matrix_algorithm: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TravelModelRecord {
    pub travel_model_id: TravelModelId,
    pub travel_model_uri: String,
    pub manifest_uri: String,
    pub artifact: ArtifactMetadata,
    pub cost_metric: TravelCostMetric,
    pub time_model: TravelTimeModel,
    pub location_count: u32,
    pub vehicle_type_count: u32,
    pub unavailable_cell_count: u64,
    pub profiles: Vec<TravelModelProfileProvenance>,
    pub created_by: PrincipalId,
    pub work_context: WorkContextId,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum TravelModelContractError {
    #[error("invalid {0}")]
    InvalidKey(&'static str),
    #[error("{0}")]
    InvalidSize(String),
    #[error("duplicate {0}")]
    DuplicateKey(String),
}

const fn default_prioritize_bidirectional() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_keys_reject_path_segments() {
        assert!(TravelLocationId::new("depot-1").is_ok());
        assert!(TravelLocationId::new("../depot").is_err());
        assert!(TravelVehicleTypeId::new("truck:heavy").is_ok());
    }
}
