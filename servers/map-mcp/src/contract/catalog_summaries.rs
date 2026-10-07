//! Portable catalog metadata binds each selected identity to its resource address.
use super::{
    MapMatrixUri, MapRelationshipError, MapRouteUri, MobilityProfileId, MobilityProfileVersion,
    RouteId, RouteMatrixId, RouteStatus,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[schemars(rename = "RouteSummary")]
pub struct RouteSummaryBuilder {
    pub route_id: RouteId,
    pub resource_uri: MapRouteUri,
    pub status: RouteStatus,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: MobilityProfileVersion,
    #[schemars(with = "veoveo_types::ChronoUtcTimestampSchema")]
    pub departure_time: DateTime<Utc>,
    #[schemars(with = "Option<veoveo_types::ChronoUtcTimestampSchema>")]
    pub arrival_time: Option<DateTime<Utc>>,
    #[schemars(with = "veoveo_types::ChronoUtcTimestampSchema")]
    pub created_at: DateTime<Utc>,
}

impl veoveo_types::Check for RouteSummaryBuilder {
    type Error = MapRelationshipError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.resource_uri.id() != &self.route_id {
            return Err(MapRelationshipError);
        }
        Ok(())
    }
}
impl RouteSummaryBuilder {
    pub fn build(self) -> Result<RouteSummary, MapRelationshipError> {
        veoveo_types::Checked::new(self).map(RouteSummary)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(with = "RouteSummaryBuilder")]
pub struct RouteSummary(veoveo_types::Checked<RouteSummaryBuilder>);
impl std::ops::Deref for RouteSummary {
    type Target = RouteSummaryBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[schemars(rename = "MatrixSummary")]
pub struct MatrixSummaryBuilder {
    pub matrix_id: RouteMatrixId,
    pub resource_uri: MapMatrixUri,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: MobilityProfileVersion,
    #[schemars(with = "veoveo_types::ChronoUtcTimestampSchema")]
    pub created_at: DateTime<Utc>,
}
impl veoveo_types::Check for MatrixSummaryBuilder {
    type Error = MapRelationshipError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.resource_uri.id() != &self.matrix_id {
            return Err(MapRelationshipError);
        }
        Ok(())
    }
}
impl MatrixSummaryBuilder {
    pub fn build(self) -> Result<MatrixSummary, MapRelationshipError> {
        veoveo_types::Checked::new(self).map(MatrixSummary)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(with = "MatrixSummaryBuilder")]
pub struct MatrixSummary(veoveo_types::Checked<MatrixSummaryBuilder>);
impl std::ops::Deref for MatrixSummary {
    type Target = MatrixSummaryBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use veoveo_types::{ResourceAddress, ResourceTemplateUri};

    fn check_wire<T: serde::de::DeserializeOwned + Serialize + JsonSchema>(wire: Value) {
        let restored: T = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), wire);
        let schema = schemars::schema_for!(T);
        assert_eq!(schema.as_value()["additionalProperties"], false);
        assert!(
            jsonschema::validator_for(schema.as_value())
                .unwrap()
                .is_valid(&wire)
        );
        for key in wire.as_object().unwrap().keys() {
            let retired: String = key
                .chars()
                .flat_map(|ch| {
                    if ch.is_ascii_uppercase() {
                        vec!['_', ch.to_ascii_lowercase()]
                    } else {
                        vec![ch]
                    }
                })
                .collect();
            if retired == *key {
                continue;
            }
            for mode in ["replacement", "mixed", "conflicting"] {
                let mut invalid = wire.clone();
                invalid[&retired] = if mode == "conflicting" {
                    Value::Null
                } else {
                    wire[key].clone()
                };
                if mode == "replacement" {
                    invalid.as_object_mut().unwrap().remove(key);
                }
                assert!(
                    serde_json::from_value::<T>(invalid).is_err(),
                    "{key}/{mode}"
                );
            }
        }
        let mut invalid = wire;
        invalid["undeclared"] = json!(true);
        assert!(serde_json::from_value::<T>(invalid).is_err());
    }

