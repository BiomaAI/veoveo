#[cfg(test)]
use schemars::JsonSchema;
use std::fmt;
use uuid::Uuid;
use veoveo_types::{
    FreshId, IdFailure, IdGeneration, IdGrammar, IdMetadata, IdProfile, IdProfileSpec, UuidGrammar,
    UuidSpelling, UuidVariant,
};

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

fn map_id_error(value: &str, metadata: IdMetadata, _: IdFailure) -> MapIdError {
    MapIdError {
        value: value.to_owned(),
        expected_prefix: metadata.prefix,
    }
}
#[doc(hidden)]
pub struct MapIds;
impl IdProfile for MapIds {
    type Error = MapIdError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        generation: IdGeneration {
            fresh: FreshId::UuidV7,
            stable_v5_namespace: Some(MAP_STABLE_ID_NAMESPACE),
        },
        ..IdProfileSpec::uuid(UuidGrammar::canonical(&[5, 7]), map_id_error)
    };
}
#[doc(hidden)]
pub struct MapAliasIds;
impl IdProfile for MapAliasIds {
    type Error = MapIdError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        grammar: IdGrammar::Uuid(
            UuidGrammar {
                versions: &[5, 7],
                variant: UuidVariant::Any,
                spelling: UuidSpelling::ParserAliases,
            },
            map_id_error,
        ),
        ..MapIds::PROFILE
    };
}

#[veoveo_types::id(prefixed(MapIds, "dataset-"), fresh, stable)]
pub struct MapDatasetId(String);

#[veoveo_types::id(prefixed(MapIds, "release-"), fresh, stable)]
pub struct DatasetReleaseId(String);

#[veoveo_types::id(prefixed(MapIds, "source-"), fresh, stable)]
pub struct MapSourceId(String);

#[veoveo_types::id(prefixed(MapIds, "source-feature-"), fresh, stable)]
pub struct SourceFeatureId(String);

#[veoveo_types::id(prefixed(MapIds, "raster-"), fresh, stable)]
pub struct RasterProductId(String);

#[veoveo_types::id(prefixed(MapIds, "raster-derivation-"), fresh, stable)]
pub struct RasterDerivationId(String);

#[veoveo_types::id(prefixed(MapIds, "spatial-derivation-"), fresh, stable)]
pub struct SpatialDerivationId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "source-policy-"), fresh, stable)]
pub struct SourcePolicyId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "acquisition-"), fresh, stable)]
pub struct AcquisitionId(String);

#[veoveo_types::id(prefixed(MapIds, "snapshot-"), fresh, stable)]
pub struct OperationalSnapshotId(String);

#[veoveo_types::id(prefixed(MapIds, "location-"), fresh, stable)]
pub struct LocationId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "boundary-"), fresh, stable)]
pub struct MapBoundaryId(String);

#[veoveo_types::id(prefixed(MapIds, "facility-"), fresh, stable)]
pub struct FacilityId(String);

#[veoveo_types::id(prefixed(MapIds, "mobility-"), fresh, stable)]
pub struct MobilityProfileId(String);

#[veoveo_types::id(prefixed(MapIds, "restriction-"), fresh, stable)]
pub struct RestrictionId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "geofence-"), fresh, stable)]
pub struct MapGeofenceId(String);

#[veoveo_types::id(prefixed(MapIds, "route-"), fresh, stable)]
pub struct RouteId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "matrix-"), fresh, stable)]
pub struct RouteMatrixId(String);

#[veoveo_types::id(prefixed(MapIds, "travel-model-"), fresh, stable)]
pub struct TravelModelId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "reachable-area-"), fresh, stable)]
pub struct ReachableAreaId(String);

#[veoveo_types::id(prefixed(MapIds, "validation-"), fresh, stable)]
pub struct ValidationId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "map-operation-"), fresh, stable)]
pub struct MapOperationId(String);

#[veoveo_types::id(prefixed(MapIds, "feature-layer-"), fresh, stable)]
pub struct FeatureLayerId(String);

#[veoveo_types::id(prefixed(MapIds, "feature-"), fresh, stable)]
pub struct MapFeatureId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "changeset-"), fresh, stable)]
pub struct FeatureChangeSetId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "feature-schema-"), fresh, stable)]
pub struct FeatureSchemaRevisionId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "style-"), fresh, stable)]
pub struct StyleRevisionId(String);

#[veoveo_types::id(prefixed(MapIds, "publication-"), fresh, stable)]
pub struct LayerPublicationId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "layer-product-"), fresh, stable)]
pub struct LayerProductId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "composition-"), fresh, stable)]
pub struct MapCompositionId(String);

#[veoveo_types::id(prefixed(MapAliasIds, "composition-revision-"), fresh, stable)]
pub struct MapCompositionRevisionId(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_prefixed_uuid_v7_values() {
        let id = RouteId::new();
        assert!(id.as_str().starts_with("route-"));
        assert_eq!(id.as_uuid().get_version_num(), 7);
        assert_eq!(RouteId::parse(&id).unwrap(), id);
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
        assert_eq!(first.as_uuid().get_version_num(), 5);
        assert_eq!(LocationId::parse(&first).unwrap(), first);
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
        assert_eq!(schema.as_value()["type"], "string");
        let naming =
            veoveo_types::naming_profile(&schema, veoveo_types::NamingSchemaContext::new(&schema))
                .unwrap()
                .unwrap();
        assert_eq!(naming.revision(), 1);
        assert_eq!(
            naming.role(),
            &veoveo_types::NamingRole::Scalar {
                profile: veoveo_types::ScalarNaming::owner(
                    module_path!().strip_suffix("::identity_profiles").unwrap(),
                    &I::schema_name(),
                )
                .unwrap(),
            }
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
