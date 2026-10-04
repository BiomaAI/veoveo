use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAP_STABLE_ID_NAMESPACE: Uuid = Uuid::from_u128(0xc15a_8bd8_ef8d_5c4e_a3aa_633e_b162_2aa8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapIdError {
    value: String,
    expected_prefix: &'static str,
}

impl fmt::Display for MapIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid map id {:?}: expected {} followed by a generated UUIDv7 or stable UUIDv5",
            self.value, self.expected_prefix
        )
    }
}

impl std::error::Error for MapIdError {}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct MapDatasetId(String);
impl MapDatasetId {
    pub const PREFIX: &'static str = "dataset-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct DatasetReleaseId(String);
impl DatasetReleaseId {
    pub const PREFIX: &'static str = "release-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct MapSourceId(String);
impl MapSourceId {
    pub const PREFIX: &'static str = "source-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct SourceFeatureId(String);
impl SourceFeatureId {
    pub const PREFIX: &'static str = "source-feature-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct RasterProductId(String);
impl RasterProductId {
    pub const PREFIX: &'static str = "raster-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct RasterDerivationId(String);
impl RasterDerivationId {
    pub const PREFIX: &'static str = "raster-derivation-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct SpatialDerivationId(String);
impl SpatialDerivationId {
    pub const PREFIX: &'static str = "spatial-derivation-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct SourcePolicyId(String);
impl SourcePolicyId {
    pub const PREFIX: &'static str = "source-policy-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct AcquisitionId(String);
impl AcquisitionId {
    pub const PREFIX: &'static str = "acquisition-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct OperationalSnapshotId(String);
impl OperationalSnapshotId {
    pub const PREFIX: &'static str = "snapshot-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct LocationId(String);
impl LocationId {
    pub const PREFIX: &'static str = "location-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct MapBoundaryId(String);
impl MapBoundaryId {
    pub const PREFIX: &'static str = "boundary-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct FacilityId(String);
impl FacilityId {
    pub const PREFIX: &'static str = "facility-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct MobilityProfileId(String);
impl MobilityProfileId {
    pub const PREFIX: &'static str = "mobility-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct RestrictionId(String);
impl RestrictionId {
    pub const PREFIX: &'static str = "restriction-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct MapGeofenceId(String);
impl MapGeofenceId {
    pub const PREFIX: &'static str = "geofence-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct RouteId(String);
impl RouteId {
    pub const PREFIX: &'static str = "route-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct RouteMatrixId(String);
impl RouteMatrixId {
    pub const PREFIX: &'static str = "matrix-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct TravelModelId(String);
impl TravelModelId {
    pub const PREFIX: &'static str = "travel-model-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct ReachableAreaId(String);
impl ReachableAreaId {
    pub const PREFIX: &'static str = "reachable-area-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct ValidationId(String);
impl ValidationId {
    pub const PREFIX: &'static str = "validation-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct MapOperationId(String);
impl MapOperationId {
    pub const PREFIX: &'static str = "map-operation-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct FeatureLayerId(String);
impl FeatureLayerId {
    pub const PREFIX: &'static str = "feature-layer-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct MapFeatureId(String);
impl MapFeatureId {
    pub const PREFIX: &'static str = "feature-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct FeatureChangeSetId(String);
impl FeatureChangeSetId {
    pub const PREFIX: &'static str = "changeset-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct FeatureSchemaRevisionId(String);
impl FeatureSchemaRevisionId {
    pub const PREFIX: &'static str = "feature-schema-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct StyleRevisionId(String);
impl StyleRevisionId {
    pub const PREFIX: &'static str = "style-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,true),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct LayerPublicationId(String);
impl LayerPublicationId {
    pub const PREFIX: &'static str = "publication-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct LayerProductId(String);
impl LayerProductId {
    pub const PREFIX: &'static str = "layer-product-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct MapCompositionId(String);
impl MapCompositionId {
    pub const PREFIX: &'static str = "composition-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=MapIdError,validate=|value| validate_map_id(value,Self::PREFIX,false),generate=|| format!("{}{}",Self::PREFIX,Uuid::now_v7()))]
pub struct MapCompositionRevisionId(String);
impl MapCompositionRevisionId {
    pub const PREFIX: &'static str = "composition-revision-";
    pub fn from_stable_key(value: &[u8]) -> Self {
        Self::parse(stable_map_id(Self::PREFIX, value))
            .expect("stable map generator produces an admitted identity")
    }
    pub fn uuid(&self) -> Uuid {
        map_uuid(self.as_str(), Self::PREFIX)
    }
}

fn validate_map_id(value: &str, prefix: &'static str, canonical: bool) -> Result<(), MapIdError> {
    let invalid = || MapIdError {
        value: value.to_owned(),
        expected_prefix: prefix,
    };
    let raw = value.strip_prefix(prefix).ok_or_else(invalid)?;
    let uuid = Uuid::parse_str(raw).map_err(|_| invalid())?;
    if !matches!(uuid.get_version_num(), 5 | 7)
        || (canonical && (uuid.get_variant() != uuid::Variant::RFC4122 || raw != uuid.to_string()))
    {
        return Err(invalid());
    }
    Ok(())
}
fn stable_map_id(prefix: &str, value: &[u8]) -> String {
    format!(
        "{}{}",
        prefix,
        Uuid::new_v5(&MAP_STABLE_ID_NAMESPACE, value)
    )
}
fn map_uuid(value: &str, prefix: &str) -> Uuid {
    Uuid::parse_str(&value[prefix.len()..]).expect("validated map id always contains a UUID")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_prefixed_uuid_v7_values() {
        let id = RouteId::new();
        assert!(id.as_str().starts_with(RouteId::PREFIX));
        assert_eq!(id.uuid().get_version_num(), 7);
        assert_eq!(RouteId::parse(id.to_string()).unwrap(), id);
    }

    #[test]
    fn ids_reject_wrong_prefix_and_uuid_version() {
        assert!(RouteId::parse(format!("location-{}", Uuid::now_v7())).is_err());
        assert!(
            RouteId::parse(format!(
                "route-{}",
                Uuid::parse_str("c5efe3f7-8e96-4e49-b4f2-36cdf383fe34").unwrap()
            ))
            .is_err()
        );
    }

    #[test]
    fn source_feature_ids_are_stable_uuid_v5_values() {
        let first = LocationId::from_stable_key(b"source-a:place:42");
        let second = LocationId::from_stable_key(b"source-a:place:42");
        assert_eq!(first, second);
        assert_eq!(first.uuid().get_version_num(), 5);
        assert_eq!(LocationId::parse(first.to_string()).unwrap(), first);
    }
}

#[cfg(test)]
mod identity_profiles {
    use super::*;
    use veoveo_types::Identity;
    fn check<I: Identity + schemars::JsonSchema>(prefix: &str, stable: &str)
    where
        I::Error: std::fmt::Debug,
    {
        let expected = Uuid::new_v5(
            &Uuid::from_u128(0xc15a_8bd8_ef8d_5c4e_a3aa_633e_b162_2aa8),
            b"qualification",
        );
        assert_eq!(stable, format!("{prefix}{expected}"));
        assert_eq!(I::parse_identity(stable).unwrap().identity_text(), stable);
        let schema = I::json_schema(&mut schemars::SchemaGenerator::default());
        assert_eq!(
            serde_json::to_value(schema).unwrap(),
            serde_json::json!({"type":"string"})
        );
    }
    #[test]
    fn every_prefix_and_stable_namespace_preserves_its_owner_mapping() {
        check::<MapDatasetId>(
            "dataset-",
            MapDatasetId::from_stable_key(b"qualification").as_str(),
        );
        check::<DatasetReleaseId>(
            "release-",
            DatasetReleaseId::from_stable_key(b"qualification").as_str(),
        );
        check::<MapSourceId>(
            "source-",
            MapSourceId::from_stable_key(b"qualification").as_str(),
        );
        check::<SourceFeatureId>(
            "source-feature-",
            SourceFeatureId::from_stable_key(b"qualification").as_str(),
        );
        check::<RasterProductId>(
            "raster-",
            RasterProductId::from_stable_key(b"qualification").as_str(),
        );
        check::<RasterDerivationId>(
            "raster-derivation-",
            RasterDerivationId::from_stable_key(b"qualification").as_str(),
        );
        check::<SpatialDerivationId>(
            "spatial-derivation-",
            SpatialDerivationId::from_stable_key(b"qualification").as_str(),
        );
        check::<SourcePolicyId>(
            "source-policy-",
            SourcePolicyId::from_stable_key(b"qualification").as_str(),
        );
        check::<AcquisitionId>(
            "acquisition-",
            AcquisitionId::from_stable_key(b"qualification").as_str(),
        );
        check::<OperationalSnapshotId>(
            "snapshot-",
            OperationalSnapshotId::from_stable_key(b"qualification").as_str(),
        );
        check::<LocationId>(
            "location-",
            LocationId::from_stable_key(b"qualification").as_str(),
        );
        check::<MapBoundaryId>(
            "boundary-",
            MapBoundaryId::from_stable_key(b"qualification").as_str(),
        );
        check::<FacilityId>(
            "facility-",
            FacilityId::from_stable_key(b"qualification").as_str(),
        );
        check::<MobilityProfileId>(
            "mobility-",
            MobilityProfileId::from_stable_key(b"qualification").as_str(),
        );
        check::<RestrictionId>(
            "restriction-",
            RestrictionId::from_stable_key(b"qualification").as_str(),
        );
        check::<MapGeofenceId>(
            "geofence-",
            MapGeofenceId::from_stable_key(b"qualification").as_str(),
        );
        check::<RouteId>(
            "route-",
            RouteId::from_stable_key(b"qualification").as_str(),
        );
        check::<RouteMatrixId>(
            "matrix-",
            RouteMatrixId::from_stable_key(b"qualification").as_str(),
        );
        check::<TravelModelId>(
            "travel-model-",
            TravelModelId::from_stable_key(b"qualification").as_str(),
        );
        check::<ReachableAreaId>(
            "reachable-area-",
            ReachableAreaId::from_stable_key(b"qualification").as_str(),
        );
        check::<ValidationId>(
            "validation-",
            ValidationId::from_stable_key(b"qualification").as_str(),
        );
        check::<MapOperationId>(
            "map-operation-",
            MapOperationId::from_stable_key(b"qualification").as_str(),
        );
        check::<FeatureLayerId>(
            "feature-layer-",
            FeatureLayerId::from_stable_key(b"qualification").as_str(),
        );
        check::<MapFeatureId>(
            "feature-",
            MapFeatureId::from_stable_key(b"qualification").as_str(),
        );
        check::<FeatureChangeSetId>(
            "changeset-",
            FeatureChangeSetId::from_stable_key(b"qualification").as_str(),
        );
        check::<FeatureSchemaRevisionId>(
            "feature-schema-",
            FeatureSchemaRevisionId::from_stable_key(b"qualification").as_str(),
        );
        check::<StyleRevisionId>(
            "style-",
            StyleRevisionId::from_stable_key(b"qualification").as_str(),
        );
        check::<LayerPublicationId>(
            "publication-",
            LayerPublicationId::from_stable_key(b"qualification").as_str(),
        );
        check::<LayerProductId>(
            "layer-product-",
            LayerProductId::from_stable_key(b"qualification").as_str(),
        );
        check::<MapCompositionId>(
            "composition-",
            MapCompositionId::from_stable_key(b"qualification").as_str(),
        );
        check::<MapCompositionRevisionId>(
            "composition-revision-",
            MapCompositionRevisionId::from_stable_key(b"qualification").as_str(),
        );
    }
    #[test]
    fn map_owner_preserves_alias_profile_without_broadening_canonical_owners() {
        let raw = "550E8400-E29B-51D4-1716-446655440000";
        let preserving = format!("source-policy-{raw}");
        let id = SourcePolicyId::parse(&preserving).unwrap();
        assert_eq!(id.as_str(), preserving);
        assert_eq!(serde_json::to_value(&id).unwrap(), preserving);
        assert!(RouteId::parse(format!("route-{raw}")).is_err());
        assert!(
            serde_json::from_value::<RouteId>(serde_json::json!(format!("route-{raw}"))).is_err()
        );
        assert_eq!(
            RouteId::schema_id(),
            "veoveo_map_mcp::contract::ids::RouteId"
        );
    }
}
