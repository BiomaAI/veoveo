//! Owner persistence table declarations for LIVE and changefeed observation.
use veoveo_modules::{ChangefeedRetention, ObservationReplay, ObservationTable, TableName};

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum MapObservationTable {
    #[vocabulary(rename = "map_source")]
    MapSource,
    #[vocabulary(rename = "map_dataset_release")]
    MapDatasetRelease,
    #[vocabulary(rename = "map_active_release")]
    MapActiveRelease,
    #[vocabulary(rename = "map_mobility_profile")]
    MapMobilityProfile,
    #[vocabulary(rename = "map_restriction")]
    MapRestriction,
    #[vocabulary(rename = "map_operational_snapshot")]
    MapOperationalSnapshot,
    #[vocabulary(rename = "map_route")]
    MapRoute,
    #[vocabulary(rename = "map_route_dependency")]
    MapRouteDependency,
    #[vocabulary(rename = "map_route_matrix")]
    MapRouteMatrix,
    #[vocabulary(rename = "map_acquisition")]
    MapAcquisition,
    #[vocabulary(rename = "map_derivation")]
    MapDerivation,
    #[vocabulary(rename = "map_feature_layer")]
    MapFeatureLayer,
    #[vocabulary(rename = "map_feature_schema_revision")]
    MapFeatureSchemaRevision,
    #[vocabulary(rename = "map_style_revision")]
    MapStyleRevision,
    #[vocabulary(rename = "map_feature_head")]
    MapFeatureHead,
    #[vocabulary(rename = "map_feature_revision")]
    MapFeatureRevision,
    #[vocabulary(rename = "map_feature_changeset")]
    MapFeatureChangeset,
    #[vocabulary(rename = "map_layer_publication")]
    MapLayerPublication,
    #[vocabulary(rename = "map_layer_product")]
    MapLayerProduct,
    #[vocabulary(rename = "map_composition")]
    MapComposition,
    #[vocabulary(rename = "map_composition_revision")]
    MapCompositionRevision,
}

impl From<MapObservationTable> for ObservationTable {
    fn from(table: MapObservationTable) -> Self {
        Self::new(
            TableName::new(table.as_str()).expect("owner table declaration"),
            ObservationReplay::Changefeed(
                ChangefeedRetention::from_days(30).expect("qualified retention"),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declared_observation_names_pass_identifier_admission() {
        assert!("task".parse::<MapObservationTable>().is_err());
        assert!(
            "injected; DELETE task"
                .parse::<MapObservationTable>()
                .is_err()
        );
        for &table in MapObservationTable::ALL {
            let observed = ObservationTable::from(table);
            assert_eq!(observed.as_str(), table.as_str());
            assert_eq!(
                table.as_str().parse::<MapObservationTable>().unwrap(),
                table
            );
        }
    }
}
