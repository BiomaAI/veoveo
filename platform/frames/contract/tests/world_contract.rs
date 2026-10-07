use veoveo_frames_contract::{
    FrameBasis, FrameEntityPath, FrameId, FrameNode, FrameParentTransform, FrameSourceReference,
    FrameStreamUri, FrameWorldId, FrameWorldRevision, FrameWorldRevisionId, FrameWorldRevisionUri,
    FrameWorldSummary, FrameWorldTree, ValidatedWorldTree,
};

fn root() -> FrameNode {
    FrameNode {
        frame_id: FrameId::parse("earth").unwrap(),
        basis: FrameBasis::EcefWgs84,
        parent_frame_id: None,
        parent_transform: None,
        description: None,
    }
}

fn child(id: &str, parent: &str) -> FrameNode {
    FrameNode {
        frame_id: FrameId::parse(id).unwrap(),
        basis: FrameBasis::Enu,
        parent_frame_id: Some(FrameId::parse(parent).unwrap()),
        parent_transform: Some(FrameParentTransform::StaticRigid {
            translation_m: [0.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        }),
        description: None,
    }
}

fn revision() -> FrameWorldRevision {
    FrameWorldRevision::new(
        FrameWorldRevisionUri::new(
            &FrameWorldId::parse("survey").unwrap(),
            &FrameWorldRevisionId::parse("revision-1").unwrap(),
        ),
        1.try_into().unwrap(),
        ValidatedWorldTree::new(FrameWorldTree {
            frames: vec![root(), child("camera", "earth")],
        })
        .unwrap(),
        "2026-01-01T00:00:00Z".parse().unwrap(),
    )
}

#[test]
fn metadata_round_trips_and_derives_all_identities() {
    let revision = revision();
    let summary =
        FrameWorldSummary::new(revision.world_id(), "Survey".into(), revision.created_at())
            .with_description(Some("Complete tree".into()))
            .with_head(revision.revision_id(), revision.revision());
    let wire = serde_json::to_value(&summary).unwrap();
    assert_eq!(
        wire,
        serde_json::json!({
            "worldId":"survey", "worldUri":"frames://world/survey", "displayName":"Survey",
            "description":"Complete tree", "headRevisionId":"revision-1", "revision":1,
            "createdAt":"2026-01-01T00:00:00Z", "updatedAt":"2026-01-01T00:00:00Z"
        })
    );
    assert_eq!(
        serde_json::from_value::<FrameWorldSummary>(wire).unwrap(),
        summary
    );
    let wire = serde_json::to_value(&revision).unwrap();
    assert_eq!(wire["worldUri"], "frames://world/survey");
    assert_eq!(
        wire["rootFrameUri"],
        "frames://world/survey/revision/revision-1/frame/earth"
    );
    assert_eq!(
        serde_json::from_value::<FrameWorldRevision>(wire).unwrap(),
        revision
    );
    let source = FrameSourceReference::from(&revision);
    let wire = serde_json::to_value(&source).unwrap();
    assert_eq!(wire["revisionId"], "revision-1");
    assert_eq!(
        serde_json::from_value::<FrameSourceReference>(wire).unwrap(),
        source
    );
    assert_eq!(
        revision.frame(&revision.root_frame_uri()).unwrap().frame_id,
        root().frame_id
    );
    let wrong_parent = FrameWorldRevisionUri::new(
        &FrameWorldId::parse("other").unwrap(),
        &revision.revision_id(),
    );
    assert!(
        revision
            .frame(&wrong_parent.frame(&root().frame_id))
            .is_none()
    );
}

#[test]
fn decoding_rejects_identity_tree_root_and_digest_disagreement() {
    let baseline = serde_json::to_value(revision()).unwrap();
    for (field, value) in [
        ("worldId", serde_json::json!("other")),
        ("worldUri", serde_json::json!("frames://world/other")),
        ("revisionId", serde_json::json!("revision-other")),
        (
            "revisionUri",
            serde_json::json!("frames://world/other/revision/revision-1"),
        ),
        ("revision", serde_json::json!(0)),
        (
            "specDigest",
            serde_json::json!(format!("sha256:{}", "0".repeat(64))),
        ),
        (
            "rootFrameUri",
            serde_json::json!("frames://world/survey/revision/revision-1/frame/camera"),
        ),
        (
            "rootFrameUri",
            serde_json::json!("frames://world/other/revision/revision-1/frame/earth"),
        ),
        (
            "rootFrameUri",
            serde_json::json!("frames://world/survey/revision/revision-other/frame/earth"),
        ),
        (
            "rootFrameUri",
            serde_json::json!("frames://world/survey/revision/revision-1/frame/missing"),
        ),
        ("tree", serde_json::json!({"frames":[]})),
    ] {
        let mut wire = baseline.clone();
        wire[field] = value;
        assert!(
            serde_json::from_value::<FrameWorldRevision>(wire).is_err(),
            "{field}"
        );
    }
    let mut wire = baseline;
    wire["tree"]["frames"][0]["description"] = "tampered".into();
    assert!(serde_json::from_value::<FrameWorldRevision>(wire).is_err());

    let mut source = serde_json::to_value(FrameSourceReference::from(&revision())).unwrap();
    source["revisionId"] = "other".into();
    assert!(serde_json::from_value::<FrameSourceReference>(source).is_err());
}

#[test]
fn summary_admits_only_coherent_empty_or_published_heads() {
    let empty = FrameWorldSummary::new(
        FrameWorldId::parse("survey").unwrap(),
        "Survey".into(),
        revision().created_at(),
    );
    let baseline = serde_json::to_value(&empty).unwrap();
    assert!(baseline.get("headRevisionId").is_none());
    assert_eq!(baseline["revision"], 0);
    assert_eq!(
        serde_json::from_value::<FrameWorldSummary>(baseline.clone()).unwrap(),
        empty
    );
    for (field, value) in [
        ("worldId", serde_json::json!("other")),
        ("worldUri", serde_json::json!("frames://world/other")),
        ("revision", serde_json::json!(1)),
        ("headRevisionId", serde_json::json!("revision-1")),
    ] {
        let mut wire = baseline.clone();
        wire[field] = value;
        assert!(
            serde_json::from_value::<FrameWorldSummary>(wire).is_err(),
            "{field}"
        );
    }
}

#[test]
fn complete_tree_admission_rejects_structural_and_transform_errors() {
    let mut orphan = child("camera", "missing");
    let mut invalid_axes = child("camera", "earth");
    invalid_axes.basis = FrameBasis::Cartesian {
        axes: veoveo_frames_contract::FrameAxes {
            x: veoveo_frames_contract::FrameAxisDirection::Right,
            y: veoveo_frames_contract::FrameAxisDirection::Left,
            z: veoveo_frames_contract::FrameAxisDirection::Up,
        },
    };
    let mut bad_rotation = child("camera", "earth");
    bad_rotation.parent_transform = Some(FrameParentTransform::StaticRigid {
        translation_m: [0.0; 3],
        rotation_xyzw: [0.0; 4],
    });
    let mut nonfinite = child("camera", "earth");
    nonfinite.parent_transform = Some(FrameParentTransform::StaticRigid {
        translation_m: [f64::NAN; 3],
        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
    });
    let mut wrong_root = root();
    wrong_root.basis = FrameBasis::Enu;
    let mut transformed_root = root();
    transformed_root.parent_transform = child("camera", "earth").parent_transform;
    let mut no_transform = child("camera", "earth");
    no_transform.parent_transform = None;
    let mut blank_description = child("camera", "earth");
    blank_description.description = Some(" ".into());
    for frames in [
        vec![],
        vec![root(), root()],
        vec![wrong_root],
        vec![transformed_root],
        vec![root(), orphan.clone()],
        vec![root(), no_transform],
        vec![root(), invalid_axes],
        vec![root(), bad_rotation],
        vec![root(), nonfinite],
        vec![root(), blank_description],
        vec![root(), child("camera", "camera")],
        vec![root(), child("a", "b"), child("b", "a")],
    ] {
        assert!(ValidatedWorldTree::new(FrameWorldTree { frames }).is_err());
    }
    orphan.parent_frame_id = None;
    assert!(
        ValidatedWorldTree::new(FrameWorldTree {
            frames: vec![root(), orphan]
        })
        .is_err()
    );
}

#[test]
fn maximum_depth_tree_is_iterative_and_canonical() {
    let mut frames = vec![root()];
    let mut parent = "earth".to_owned();
    // Reverse sort order forces the first walk to traverse the whole chain.
    for n in (0..9999).rev() {
        let id = format!("child-{n:05}");
        frames.push(child(&id, &parent));
        parent = id;
    }
    let validated = ValidatedWorldTree::new(FrameWorldTree {
        frames: frames.clone(),
    })
    .unwrap();
    frames.reverse();
    assert_eq!(
        ValidatedWorldTree::new(FrameWorldTree {
            frames: frames.clone()
        })
        .unwrap(),
        validated
    );
    frames.push(child("overflow", "earth"));
    assert!(ValidatedWorldTree::new(FrameWorldTree { frames }).is_err());
}

#[test]
fn canonical_digest_preserves_the_published_root_tree_encoding() {
    let tree = ValidatedWorldTree::new(FrameWorldTree {
        frames: vec![root()],
    })
    .unwrap();
    assert_eq!(
        serde_json::to_string(tree.tree()).unwrap(),
        r#"{"frames":[{"frameId":"earth","basis":{"kind":"ecef_wgs84"}}]}"#
    );
    assert_ne!(
        tree.spec_digest().hex(),
        "3e36f3d4c9f0adc10416c017af7b567ab3b9e7fe654ffe8f51b1f697536206bc"
    );
    assert_eq!(
        tree.spec_digest(),
        ValidatedWorldTree::new(
            serde_json::from_slice(&serde_json::to_vec(tree.tree()).unwrap()).unwrap()
        )
        .unwrap()
        .spec_digest()
    );
}

#[test]
fn dynamic_references_accept_independent_producers_and_preserve_components() {
    use veoveo_types::{ResourceUri, ResourceUriBuilder, UriSegment};
    let resource = ResourceUriBuilder::new("independent-producer://session")
        .unwrap()
        .segment(UriSegment::new("flight/a ?#%é").unwrap())
        .query_pair("timeline", "sensor + clock")
        .unwrap()
        .build()
        .unwrap();
    let uri = FrameStreamUri::try_from(resource.clone()).unwrap();
    assert_eq!(uri.as_resource_uri(), &resource);
    assert_eq!(ResourceUri::from(uri.clone()), resource);
    let parts = resource.components().unwrap();
    assert_eq!(parts.path_segments().next().unwrap(), "flight/a ?#%é");
    assert_eq!(parts.query_parameters()["timeline"], "sensor + clock");
    assert_eq!(FrameStreamUri::from(parts), uri);
    let entity = FrameEntityPath::new("/world/vehicle camera/é%2F").unwrap();
    let transform = FrameParentTransform::DynamicStream {
        stream_uri: uri.clone(),
        entity_path: entity.clone(),
    };
    let wire = serde_json::to_value(&transform).unwrap();
    assert_eq!(
        wire,
        serde_json::json!({
            "kind": "dynamic_stream", "streamUri": uri.as_str(), "entityPath": entity.as_str()
        })
    );
    assert_eq!(
        serde_json::from_value::<FrameParentTransform>(wire).unwrap(),
        transform
    );
    for value in [
        "uav-sim://session/uav-showcase",
        "recording://recording/019ffdb2-0598-7476-96d3-f3d7b0769f9e",
        "independent-producer://session/flight?future_parameter=value",
    ] {
        let uri = FrameStreamUri::parse(value).unwrap();
        assert_eq!(uri.as_str(), value);
    }
}

#[test]
fn dynamic_reference_decoding_rejects_templates_credentials_and_normalization() {
    for value in [
        "",
        "relative/path",
        "producer:///missing-authority",
        "Producer://session/run",
        "producer://session/a/../run",
        "producer://session/{run}",
        "producer://session/run#fragment",
        "producer://user:private-token@session/run",
        "producer://session:8080/run",
        "producer://session/run name",
        "producer://session/é",
        "producer://session/run%",
        "producer://session/run%GG",
        "producer://session/%FF",
        "producer://session/%00",
        "producer://session/run?at=1&at=2",
        "producer://session/run?at=1&%61t=2",
        "producer://session/run?=value",
        "producer://session/run?at=%00",
    ] {
        assert!(FrameStreamUri::parse(value).is_err(), "{value}");
        assert!(
            serde_json::from_value::<FrameParentTransform>(serde_json::json!({
                "kind": "dynamic_stream", "streamUri": value, "entityPath": "vehicle/body"
            }))
            .is_err(),
            "{value}"
        );
    }
    let error = FrameStreamUri::parse("producer://user:private-token@session/run")
        .unwrap_err()
        .to_string();
    assert!(!error.contains("private-token"));
}

#[test]
fn entity_paths_have_byte_bounds_and_preserve_the_producer_selector() {
    for value in [
        "/world/uav/body",
        "vehicle with spaces",
        " leading space",
        "é",
        &"é".repeat(1024),
    ] {
        let path = FrameEntityPath::new(value).unwrap();
        assert_eq!(path.as_str(), value);
        assert_eq!(serde_json::to_value(path).unwrap(), value);
    }
    for value in ["", " ", "\t", "body\nposition", "body\0", &"é".repeat(1025)] {
        assert!(FrameEntityPath::new(value).is_err());
        assert!(
            serde_json::from_value::<FrameParentTransform>(serde_json::json!({
                "kind": "dynamic_stream", "streamUri": "producer://session/run", "entityPath": value
            }))
            .is_err()
        );
    }
}

#[test]
fn existing_uav_dynamic_world_keeps_its_wire_tree() {
    let scenario: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../showcase/uav-sim/scenarios/new-york-aerial.json"
    ))
    .unwrap();
    let wire = scenario["world"]["tree"].clone();
    let tree: FrameWorldTree = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&tree).unwrap(), wire);
    let admitted = ValidatedWorldTree::new(tree).unwrap();
    assert_ne!(
        admitted.spec_digest().hex(),
        "7941fba19b5edcce73d37e1c1bef82d98f42e9c79b5fdb19671e21d1924e3b8a"
    );
    let revision = FrameWorldRevision::new(
        FrameWorldRevisionUri::new(
            &FrameWorldId::parse("showcase").unwrap(),
            &FrameWorldRevisionId::parse("revision-1").unwrap(),
        ),
        1.try_into().unwrap(),
        admitted.clone(),
        "2026-01-01T00:00:00Z".parse().unwrap(),
    );
    let mut wire = serde_json::to_value(&revision).unwrap();
    for _ in 0..3 {
        let bytes = serde_json::to_vec(&wire).unwrap();
        let decoded: FrameWorldRevision = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, revision);
        wire = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            serde_json::from_value::<FrameWorldRevision>(wire.clone()).unwrap(),
            revision
        );
    }
    let dynamic = admitted
        .tree()
        .frames
        .iter()
        .filter_map(|frame| match &frame.parent_transform {
            Some(FrameParentTransform::DynamicStream {
                stream_uri,
                entity_path,
            }) => Some((stream_uri, entity_path)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(dynamic.len(), 4);
    for (uri, entity) in dynamic {
        assert_eq!(uri.as_str(), "uav-sim://session/uav-showcase");
        assert!(
            entity
                .as_str()
                .starts_with("/world/uav-sim/uav-showcase/vehicle/")
        );
    }
}

#[test]
fn frame_world_input_policy_preserves_route_and_identity_errors() {
    use veoveo_frames_contract::{FrameUriError, FrameWorldUri};
    assert!(matches!(
        FrameWorldUri::parse("frames://world/%21invalid"),
        Err(FrameUriError::Route)
    ));
    assert!(matches!(
        FrameWorldUri::parse("frames://world/!invalid"),
        Err(FrameUriError::Identity(_))
    ));
    assert!(matches!(
        FrameWorldUri::parse("frames://other/!invalid"),
        Err(FrameUriError::Route)
    ));
    assert!(matches!(
        FrameWorldUri::parse("frames://world/%2E%2E"),
        Err(FrameUriError::Components(_))
    ));
    let world = FrameWorldUri::new(&FrameWorldId::parse("survey").unwrap());
    assert_eq!(
        FrameWorldUri::RESOURCE_ROUTES[0]
            .discovery_template()
            .unwrap(),
        "frames://world/{world_id}"
    );
    assert_eq!(world.as_str(), "frames://world/survey");
}

#[test]
fn current_frames_wire_refuses_retired_keys_at_roots_and_nested_variants() {
    let revision = revision();
    let child = revision
        .tree()
        .frames
        .iter()
        .position(|frame| frame.frame_id == FrameId::parse("camera").unwrap())
        .expect("producer includes camera child");
    let baseline = serde_json::to_value(&revision).unwrap();
    for (path, retired) in [
        ("/worldId".to_owned(), "world_id"),
        ("/revisionUri".to_owned(), "revision_uri"),
        ("/specDigest".to_owned(), "spec_digest"),
        (format!("/tree/frames/{child}/frameId"), "frame_id"),
        (
            format!("/tree/frames/{child}/parentFrameId"),
            "parent_frame_id",
        ),
        (
            format!("/tree/frames/{child}/parentTransform/translationM"),
            "translation_m",
        ),
        (
            format!("/tree/frames/{child}/parentTransform/rotationXyzw"),
            "rotation_xyzw",
        ),
    ] {
        let (parent, current) = path.rsplit_once('/').unwrap();
        for mixed in [false, true] {
            let mut bad = baseline.clone();
            let object = if parent.is_empty() {
                &mut bad
            } else {
                bad.pointer_mut(parent).unwrap()
            };
            let object = object.as_object_mut().unwrap();
            let value = object[current].clone();
            if !mixed {
                object.remove(current);
            }
            object.insert(retired.into(), value);
            assert!(
                serde_json::from_value::<FrameWorldRevision>(bad).is_err(),
                "{path}, mixed={mixed}"
            );
        }
    }
}

// Optional owner producer capture keeps dependent fixture digests derived from
// the same checked tree constructor used by Frames publication.
#[test]
fn dependent_uav_fixture_revisions_are_produced_by_checked_frames() {
    let mut cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../servers/uav-sim-mcp/testdata/controlled-inputs.json"
    ))
    .unwrap();
    for case in cases.as_array_mut().unwrap() {
        let Some(wire) = case.pointer_mut("/arguments/world_revision") else {
            continue;
        };
        let tree: FrameWorldTree = serde_json::from_value(wire["tree"].clone()).unwrap();
        let revision = FrameWorldRevision::new(
            serde_json::from_value(wire["revisionUri"].clone()).unwrap(),
            wire["revision"].as_u64().unwrap().try_into().unwrap(),
            ValidatedWorldTree::new(tree).unwrap(),
            serde_json::from_value(wire["createdAt"].clone()).unwrap(),
        );
        let produced = serde_json::to_value(&revision).unwrap();
        if std::env::var_os("VEOVEO_FRAMES_FIXTURE_CAPTURE").is_none() {
            assert_eq!(
                *wire, produced,
                "dependent UAV revision differs from its Frames producer"
            );
        }
        *wire = produced;
    }
    if let Some(output) = std::env::var_os("VEOVEO_FRAMES_FIXTURE_CAPTURE") {
        std::fs::write(output, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}
