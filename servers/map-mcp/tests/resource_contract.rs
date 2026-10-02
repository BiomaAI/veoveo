use veoveo_map_mcp::{contract::*, uris};
use veoveo_types::ResourceTemplateUri;

fn addresses() -> Vec<(MapResource, &'static str)> {
    let layer = FeatureLayerId::new();
    let feature = MapFeatureId::new();
    let publication = LayerPublicationId::new();
    vec![
        (
            MapResource::Document(MapDocument::Agents),
            uris::DOC_TEMPLATE,
        ),
        (
            MapResource::Acquisition {
                id: AcquisitionId::new(),
            },
            uris::ACQUISITION_TEMPLATE,
        ),
        (
            MapResource::Dataset {
                id: MapDatasetId::new(),
            },
            uris::DATASET_TEMPLATE,
        ),
        (
            MapResource::Location {
                id: LocationId::new(),
            },
            uris::LOCATION_TEMPLATE,
        ),
        (
            MapResource::Facility {
                id: FacilityId::new(),
            },
            uris::FACILITY_TEMPLATE,
        ),
        (
            MapResource::Matrix {
                id: RouteMatrixId::new(),
            },
            uris::MATRIX_TEMPLATE,
        ),
        (
            MapResource::Layer {
                layer: layer.clone(),
            },
            uris::FEATURE_LAYER_TEMPLATE,
        ),
        (
            MapResource::Schema {
                layer: layer.clone(),
                version: 3,
            },
            uris::FEATURE_SCHEMA_TEMPLATE,
        ),
        (
            MapResource::Style {
                layer: layer.clone(),
                version: 4,
            },
            uris::FEATURE_STYLE_TEMPLATE,
        ),
        (
            MapResource::StyleRevision {
                id: StyleRevisionId::new(),
            },
            uris::FEATURE_STYLE_REVISION_TEMPLATE,
        ),
        (
            MapResource::Features {
                layer: layer.clone(),
            },
            uris::FEATURES_TEMPLATE,
        ),
        (
            MapResource::Feature {
                layer: layer.clone(),
                feature: feature.clone(),
            },
            uris::FEATURE_TEMPLATE,
        ),
        (
            MapResource::FeatureRevision {
                layer: layer.clone(),
                feature,
                revision: 5,
            },
            uris::FEATURE_REVISION_TEMPLATE,
        ),
        (
            MapResource::Changeset {
                layer: layer.clone(),
                changeset: FeatureChangeSetId::new(),
            },
            uris::CHANGESET_TEMPLATE,
        ),
        (
            MapResource::Publication {
                layer: layer.clone(),
                publication: publication.clone(),
            },
            uris::PUBLICATION_TEMPLATE,
        ),
        (
            MapResource::Product {
                layer,
                publication,
                product: LayerProductId::new(),
            },
            uris::LAYER_PRODUCT_TEMPLATE,
        ),
        (
            MapResource::Composition {
                id: MapCompositionId::new(),
            },
            uris::COMPOSITION_TEMPLATE,
        ),
        (
            MapResource::CompositionRevision {
                id: MapCompositionId::new(),
                revision: 6,
            },
            uris::COMPOSITION_REVISION_TEMPLATE,
        ),
    ]
}

#[test]
fn direct_addresses_preserve_their_declared_templates_and_wire_shape() {
    for (address, template) in addresses() {
        let uri = address.to_uri();
        assert_eq!(MapResource::parse(uri.as_str()).unwrap(), address);
        let wire = serde_json::to_value(&address).unwrap();
        assert_eq!(wire, uri.as_str());
        assert_eq!(
            serde_json::from_value::<MapResource>(wire).unwrap(),
            address
        );
        // Supply the variables from the independent declaration and owner path.
        let vars: Vec<_> = template
            .split('{')
            .skip(1)
            .filter_map(|s| s.split_once('}').map(|(v, _)| v))
            .filter(|v| !v.starts_with('?'))
            .collect();
        let segments: Vec<_> = uri
            .components()
            .unwrap()
            .path_segments()
            .map(|s| s.into_owned())
            .collect();
        let pattern: Vec<_> = template.split('/').skip(3).collect();
        let values: std::collections::BTreeMap<String, String> = pattern
            .iter()
            .zip(segments)
            .filter_map(|(part, value)| {
                vars.iter()
                    .find(|v| part.starts_with(&format!("{{{v}}}")))
                    .map(|v| (v.to_string(), value))
            })
            .collect();
        assert_eq!(
            ResourceTemplateUri::new(template)
                .unwrap()
                .expand_scalars(&values)
                .unwrap(),
            uri
        );
    }
}

#[test]
fn direct_addresses_reject_aliases_queries_wrong_parents_and_extra_paths() {
    for (address, _) in addresses() {
        let uri = address.to_uri().to_string();
        for invalid in [
            format!("{uri}/extra"),
            format!("{uri}?cursor=private-fixture"),
            format!("{uri}#private-fixture"),
            uri.replacen("map://", "MAP://", 1),
        ] {
            let error = MapResource::parse(&invalid).unwrap_err().to_string();
            assert!(!error.contains("private-fixture"));
        }
    }
    let layer = FeatureLayerId::new();
    let feature = MapFeatureId::new();
    for invalid in [
        format!("map://feature-layer/{feature}/feature/{layer}"),
        format!("map://feature-layer/{layer}/feature/private-fixture"),
        format!("map://feature-layer/{layer}/schema/01"),
        format!("map://feature-layer/{layer}/schema/-1"),
        format!("map://feature-layer/{layer}/schema/18446744073709551616"),
        format!(
            "map://feature-layer/{}",
            layer.to_string().replace("feature", "%66eature")
        ),
        "map://docs/private-fixture".into(),
        "map://docs/%61gents".into(),
        "map://docs/agents?".into(),
        "map://docs/agents/bad%".into(),
    ] {
        assert!(MapResource::parse(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn helpers_keep_parent_types_and_query_filters_use_shared_component_admission() {
    let layer = FeatureLayerId::new();
    let publication = LayerPublicationId::new();
    let product = LayerProductId::new();
    assert_eq!(
        uris::parse_layer_product(&uris::layer_product_uri(&layer, &publication, &product)),
        Some((layer.clone(), publication, product))
    );
    assert_eq!(
        uris::parse_doc(&uris::doc_uri(MapDocument::Agents)),
        Some(MapDocument::Agents)
    );
    let root = uris::features_uri(&layer);
    let request = uris::parse_features_request(&format!(
        "{root}?bbox=170,-10,-170,10&datetime=../2026-07-22T00%3A00%3A00Z&limit=25"
    ))
    .unwrap()
    .unwrap();
    assert_eq!(request.layer_id, layer);
    assert_eq!(request.limit, 25);
    assert_eq!(request.bbox.unwrap().west, 170.0);
    assert!(
        request.datetime.unwrap().interval[0]
            .as_timestamp()
            .is_none()
    );
    assert!(
        uris::parse_features_request(&root.replace("feature-layer-", "%66eature-layer-")).is_err()
    );
    assert!(uris::parse_features_request(&format!("{root}?")).is_err());
    for suffix in [
        "?limit=1&limit=2",
        "?limit=%",
        "#fragment",
        "?unknown=value",
        "?bbox=nan,0,1,2",
    ] {
        assert!(uris::parse_features_request(&format!("{root}{suffix}")).is_err());
    }
}
