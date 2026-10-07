//! Checked Map fixtures and admission for the existing browser App bridge.
use anyhow::{Result, ensure};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_map_mcp::contract::*;
use veoveo_types::{AccessSubject, InvocationMode, PolicyVersion, PrincipalId, WorkContextId};

pub(super) struct Fixture {
    pub layer: FeatureLayer,
    pub publication: LayerPublication,
    composition: MapComposition,
    source: RegisteredSource,
    release: DatasetRelease,
    features: Vec<MapFeature>,
    source_features: Vec<SourceFeatureMatch>,
}

impl Fixture {
    pub fn new() -> Result<Self> {
        let now = chrono::Utc::now();
        let author = PrincipalId::parse("map-acceptance")?;
        let context = WorkContextId::parse("map-acceptance")?;
        let layer_id = FeatureLayerId::from_stable_key(b"map-acceptance/layer");
        let publication_id = LayerPublicationId::from_stable_key(b"map-acceptance/publication");
        let source_id = MapSourceId::from_stable_key(b"map-acceptance/source");
        let dataset_id = MapDatasetId::from_stable_key(b"map-acceptance/dataset");
        let release_id = DatasetReleaseId::from_stable_key(b"map-acceptance/release");
        let style = MapStyleRevision {
            style_revision_id: StyleRevisionId::from_stable_key(b"map-acceptance/style"),
            layer_id: layer_id.clone(),
            version: 1,
            created_at: now,
            style: LayerStyle {
                rules: vec![StyleRule {
                    geometry_type: None,
                    minimum_zoom: None,
                    maximum_zoom: None,
                    fill_color: Some("#287e8e".into()),
                    fill_opacity: Some(0.32),
                    line_color: Some("#164d59".into()),
                    line_width_px: Some(3.),
                    circle_color: Some("#b34f68".into()),
                    circle_radius_px: Some(7.),
                    label_property: None,
                }],
            },
        };
        let schema = serde_json::json!({"type":"object","additionalProperties":true});
        let layer = FeatureLayer::new(FeatureLayerValue {
            layer_id: layer_id.clone(),
            title: "San Salvador operations".into(),
            description: None,
            content_class: FeatureContentClass::Reference,
            schema: FeatureSchemaRevision {
                schema_revision_id: FeatureSchemaRevisionId::from_stable_key(
                    b"map-acceptance/schema",
                ),
                layer_id: layer_id.clone(),
                version: 1,
                digest_sha256: hex::encode(Sha256::digest(serde_json::to_vec(&schema)?)),
                schema,
                created_at: now,
            },
            style: Some(style.clone()),
            revision: 3,
            owner: AccessSubject::Principal(author.clone()),
            created_by: author.clone(),
            work_context: context.clone(),
            classification: None,
            data_labels: BTreeSet::new(),
            archived_at: None,
            created_at: now,
            updated_at: now,
        })?;
        let publication = LayerPublication::new(LayerPublicationValue {
            publication_id: publication_id.clone(),
            layer_id: layer_id.clone(),
            layer_revision: 3,
            schema_version: 1,
            style_revision_id: Some(style.style_revision_id.clone()),
            title: Some("Operations publication".into()),
            artifact_uris: vec![],
            published_by: author.clone(),
            work_context: context.clone(),
            published_at: now,
        })?;
        let composition_id = MapCompositionId::from_stable_key(b"map-acceptance/composition");
        let composition = MapComposition::new(MapCompositionValue {
            composition_id: composition_id.clone(),
            title: "San Salvador operational picture".into(),
            current: MapCompositionRevision::new(MapCompositionRevisionValue {
                composition_revision_id: MapCompositionRevisionId::from_stable_key(
                    b"map-acceptance/composition-revision",
                ),
                composition_id,
                revision: 2,
                layers: vec![CompositionLayer {
                    layer_id: layer_id.clone(),
                    publication_id,
                    style_revision_id: Some(style.style_revision_id.clone()),
                    visible: true,
                    opacity: 1.,
                }],
                view: CompositionView {
                    center: Wgs84Position::new(-89.2182, 13.6929, None)?,
                    zoom: 12.5,
                    bearing_deg: 0.,
                    pitch_deg: 0.,
                },
                created_by: author.clone(),
                created_at: now,
            })?,
            owner: AccessSubject::Principal(author.clone()),
            created_by: author.clone(),
            work_context: context.clone(),
            classification: None,
            data_labels: BTreeSet::new(),
            archived_at: None,
            created_at: now,
            updated_at: now,
        })?;
        let license = DatasetLicense {
            license_id: "acceptance".into(),
            source_terms_uri: HttpsEndpoint::parse("https://example.invalid/terms")?,
            attribution: "Veoveo acceptance source".into(),
            redistribution_allowed: true,
            derivatives_allowed: true,
            offline_bundle_allowed: false,
            expires_at: None,
        };
        let source = RegisteredSource::new(RegisteredSourceValue {
            source_id: source_id.clone(),
            dataset_id: dataset_id.clone(),
            name: "San Salvador reference source".into(),
            adapter_kind: SourceAdapterKind::AuthorityVector,
            authority: AuthorityClass::SyntheticTest,
            acquisition_model: AcquisitionModel::Snapshot,
            map_families: BTreeSet::from([MapFamily::RoadStreet]),
            location: SourceLocation::Https {
                endpoint: HttpsEndpoint::parse("https://example.invalid/acceptance.geojson")?,
                allowed_redirect_hosts: BTreeSet::new(),
            },
            credential: None,
            publisher_key_refs: BTreeSet::new(),
            expected_media_types: BTreeSet::from(["application/geo+json".into()]),
            maximum_download_bytes: 1024 * 1024,
            maximum_elapsed_seconds: 30,
            license: license.clone(),
            enabled: true,
            record_version: 1,
            created_at: now,
            updated_at: now,
        })?;
        let artifact = veoveo_artifact_contract::ArtifactUri::plane(
            veoveo_artifact_contract::ArtifactId::new(),
        );
        let release = DatasetRelease::new(DatasetReleaseValue {
            release_id: release_id.clone(),
            dataset_id,
            source_id: source_id.clone(),
            version_label: "acceptance".into(),
            source_digest_sha256: hex::encode(Sha256::digest(b"acceptance-source")),
            coverage: Wgs84BoundingBox {
                west: -89.24,
                south: 13.67,
                east: -89.19,
                north: 13.72,
            },
            acquired_at: now,
            valid_from: now,
            valid_until: None,
            schema_version: 2,
            normalization_pipeline_version: "acceptance".into(),
            routing_build_version: None,
            license: license.clone(),
            raw_artifact_uri: artifact.clone(),
            normalized_artifact_uris: vec![artifact.clone()],
            quality_report_uri: artifact,
            supersedes_release_id: None,
            state: DatasetReleaseState::Active,
            record_version: 1,
            updated_at: now,
        })?;
        let point = |x, y| GeoJsonPosition::new(x, y, None);
        let geometries = [
            (
                "command",
                "Command post",
                FeatureGeometry::Point(point(-89.2182, 13.6929)),
            ),
            (
                "route",
                "Supply route",
                FeatureGeometry::LineString(vec![
                    point(-89.229, 13.688),
                    point(-89.2182, 13.6929),
                    point(-89.207, 13.699),
                ]),
            ),
            (
                "sector",
                "Operating sector",
                FeatureGeometry::Polygon(vec![vec![
                    point(-89.225, 13.688),
                    point(-89.211, 13.688),
                    point(-89.211, 13.699),
                    point(-89.225, 13.699),
                    point(-89.225, 13.688),
                ]]),
            ),
        ];
        let mut features = vec![];
        for (role, title, geometry) in geometries {
            features.push(MapFeature::new(MapFeatureValue {
                feature_type: GeoJsonFeatureType::Feature,
                conforms_to: vec![
                    JSON_FG_CORE_CONFORMANCE.into(),
                    JSON_FG_TYPES_SCHEMAS_CONFORMANCE.into(),
                ],
                id: MapFeatureId::from_stable_key(role.as_bytes()),
                geometry,
                properties: BTreeMap::from([("role".into(), serde_json::json!(role))]),
                semantic_type: role.into(),
                time: None,
                layer_id: layer_id.clone(),
                feature_revision: 1,
                layer_revision: 3,
                schema_version: 1,
                deleted: false,
                title: Some(title.into()),
                related_resources: vec![],
                evidence_resources: vec![],
                provenance: FeatureProvenance {
                    actor_id: author.clone(),
                    work_context: context.clone(),
                    policy_revision: PolicyVersion::parse("acceptance")?,
                    invocation_mode: InvocationMode::Automated,
                    initiator_id: None,
                    delegation_id: None,
                },
                created_at: now,
            })?);
        }
        let geometry = features[2].geometry.clone();
        let source_feature = SourceFeature::new(SourceFeatureValue {
            schema_version: SOURCE_FEATURE_SCHEMA_VERSION,
            feature_id: SourceFeatureId::from_stable_key(b"map-acceptance/source-feature"),
            source_id,
            release_id,
            source_element_type: SourceElementType::Feature,
            source_element_id: "reference-zone".into(),
            source_element_version: "1".into(),
            representation: SourceFeatureRepresentation::Polygon,
            source_geometry_path: vec![],
            geometry_digest_sha256: hex::encode(Sha256::digest(serde_json::to_vec(&geometry)?)),
            geometry,
            normalized_tags: BTreeMap::from([(
                "name".into(),
                serde_json::json!("Reference coverage"),
            )]),
            original_names: BTreeMap::new(),
            original_references: BTreeSet::new(),
            operating_area_ids: BTreeSet::new(),
            source_digest_sha256: release.source_digest_sha256.clone(),
            license,
            acquired_at: now,
        })?;
        Ok(Self {
            layer,
            publication,
            composition,
            source,
            release,
            features,
            source_features: vec![SourceFeatureMatch {
                feature: source_feature,
                distance: None,
            }],
        })
    }

