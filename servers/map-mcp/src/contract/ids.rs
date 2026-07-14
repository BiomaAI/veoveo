use std::{fmt, str::FromStr};

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

macro_rules! map_id {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            pub fn new() -> Self {
                Self(format!("{}{}", Self::PREFIX, Uuid::now_v7()))
            }

            pub fn from_stable_key(value: &[u8]) -> Self {
                Self(format!(
                    "{}{}",
                    Self::PREFIX,
                    Uuid::new_v5(&MAP_STABLE_ID_NAMESPACE, value)
                ))
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, MapIdError> {
                let value = value.into();
                let Some(raw) = value.strip_prefix(Self::PREFIX) else {
                    return Err(MapIdError {
                        value,
                        expected_prefix: Self::PREFIX,
                    });
                };
                let uuid = Uuid::parse_str(raw).map_err(|_| MapIdError {
                    value: value.clone(),
                    expected_prefix: Self::PREFIX,
                })?;
                if !matches!(uuid.get_version_num(), 5 | 7) {
                    return Err(MapIdError {
                        value,
                        expected_prefix: Self::PREFIX,
                    });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn uuid(&self) -> Uuid {
                Uuid::parse_str(&self.0[Self::PREFIX.len()..])
                    .expect("validated map id always contains a UUID")
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = MapIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = MapIdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

map_id!(MapDatasetId, "dataset-");
map_id!(DatasetReleaseId, "release-");
map_id!(MapSourceId, "source-");
map_id!(SourcePolicyId, "source-policy-");
map_id!(AcquisitionId, "acquisition-");
map_id!(OperationalSnapshotId, "snapshot-");
map_id!(LocationId, "location-");
map_id!(MapBoundaryId, "boundary-");
map_id!(FacilityId, "facility-");
map_id!(MobilityProfileId, "mobility-");
map_id!(RestrictionId, "restriction-");
map_id!(MapGeofenceId, "geofence-");
map_id!(RouteId, "route-");
map_id!(RouteMatrixId, "matrix-");
map_id!(ReachableAreaId, "reachable-area-");
map_id!(ValidationId, "validation-");
map_id!(MapOperationId, "map-operation-");

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
        assert!(RouteId::parse(format!("route-{}", Uuid::new_v4())).is_err());
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
