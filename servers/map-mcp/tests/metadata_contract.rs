use veoveo_map_mcp::contract::{
    FeatureLayerId, LayerProductId, LayerPublicationId, MapAddress, MapCompositionId,
    MapMetadataCursor, MapMetadataRequest, MapScope,
};
use veoveo_types::{ResourceAddress, ResourceUri, ScopeDefinition};

#[test]
fn every_metadata_selection_builds_and_round_trips_with_typed_cursors() {
    let layer = FeatureLayerId::new();
    let publication = LayerPublicationId::new();
    for request in [
        MapMetadataRequest::Layers { after: None },
        MapMetadataRequest::Layers {
            after: Some(layer.clone()),
        },
        MapMetadataRequest::Publications {
            layer: None,
            after: None,
        },
        MapMetadataRequest::Publications {
            layer: Some(layer.clone()),
            after: None,
        },
        MapMetadataRequest::Publications {
            layer: Some(layer.clone()),
            after: Some(publication.clone()),
        },
        MapMetadataRequest::Products {
            publication: None,
            after: None,
        },
        MapMetadataRequest::Products {
            publication: Some(publication.clone()),
            after: None,
        },
        MapMetadataRequest::Products {
            publication: Some(publication),
            after: Some(LayerProductId::new()),
        },
        MapMetadataRequest::Compositions { after: None },
        MapMetadataRequest::Compositions {
            after: Some(MapCompositionId::new()),
        },
    ] {
        let uri = request.to_uri().unwrap();
        assert_eq!(MapAddress::parse(uri.as_str()).unwrap().to_uri(), uri);
        assert_eq!(MapMetadataRequest::parse(uri.as_str()).unwrap(), request);
        assert_eq!(
            <MapMetadataRequest as ResourceAddress>::parse(&uri).unwrap(),
            request
        );
        if let Some(cursor) = request.cursor() {
            let wire = serde_json::to_string(&cursor).unwrap();
            let decoded: MapMetadataCursor = serde_json::from_str(&wire).unwrap();
            assert_eq!(decoded, cursor);
            assert_eq!(decoded.resume(&request).unwrap(), request);
        }
    }
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(MapMetadataCursor)).unwrap()["type"],
        "string"
    );
}

#[test]
fn cursors_bind_collection_and_parent_and_reject_invalid_envelopes() {
    let layer = FeatureLayerId::new();
    let position = MapMetadataRequest::Publications {
        layer: Some(layer.clone()),
        after: Some(LayerPublicationId::new()),
    };
    let cursor = position.cursor().unwrap();
    for selection in [
        MapMetadataRequest::Layers { after: None },
        MapMetadataRequest::Publications {
            layer: None,
            after: None,
        },
        MapMetadataRequest::Publications {
            layer: Some(FeatureLayerId::new()),
            after: None,
        },
        MapMetadataRequest::Products {
            publication: None,
            after: None,
        },
    ] {
        assert!(cursor.resume(&selection).is_err());
    }
    assert!(
        MapMetadataRequest::parse(&format!("map://publications?cursor={}", cursor.as_str()))
            .is_err()
    );
    assert!(
        MapMetadataRequest::parse(&format!("map://layer-products?cursor={}", cursor.as_str()))
            .is_err()
    );
    let json: serde_json::Value =
        serde_json::from_slice(&hex::decode(cursor.as_str()).unwrap()).unwrap();
    for (field, wrong) in [("version", 2.into()), ("extra", true.into())] {
        let mut value = json.clone();
        value[field] = wrong;
        assert!(
            MapMetadataCursor::parse(hex::encode(serde_json::to_vec(&value).unwrap())).is_err()
        );
    }
    for (field, wrong) in [
        ("after", serde_json::Value::Null),
        ("after", layer.as_str().into()),
        ("extra", true.into()),
    ] {
        let mut value = json.clone();
        value["request"][field] = wrong;
        assert!(
            MapMetadataCursor::parse(hex::encode(serde_json::to_vec(&value).unwrap())).is_err()
        );
    }
    for wrong in ["".to_owned(), "0".into(), "gg".into(), "a".repeat(2049)] {
        assert!(MapMetadataCursor::parse(wrong).is_err());
    }
}

#[test]
fn resource_parser_rejects_malformed_and_unsupported_parameters() {
    for uri in [
        "map://feature-layers?",
        "map://compositions?cursor=gg",
        "map://publications?layer_id=bad",
        "map://layer-products?publication_id=bad",
        "map://publications?layer_id=a&layer_id=b",
        "map://publications?layer_id=a&%6cayer_id=b",
        "map://compositions?limit=100",
        "map://feature-layers?layer_id=bad",
        "map://feature-layers?cursor=aa&cursor=bb",
        "map://feature-layers#fragment",
        "map://feature-layers?cursor=%GG",
        "map://feature-layers?cursor=%FF",
        "map://feature-layers/",
        "map://feature-layers/..",
        "other://feature-layers",
    ] {
        assert!(MapMetadataRequest::parse(uri).is_err(), "accepted {uri}");
    }
    let uri = ResourceUri::new("map://publications?layer_id=secret-token").unwrap();
    let error = <MapMetadataRequest as ResourceAddress>::parse(&uri).unwrap_err();
    assert!(!format!("{error:?}: {error}").contains("secret-token"));
}

#[test]
fn scope_wire_values_and_schemas_match_the_domain_vocabulary() {
    let expected = [
        "map:admin",
        "map:dataset:read",
        "map:feature:read",
        "map:feature:write",
        "map:feature:publish",
        "map:feature:admin",
        "map:route",
        "map:route_matrix",
        "map:spatial:derive",
        "map:raster:derive",
        "map:restriction:publish",
        "map:restriction:withdraw",
    ];
    assert_eq!(MapScope::ALL.len(), expected.len());
    for (scope, wire) in MapScope::ALL.iter().zip(expected) {
        assert_eq!(scope.name().as_str(), wire);
        assert_eq!(serde_json::to_value(scope).unwrap(), wire);
        assert_eq!(wire.parse::<MapScope>().unwrap(), *scope);
    }
    assert!("map:feature:reed".parse::<MapScope>().is_err());
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(MapScope)).unwrap()["enum"],
        serde_json::json!(expected)
    );
}