    pub fn resources(&self, basemap: MapWorkspaceBasemap) -> Result<BTreeMap<String, Value>> {
        basemap.validate().map_err(anyhow::Error::msg)?;
        let page = |items| OwnedPage {
            items,
            limit: 100,
            next_cursor: None,
        };
        let mut resources = BTreeMap::new();
        resources.insert(
            "map://workspace".into(),
            serde_json::to_value(MapWorkspaceAccess {
                administration: true,
                dataset_read: true,
                feature_read: true,
                feature_write: true,
                feature_publish: true,
                basemap,
            })?,
        );
        resources.insert(
            "map://feature-layers".into(),
            serde_json::to_value(page(vec![self.layer.clone()]))?,
        );
        resources.insert(
            "map://publications".into(),
            serde_json::to_value(OwnedPage {
                items: vec![self.publication.clone()],
                limit: 100,
                next_cursor: None,
            })?,
        );
        resources.insert(
            "map://compositions".into(),
            serde_json::to_value(OwnedPage {
                items: vec![self.composition.clone()],
                limit: 100,
                next_cursor: None,
            })?,
        );
        let style = self.layer.style.clone().expect("fixture style");
        resources.insert(
            MapResource::StyleRevision {
                id: style.style_revision_id.clone(),
            }
            .to_uri()
            .to_string(),
            serde_json::to_value(style)?,
        );
        resources.insert(
            "map://sources".into(),
            serde_json::to_value(MapSourcePage::from_lookahead(vec![SourceSummary::new(
                &self.source,
            )?])?)?,
        );
        resources.insert(
            "map://datasets".into(),
            serde_json::to_value(ReleasePage {
                items: vec![self.release.clone()],
                limit: 100,
                next_cursor: None,
            })?,
        );
        resources.insert(
            "map://active-releases".into(),
            serde_json::to_value(vec![ActiveReleasePointer {
                dataset_id: self.release.dataset_id.clone(),
                release_id: self.release.release_id.clone(),
                previous_release_id: None,
                record_version: 1,
                activated_at: self.release.valid_from,
            }])?,
        );
        resources.insert(
            "map://acquisitions".into(),
            serde_json::to_value(OwnedPage::<AcquisitionJob> {
                items: vec![],
                limit: 100,
                next_cursor: None,
            })?,
        );
        resources.insert(
            "map://mobility-profiles".into(),
            serde_json::to_value(MapMobilityProfilePage::from_lookahead(vec![])?)?,
        );
        Ok(resources)
    }