    #[test]
    fn summaries_bind_typed_addresses_and_admit_only_current_members() {
        let route_id = RouteId::new();
        let matrix_id = RouteMatrixId::new();
        let profile = MobilityProfileId::new();
        let now = "2026-10-06T00:00:00Z".parse().unwrap();
        let route = RouteSummaryBuilder {
            resource_uri: MapRouteUri::new(route_id.clone()),
            route_id,
            status: RouteStatus::Unavailable,
            mobility_profile_id: profile.clone(),
            mobility_profile_version: MobilityProfileVersion::FIRST,
            departure_time: now,
            arrival_time: None,
            created_at: now,
        };
        let matrix = MatrixSummaryBuilder {
            resource_uri: MapMatrixUri::new(matrix_id.clone()),
            matrix_id: matrix_id.clone(),
            mobility_profile_id: profile,
            mobility_profile_version: MobilityProfileVersion::FIRST,
            created_at: now,
        };
        let route_wire = serde_json::to_value(route.clone().build().unwrap()).unwrap();
        assert_eq!(route_wire["arrivalTime"], Value::Null);
        check_wire::<RouteSummary>(route_wire.clone());
        let matrix_wire = serde_json::to_value(matrix.clone().build().unwrap()).unwrap();
        check_wire::<MatrixSummary>(matrix_wire.clone());
        // Actual Chrono serialization includes signed years and leap-second nanoseconds.
        for timestamp in [
            "2016-12-31T23:59:60.999999999Z",
            "-0001-12-31T23:59:59.999999999Z",
            "+10000-01-01T00:00:00.000000001Z",
            "-262143-01-01T00:00:00Z",
            "+262142-12-31T23:59:59.999999999Z",
        ] {
            let timestamp: DateTime<Utc> = timestamp.parse().unwrap();
            let mut route = route.clone();
            route.departure_time = timestamp;
            route.arrival_time = Some(timestamp);
            route.created_at = timestamp;
            check_wire::<RouteSummary>(serde_json::to_value(route.build().unwrap()).unwrap());
            let mut matrix = matrix.clone();
            matrix.created_at = timestamp;
            check_wire::<MatrixSummary>(serde_json::to_value(matrix.build().unwrap()).unwrap());
        }
        let mut wrong_route = route;
        wrong_route.resource_uri = MapRouteUri::new(RouteId::new());
        assert!(wrong_route.build().is_err());
        let mut wrong_route_wire = route_wire;
        wrong_route_wire["resourceUri"] = json!(MapRouteUri::new(RouteId::new()));
        assert!(serde_json::from_value::<RouteSummary>(wrong_route_wire).is_err());
        let mut wrong_matrix = matrix;
        wrong_matrix.resource_uri = MapMatrixUri::new(RouteMatrixId::new());
        assert!(wrong_matrix.build().is_err());
        let mut wrong_matrix_wire = matrix_wire;
        wrong_matrix_wire["resourceUri"] = json!(MapMatrixUri::new(RouteMatrixId::new()));
        assert!(serde_json::from_value::<MatrixSummary>(wrong_matrix_wire).is_err());
        let address = MapMatrixUri::new(matrix_id.clone());
        assert_eq!(
            address.to_uri().unwrap(),
            super::super::MapResource::Matrix {
                id: matrix_id.clone()
            }
            .to_uri()
        );
        assert_eq!(MapMatrixUri::parse(address.as_str()).unwrap(), address);
        let expanded = ResourceTemplateUri::new(MapMatrixUri::TEMPLATE)
            .unwrap()
            .expand_scalars(&[("matrix_id".to_string(), matrix_id.to_string())].into())
            .unwrap();
        assert_eq!(expanded, address.to_uri().unwrap());
        for invalid in ["map://matrix/invalid", "map://route/invalid"] {
            assert!(MapMatrixUri::parse(invalid).is_err());
        }
    }
}
