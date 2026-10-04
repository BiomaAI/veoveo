use super::*;

fn policy() -> PreviewScenePolicy {
    PreviewScenePolicy {
        width_px: 1280,
        height_px: 720,
        max_screen_error_px: 16.0,
    }
}

#[test]
fn every_resource_family_round_trips_through_the_shared_components() {
    let composition = SceneCompositionId::from_stable_key(b"typed-view");
    let tile = TileKey::from_bytes(b"tile-content");
    let resources = [
        ViewResource::Docs,
        ViewResource::Document(ViewDocument::Agents),
        ViewResource::Document(ViewDocument::Design),
        ViewResource::Contract,
        ViewResource::PreviewApp,
        ViewResource::Layers,
        ViewResource::Compositions,
        ViewResource::Views,
        ViewResource::Frames,
        ViewResource::Layer(LayerUri::new(LayerId::parse("layer-1").unwrap())),
        ViewResource::View(ViewUri::new(ViewId::parse("view-1").unwrap())),
        ViewResource::Composition(CompositionUri::new(composition)),
        ViewResource::Frame(FrameUri::new(FrameId::parse("frame-1").unwrap())),
        ViewResource::Tile(TileUri::new(tile)),
        ViewResource::Scene(ViewSceneUri::new(ViewId::parse("view-1").unwrap(), policy()).unwrap()),
    ];
    for resource in resources {
        let uri = resource.to_uri().unwrap();
        assert_eq!(ViewResource::parse(&uri).unwrap(), resource);
        assert_eq!(serde_json::to_value(&resource).unwrap(), uri.as_str());
        assert_eq!(
            serde_json::from_value::<ViewResource>(serde_json::json!(uri.as_str())).unwrap(),
            resource
        );
    }
}

#[test]
fn scene_construction_keeps_the_wire_shape_and_checks_numeric_admission() {
    let id = ViewId::parse("view-1").unwrap();
    let address = ViewSceneUri::new(id.clone(), policy()).unwrap();
    assert_eq!(
        address.to_string(),
        "view://view/view-1/scene?width_px=1280&height_px=720&max_screen_error_px=16"
    );
    assert_eq!(address.view_id(), &id);
    assert_eq!(address.policy(), policy());
    assert!(ViewUri::parse(address.to_string()).is_err());
    for error in [
        0.0,
        0.249,
        256.1,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        assert!(
            ViewSceneUri::new(
                id.clone(),
                PreviewScenePolicy {
                    max_screen_error_px: error,
                    ..policy()
                }
            )
            .is_err()
        );
    }
    for (width_px, height_px) in [(0, 720), (1280, 0)] {
        assert!(
            ViewSceneUri::new(
                id.clone(),
                PreviewScenePolicy {
                    width_px,
                    height_px,
                    ..policy()
                }
            )
            .is_err()
        );
    }
    // The address profile does not substitute for the installation's smaller GPU budget.
    let address = ViewSceneUri::new(
        id,
        PreviewScenePolicy {
            width_px: 4096,
            ..policy()
        },
    )
    .unwrap();
    assert!(
        address
            .policy()
            .validate(&CaptureLimits {
                max_width_px: 1920,
                max_height_px: 1080,
                max_pixels: 1920 * 1080,
                max_deadline_ms: 1000
            })
            .is_err()
    );
}

#[test]
fn route_admission_rejects_credentials_aliases_and_wrong_parents() {
    for uri in [
        "view://user:secret@view/view-1",
        "view://view:80/view-1",
        "view://view/view-1#secret",
        "view://view/view-1?secret=value",
        "view://view/view-1?",
        "view://views/",
        "view://view/view-1/extra",
        "view://view/.",
        "view://view/..",
        "view://view/%2E%2E",
        "view://view/a%2Fb",
        "view://view/a%3Fb",
        "view://view/a%23b",
        "view://view/a%25b",
        "view://view/a%20b",
        "view://view/%00",
        "view://view/%FF",
        "view://view/%zz",
        "view://view/v%69ew-1",
        "VIEW://view/view-1",
        "other://view/view-1",
        "ui://other/preview.html",
        "view://docs/unknown",
        "view://docs/agents/extra",
        "view://tile/abc",
        "view://composition/view-1",
        "view://frame/view-1/scene",
    ] {
        let error = ViewResource::parse(uri).unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid View resource address or parameters"
        );
    }
    assert!(FrameUri::parse("view://view/view-1").is_err());
    assert!(CompositionUri::parse("view://frame/frame-1").is_err());
    for id in [".", "..", "a/b", "a?b", "a#b", "a%b", "a b"] {
        assert!(ViewId::parse(id).is_err());
        assert!(FrameId::parse(id).is_err());
        assert!(LayerId::parse(id).is_err());
    }
}

#[test]
fn scene_queries_require_exact_names_unique_values_and_canonical_numbers() {
    let base = "view://view/view-1/scene?";
    for query in [
        "width_px=1280",
        "width_px=1280&height_px=720&max_screen_error_px=16&other=secret",
        "width_px=1280&width_px=1280&height_px=720&max_screen_error_px=16",
        "width_px=1280&%77idth_px=1280&height_px=720&max_screen_error_px=16",
        "width_px=0&height_px=720&max_screen_error_px=16",
        "width_px=4294967296&height_px=720&max_screen_error_px=16",
        "width_px=-1&height_px=720&max_screen_error_px=16",
        "width_px=1280&height_px=720&max_screen_error_px=NaN",
        "width_px=1280&height_px=720&max_screen_error_px=inf",
        "width_px=1280&height_px=720&max_screen_error_px=0.2",
        "width_px=1280&height_px=720&max_screen_error_px=257",
        "width_px=01280&height_px=720&max_screen_error_px=16",
        "width_px=1280&height_px=720&max_screen_error_px=16.0",
        "height_px=720&width_px=1280&max_screen_error_px=16",
        "width_px=1280&height_px=720&max_screen_error_px=16#fragment",
    ] {
        assert!(
            ViewSceneUri::parse(format!("{base}{query}")).is_err(),
            "accepted {query}"
        );
    }
}

#[test]
fn tile_keys_keep_the_lowercase_sha256_identity() {
    let key = TileKey::from_bytes(b"layer\nhttps://tiles.test/tile.glb");
    assert_eq!(
        key.as_str(),
        Sha256Digest::from_bytes(b"layer\nhttps://tiles.test/tile.glb").as_str()
    );
    assert_eq!(TileKey::parse(key.as_str()).unwrap(), key);
    assert!(TileKey::parse(key.as_str().to_uppercase()).is_err());
    assert!(TileKey::parse("a".repeat(63)).is_err());
    assert!(TileKey::parse("g".repeat(64)).is_err());
}

#[test]
fn typed_record_addresses_keep_the_public_string_schema() {
    for (schema, fields) in [
        (
            serde_json::to_value(schemars::schema_for!(super::super::ViewRecord)).unwrap(),
            vec!["view_uri", "composition_uri"],
        ),
        (
            serde_json::to_value(schemars::schema_for!(super::super::FrameRecord)).unwrap(),
            vec!["frame_uri", "composition_uri"],
        ),
        (
            serde_json::to_value(schemars::schema_for!(super::super::SceneComposition)).unwrap(),
            vec!["composition_uri"],
        ),
        (
            serde_json::to_value(schemars::schema_for!(super::super::SceneTileRecord)).unwrap(),
            vec!["tile_uri"],
        ),
    ] {
        for field in fields {
            assert_eq!(schema["properties"][field]["type"], "string", "{field}");
        }
    }
}
