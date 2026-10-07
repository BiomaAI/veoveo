//! Browser bundle roots select existing owner DTOs; these fields are schema tooling,
//! not an additional transport envelope.
use super::*;
use schemars::{JsonSchema, Schema};

#[derive(JsonSchema)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct AppContracts {
    pub workspace: MapWorkspaceAccess,
    pub layers: MapMetadataPage<FeatureLayer>,
    pub publications: MapMetadataPage<LayerPublication>,
    pub compositions: MapMetadataPage<MapComposition>,
    pub sources: MapSourcePage,
    pub datasets: ReleasePage,
    pub profiles: MapMobilityProfilePage,
    pub acquisitions: OwnedPage<AcquisitionJob>,
    pub active_releases: Vec<ActiveDatasetRelease>,
    pub style: MapStyleRevision,
    pub features: QueryFeaturesOutput,
    pub source_features: QuerySourceFeaturesOutput,
    pub layer: FeatureLayer,
    pub validated: ValidateFeatureChangesOutput,
    pub committed: CommitFeatureChangesOutput,
    pub acquisition: AcquisitionJob,
    pub composition: MapComposition,
    pub source: RegisteredSource,
    pub profile: MobilityProfile,
    pub publication: LayerPublication,
    pub inspected: InspectGeoPackageOutput,
    pub imported: MapTaskProduct<ImportFeatureLayerOutput>,
    pub release: ReleaseMutationResponse,
}

pub fn schema_bundle() -> Schema {
    schemars::schema_for!(AppContracts)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_page_roots_require_and_admit_nullable_cursor_bytes() {
        let schema = serde_json::to_value(schema_bundle()).unwrap();
        for (root, bytes) in [
            (
                "layers",
                serde_json::to_value(MapMetadataPage::<FeatureLayer> {
                    items: vec![],
                    limit: 100,
                    next_cursor: None,
                })
                .unwrap(),
            ),
            (
                "publications",
                serde_json::to_value(MapMetadataPage::<LayerPublication> {
                    items: vec![],
                    limit: 100,
                    next_cursor: None,
                })
                .unwrap(),
            ),
            (
                "compositions",
                serde_json::to_value(MapMetadataPage::<MapComposition> {
                    items: vec![],
                    limit: 100,
                    next_cursor: None,
                })
                .unwrap(),
            ),
            (
                "sources",
                serde_json::to_value(MapSourcePage::from_lookahead(vec![]).unwrap()).unwrap(),
            ),
            (
                "datasets",
                serde_json::to_value(ReleasePage {
                    items: vec![],
                    limit: 100,
                    next_cursor: None,
                })
                .unwrap(),
            ),
            (
                "profiles",
                serde_json::to_value(MapMobilityProfilePage::from_lookahead(vec![]).unwrap())
                    .unwrap(),
            ),
            (
                "acquisitions",
                serde_json::to_value(OwnedPage::<AcquisitionJob> {
                    items: vec![],
                    limit: 100,
                    next_cursor: None,
                })
                .unwrap(),
            ),
        ] {
            let selected = serde_json::json!({
                "$ref": schema["properties"][root]["$ref"],
                "$defs": schema["$defs"],
            });
            let validator = jsonschema::validator_for(&selected).unwrap();
            assert!(validator.is_valid(&bytes));
            let mut value = bytes.clone();
            value["nextCursor"] = serde_json::json!("owner-cursor");
            assert!(validator.is_valid(&value));
            value["nextCursor"] = serde_json::json!(42);
            assert!(!validator.is_valid(&value));
            value.as_object_mut().unwrap().remove("nextCursor");
            assert!(!validator.is_valid(&value));
        }
    }
}
