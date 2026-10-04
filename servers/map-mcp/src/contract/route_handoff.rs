//! Map owns handoff structure and provenance; consumers add their actuation policy.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_types::Sha256Digest;

use super::{
    DatasetReleaseId, MapMobilityProfileUri, MapRouteUri, OperationalSnapshotId, RestrictionId,
    RouteStatus, ValidationId, Wgs84Position,
};

pub const MAP_ROUTE_HANDOFF_SCHEMA: &str = "veoveo.ai/map-route-handoff/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum MapRouteHandoffSchema {
    #[serde(rename = "veoveo.ai/map-route-handoff/v1")]
    V1,
}

/// Construction inputs. `build` checks the relationships before a consumer can use them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapRouteHandoffBuilder {
    pub schema_profile: MapRouteHandoffSchema,
    pub route_uri: MapRouteUri,
    #[serde(with = "veoveo_types::sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub route_digest_sha256: Sha256Digest,
    #[schemars(schema_with = "handoff_status_schema")]
    pub route_status: RouteStatus,
    pub mobility_profile_uri: MapMobilityProfileUri,
    #[schemars(length(min = 2, max = 10_000))]
    pub path: Vec<Wgs84Position>,
    pub validation_id: ValidationId,
    pub validated_at: DateTime<Utc>,
    pub operational_snapshot_id: OperationalSnapshotId,
    #[schemars(length(min = 1))]
    pub base_release_ids: Vec<DatasetReleaseId>,
    pub restriction_ids: Vec<RestrictionId>,
    pub prepared_at: DateTime<Utc>,
}

fn handoff_status_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type": "string", "enum": ["validated", "planning_advisory"]})
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MapRouteHandoffError {
    #[error("a Map handoff requires a validated or planning-advisory route")]
    Status,
    #[error("a Map handoff requires 2..=10000 valid positions with distinct consecutive points")]
    Path,
    #[error(
        "a Map handoff requires unique release and restriction identities and at least one release"
    )]
    Provenance,
    #[error("a Map handoff cannot be prepared before its validation")]
    Time,
}

impl MapRouteHandoffBuilder {
    pub fn build(self) -> Result<MapRouteHandoff, MapRouteHandoffError> {
        if !matches!(
            self.route_status,
            RouteStatus::Validated | RouteStatus::PlanningAdvisory
        ) {
            return Err(MapRouteHandoffError::Status);
        }
        if !(2..=10_000).contains(&self.path.len())
            || self
                .path
                .iter()
                .any(|position| position.validate().is_err())
            || self.path.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(MapRouteHandoffError::Path);
        }
        if self.base_release_ids.is_empty()
            || self.base_release_ids.iter().collect::<BTreeSet<_>>().len()
                != self.base_release_ids.len()
            || self.restriction_ids.iter().collect::<BTreeSet<_>>().len()
                != self.restriction_ids.len()
        {
            return Err(MapRouteHandoffError::Provenance);
        }
        if self.prepared_at < self.validated_at {
            return Err(MapRouteHandoffError::Time);
        }
        Ok(MapRouteHandoff(self))
    }
}

/// A checked, execution-neutral Map route. It conveys no authority to execute a vehicle.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct MapRouteHandoff(MapRouteHandoffBuilder);

impl MapRouteHandoff {
    pub fn route_uri(&self) -> &MapRouteUri {
        &self.0.route_uri
    }
    pub fn route_digest_sha256(&self) -> &Sha256Digest {
        &self.0.route_digest_sha256
    }
    pub fn route_status(&self) -> RouteStatus {
        self.0.route_status
    }
    pub fn mobility_profile_uri(&self) -> &MapMobilityProfileUri {
        &self.0.mobility_profile_uri
    }
    pub fn path(&self) -> &[Wgs84Position] {
        &self.0.path
    }
    pub fn validation_id(&self) -> &ValidationId {
        &self.0.validation_id
    }
    pub fn validated_at(&self) -> DateTime<Utc> {
        self.0.validated_at
    }
    pub fn operational_snapshot_id(&self) -> &OperationalSnapshotId {
        &self.0.operational_snapshot_id
    }
    pub fn base_release_ids(&self) -> &[DatasetReleaseId] {
        &self.0.base_release_ids
    }
    pub fn restriction_ids(&self) -> &[RestrictionId] {
        &self.0.restriction_ids
    }
    pub fn prepared_at(&self) -> DateTime<Utc> {
        self.0.prepared_at
    }
    pub fn into_builder(self) -> MapRouteHandoffBuilder {
        self.0
    }
}

impl<'de> Deserialize<'de> for MapRouteHandoff {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        MapRouteHandoffBuilder::deserialize(deserializer)?
            .build()
            .map_err(serde::de::Error::custom)
    }
}