    pub fn query(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            "query_features" => Ok(serde_json::to_value(
                self.authored(&serde_json::from_value(arguments)?)?,
            )?),
            "query_source_features" => Ok(serde_json::to_value(
                self.source(&serde_json::from_value(arguments)?)?,
            )?),
            _ => anyhow::bail!("unsupported acceptance query"),
        }
    }

    pub fn authored(&self, request: &QueryFeaturesRequest) -> Result<QueryFeaturesOutput> {
        ensure!(
            request.layer_id == self.layer.layer_id
                && request.publication_id.as_ref() == Some(&self.publication.publication_id),
            "Map query changed selected layer/publication"
        );
        let bounds = request
            .bbox
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Map viewport query omitted bounds"))?;
        bounds.validate()?;
        ensure!(
            request.limit == 1000
                && request.cursor.is_none()
                && request.datetime.is_none()
                && request.geometry_type.is_none()
                && request.filter.is_none()
                && request.minimum_commit_sequence.is_none(),
            "unsupported acceptance query selection"
        );
        Ok(QueryFeaturesOutput {
            layer_id: self.layer.layer_id.clone(),
            features: self.features.clone(),
            next_cursor: None,
            projection_sequence: 3,
        })
    }
    pub fn source(
        &self,
        request: &QuerySourceFeaturesRequest,
    ) -> Result<QuerySourceFeaturesOutput> {
        request.validate()?;
        ensure!(
            request.release_id == self.release.release_id
                && request
                    .source_id
                    .as_ref()
                    .is_none_or(|id| id == &self.source.source_id),
            "Map query changed selected release/source"
        );
        ensure!(
            request.limit == 500
                && request.cursor.is_none()
                && request.source_element_id.is_none()
                && request.representation.is_none()
                && request.tags_equal.is_empty()
                && request.tags_exist.is_empty()
                && request.normalized_text.is_none()
                && matches!(
                    request.spatial,
                    Some(SourceSpatialQuery::BoundingBox { .. })
                ),
            "unsupported acceptance source query selection"
        );
        Ok(QuerySourceFeaturesOutput {
            release_id: self.release.release_id.clone(),
            query_digest_sha256: request.query_digest_sha256()?,
            features: self.source_features.clone(),
            next_cursor: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_map_fixture_and_copied_queries_use_checked_owner_shapes() {
        let fixture = Fixture::new().unwrap();
        let resources = fixture
            .resources(
                MapWorkspaceBasemap::open_free_map(
                    "https://tiles.openfreemap.org/styles/positron",
                    "https://tiles.openfreemap.org/styles/dark",
                )
                .unwrap(),
            )
            .unwrap();
        let layers: OwnedPage<FeatureLayer> =
            serde_json::from_value(resources["map://feature-layers"].clone()).unwrap();
        assert_eq!(layers.items[0].layer_id, fixture.layer.layer_id);
        let bounds = Wgs84BoundingBox {
            west: -89.24,
            south: 13.67,
            east: -89.19,
            north: 13.72,
        };
        let authored = QueryFeaturesRequest {
            layer_id: fixture.layer.layer_id.clone(),
            publication_id: Some(fixture.publication.publication_id.clone()),
            bbox: Some(bounds.clone()),
            datetime: None,
            geometry_type: None,
            filter: None,
            limit: 1000,
            cursor: None,
            minimum_commit_sequence: None,
        };
        let source = QuerySourceFeaturesRequest {
            release_id: fixture.release.release_id.clone(),
            source_id: Some(fixture.source.source_id.clone()),
            source_element_id: None,
            representation: None,
            tags_equal: vec![],
            tags_exist: vec![],
            normalized_text: None,
            spatial: Some(SourceSpatialQuery::BoundingBox { bounds }),
            limit: 500,
            cursor: None,
        };
        for (name, value, old, current) in [
            (
                "query_features",
                serde_json::to_value(&authored).unwrap(),
                "layer_id",
                "layerId",
            ),
            (
                "query_source_features",
                serde_json::to_value(&source).unwrap(),
                "release_id",
                "releaseId",
            ),
        ] {
            fixture.query(name, value.clone()).unwrap();
            for mixed in [false, true] {
                let mut bad = value.clone();
                let object = bad.as_object_mut().unwrap();
                let selected = if mixed {
                    object[current].clone()
                } else {
                    object.remove(current).unwrap()
                };
                object.insert(old.into(), selected);
                assert!(fixture.query(name, bad).is_err());
            }
        }
        let mut wrong = authored.clone();
        wrong.publication_id = Some(LayerPublicationId::new());
        assert!(fixture.authored(&wrong).is_err());
        let mut wrong = source.clone();
        wrong.source_id = Some(MapSourceId::new());
        assert!(fixture.source(&wrong).is_err());
        let output = fixture.source(&source).unwrap();
        assert_eq!(
            output.query_digest_sha256,
            source.query_digest_sha256().unwrap()
        );
        let feature: SourceFeatureMatch =
            serde_json::from_value(serde_json::to_value(&output.features[0]).unwrap()).unwrap();
        assert_eq!(feature.feature.release_id, source.release_id);
    }
}
