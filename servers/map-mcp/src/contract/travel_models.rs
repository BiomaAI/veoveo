use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactMetadata;
use veoveo_types::{PrincipalId, WorkContextId};

use super::{
    DatasetReleaseId, MapTravelModelUri, MobilityProfileId, OperationalSnapshotId,
    RouteConstraints, RouteDataPolicy, RouteEndpoint, TravelModelId,
};

pub const TRAVEL_MODEL_ARTIFACT_VERSION: &str = "veoveo.ai/travel-model-artifact/v1";
pub const MAX_TRAVEL_MODEL_LOCATIONS: usize = 128;
pub const MAX_TRAVEL_MODEL_VEHICLE_TYPES: usize = 64;
pub const MAX_TRAVEL_MODEL_CELLS: usize = 1_048_576;

#[veoveo_types::id(text(TravelKeys), error_context = "travel location id")]
#[schemars(with = "String")]
pub struct TravelLocationId(String);
#[veoveo_types::id(text(TravelKeys), error_context = "travel vehicle type id")]
#[schemars(with = "String")]
pub struct TravelVehicleTypeId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum TravelCostMetric {
    #[default]
    Duration,
    Distance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum TravelTimeModel {
    #[default]
    Static,
    InvariantLocalDeparture {
        local_time: NaiveDateTime,
    },
}

// Struct variants enforce closure even for the public unit variant.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TravelTimeModelWire {
    Static {},
    InvariantLocalDeparture { local_time: NaiveDateTime },
}

impl<'de> Deserialize<'de> for TravelTimeModel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match TravelTimeModelWire::deserialize(deserializer)? {
            TravelTimeModelWire::Static {} => Self::Static,
            TravelTimeModelWire::InvariantLocalDeparture { local_time } => {
                Self::InvariantLocalDeparture { local_time }
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TravelModelLocation {
    pub location_id: TravelLocationId,
    pub endpoint: RouteEndpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TravelModelVehicleType {
    pub vehicle_type_id: TravelVehicleTypeId,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: crate::contract::MobilityProfileVersion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
    pub map_resource_uri: Option<MapTravelModelUri>,
    pub model: OptimizationTravelModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TravelModelProfileProvenance {
    pub vehicle_type_id: TravelVehicleTypeId,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: crate::contract::MobilityProfileVersion,
    pub base_release_ids: BTreeSet<DatasetReleaseId>,
    pub operational_snapshot_id: OperationalSnapshotId,
    pub planner_version: String,
    pub cost_model_version: String,
    pub matrix_algorithm: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "TravelModelRecord")]
pub struct TravelModelRecordValue {
    pub travel_model_id: TravelModelId,
    pub travel_model_uri: MapTravelModelUri,
    pub manifest_uri: veoveo_artifact_contract::ArtifactUri,
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

impl TravelModelRecordValue {
    pub fn validate_identity(&self) -> Result<(), TravelModelContractError> {
        if self.travel_model_uri.id() != &self.travel_model_id
            || !matches!(
                self.manifest_uri.address(),
                veoveo_artifact_contract::ArtifactAddress::Plane(_)
            )
            || self.manifest_uri.artifact_id() != self.artifact.artifact_id()
        {
            return Err(TravelModelContractError::InvalidIdentity);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum TravelModelContractError {
    #[error("travel-model record identities or manifest parent disagree")]
    InvalidIdentity,
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

fn validate_travel_key(value: &str, label: &'static str) -> Result<(), TravelModelContractError> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':'))
        })
    {
        return Err(TravelModelContractError::InvalidKey(label));
    }
    Ok(())
}

#[doc(hidden)]
pub struct TravelKeys;
impl veoveo_types::IdProfile for TravelKeys {
    type Error = TravelModelContractError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, metadata| {
            validate_travel_key(value, metadata.error_context)
        });
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TravelModelRecord(veoveo_types::Checked<TravelModelRecordValue>);
impl TravelModelRecord {
    pub fn new(value: TravelModelRecordValue) -> Result<Self, TravelModelContractError> {
        veoveo_types::Checked::new(value).map(Self)
    }
    pub fn into_value(self) -> TravelModelRecordValue {
        self.0.into_inner()
    }
}
impl std::ops::Deref for TravelModelRecord {
    type Target = TravelModelRecordValue;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl veoveo_types::Check for TravelModelRecordValue {
    type Error = TravelModelContractError;
    fn check(&self) -> Result<(), Self::Error> {
        self.validate_identity()
    }
}
impl JsonSchema for TravelModelRecord {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        TravelModelRecordValue::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        TravelModelRecordValue::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        TravelModelRecordValue::json_schema(generator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_model_unit_and_data_variants_reject_unknown_keys() {
        let schema = serde_json::to_value(schemars::schema_for!(TravelTimeModel)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for wire in [
            serde_json::json!({"kind": "static"}),
            serde_json::json!({"kind": "invariant_local_departure", "local_time": "2026-01-01T12:00:00"}),
        ] {
            let value: TravelTimeModel = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(serde_json::to_value(value).unwrap(), wire);
            assert!(validator.is_valid(&wire));
            let mut invalid = wire;
            invalid["unexpected"] = serde_json::json!(true);
            assert!(serde_json::from_value::<TravelTimeModel>(invalid.clone()).is_err());
            assert!(!validator.is_valid(&invalid));
        }
    }

    #[test]
    fn controlled_keys_reject_path_segments() {
        assert!(TravelLocationId::parse("depot-1").is_ok());
        assert!(TravelLocationId::parse("../depot").is_err());
        assert!(TravelVehicleTypeId::parse("truck:heavy").is_ok());
    }
}
