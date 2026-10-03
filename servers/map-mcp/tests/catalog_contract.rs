use veoveo_map_mcp::contract::*;
use veoveo_types::{ResourceAddress, ResourceUriBuilder};

#[test]
fn host_admits_operational_pages_and_retains_specific_cursor_ids() {
    let dataset = MapDatasetId::new();
    let positions = [
        MapCatalogPage::Routes {
            after: Some(RouteId::new()),
        },
        MapCatalogPage::Matrices {
            after: Some(RouteMatrixId::new()),
        },
        MapCatalogPage::Acquisitions {
            after: Some(AcquisitionId::new()),
        },
        MapCatalogPage::Releases {
            dataset: None,
            after: Some(DatasetReleaseId::new()),
        },
        MapCatalogPage::Releases {
            dataset: Some(dataset),
            after: Some(DatasetReleaseId::new()),
        },
        MapCatalogPage::RasterDerivations {
            after: Some(RasterDerivationId::new()),
        },
        MapCatalogPage::SpatialDerivations {
            after: Some(SpatialDerivationId::new()),
        },
    ];
    for position in &positions {
        let uri = position.to_uri().unwrap();
        let admitted = MapAddress::parse(uri.as_str()).unwrap();
        let MapTarget::Catalog(selected) = admitted.target() else {
            panic!("wrong hosted target: {admitted:?}")
        };
        assert_eq!(selected, position);
        assert_eq!(admitted.to_uri(), uri);
        for other in &positions {
            if other != position {
                assert!(other.clone().resume(position.cursor().as_deref()).is_err());
            }
        }
    }
}

#[test]
fn catalog_cursor_admission_rejects_wrong_parent_version_type_and_query() {
    let routes = MapCatalogPage::Routes { after: None };
    let route = RouteId::new();
    let encode = |version, collection, after| {
        hex::encode(
            serde_json::to_vec(
                &serde_json::json!({"version":version,"collection":collection,"after":after}),
            )
            .unwrap(),
        )
    };
    for cursor in [
        String::new(),
        "gg".into(),
        "a".repeat(2049),
        encode(2, "routes", route.to_string()),
        encode(1, "routes", RouteMatrixId::new().to_string()),
    ] {
        assert!(routes.clone().resume(Some(&cursor)).is_err());
    }
    let cursor = MapCatalogPage::Releases {
        dataset: Some(MapDatasetId::new()),
        after: Some(DatasetReleaseId::new()),
    }
    .cursor()
    .unwrap();
    assert!(
        MapCatalogPage::Releases {
            dataset: None,
            after: None
        }
        .resume(Some(&cursor))
        .is_err()
    );
    let release_page = MapCatalogPage::Releases {
        dataset: None,
        after: None,
    };
    for body in [
        serde_json::json!({"version":2,"collection":"releases","dataset":null,"after":DatasetReleaseId::new()}),
        serde_json::json!({"version":1,"collection":"releases","dataset":null,"after":RouteId::new()}),
        serde_json::json!({"version":1,"collection":"releases","dataset":null,"after":DatasetReleaseId::new(),"extra":true}),
    ] {
        let cursor = hex::encode(serde_json::to_vec(&body).unwrap());
        assert!(release_page.clone().resume(Some(&cursor)).is_err());
    }
    for uri in [
        "map://routes?",
        "map://routes?cursor=gg",
        "map://routes?cursor=00&cursor=11",
        "map://routes?extra=value",
        "map://routes/extra",
        "map://dataset/dataset-invalid",
    ] {
        assert!(MapAddress::parse(uri).is_err(), "{uri}");
    }
}

#[test]
fn host_keeps_knowledge_addresses_separate_from_full_resource_reads() {
    let member = MapKnowledgeMember::Release {
        dataset: MapDatasetId::new(),
        release: DatasetReleaseId::new(),
    };
    let page = MapKnowledgePageUri::new(member.collection())
        .with_cursor(MapKnowledgeCursor::after(member.clone()))
        .unwrap();
    for uri in [page.to_uri(), member.to_uri()] {
        let address = MapAddress::parse(uri.as_str()).unwrap();
        assert!(matches!(
            address.target(),
            MapTarget::KnowledgePage(_) | MapTarget::KnowledgeMember(_)
        ));
        assert_eq!(address.to_uri(), uri);
    }
    assert!(matches!(
        MapAddress::parse(member.source_uri().as_str())
            .unwrap()
            .target(),
        MapTarget::Dataset(MapDatasetAddress::Release(_))
    ));
    for uri in ["map://knowledge/unknown", "other://datasets"] {
        assert!(MapAddress::parse(uri).is_err());
    }
}

#[test]
fn host_admits_owner_pages_artifacts_and_filtered_features() {
    let mobility = MobilityProfileId::new();
    let version = MobilityProfileVersion::try_from(1_u64).unwrap();
    let addresses = [
        MapSourcesUri::new(Some(MapSourceCursor::new(MapSourceId::new())))
            .to_uri()
            .unwrap(),
        MapSourceUri::new(MapSourceId::new()).to_uri().unwrap(),
        MapMobilityProfilesUri::new(Some(MapMobilityProfileCursor::new(
            mobility.clone(),
            version,
        )))
        .to_uri()
        .unwrap(),
        MapMobilityProfileUri::new(mobility, version)
            .to_uri()
            .unwrap(),
        MapRestrictionsUri::new(Some(MapRestrictionCursor::new(RestrictionId::new())))
            .to_uri()
            .unwrap(),
        MapRestrictionUri::new(RestrictionId::new())
            .to_uri()
            .unwrap(),
        MapTravelModelsUri::new(Some(
            MapTravelModelCursor::new(veoveo_types::TaskId::new()).unwrap(),
        ))
        .to_uri()
        .unwrap(),
        MapTravelModelUri::new(TravelModelId::new())
            .to_uri()
            .unwrap(),
        veoveo_artifact_contract::ArtifactUri::presented(
            &veoveo_map_mcp::uris::SCHEME,
            veoveo_artifact_contract::ArtifactId::new(),
        )
        .to_uri()
        .unwrap(),
    ];
    for uri in addresses {
        assert_eq!(MapAddress::parse(uri.as_str()).unwrap().to_uri(), uri);
    }
    let layer = FeatureLayerId::new();
    let uri = ResourceUriBuilder::new(&veoveo_map_mcp::uris::features_uri(&layer))
        .unwrap()
        .query_pair("limit", "5")
        .unwrap()
        .query_pair("bbox", "-80,30,-79,31")
        .unwrap()
        .build()
        .unwrap();
    let admitted = MapAddress::parse(uri.as_str()).unwrap();
    let MapTarget::Features(request) = admitted.target() else {
        panic!("expected a feature query")
    };
    assert_eq!(request.layer_id, layer);
    assert_eq!(request.limit, 5);
    assert_eq!(request.bbox.as_ref().unwrap().west, -80.0);
}
