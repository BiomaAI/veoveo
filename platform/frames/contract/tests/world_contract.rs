use veoveo_frames_contract::{
    FrameBasis, FrameEntityPath, FrameId, FrameNode, FrameParentTransform, FrameSourceReference,
    FrameStreamUri, FrameWorldId, FrameWorldRevision, FrameWorldRevisionId, FrameWorldRevisionUri,
    FrameWorldSummary, FrameWorldTree, ValidatedWorldTree,
};

fn root() -> FrameNode {
    FrameNode {
        frame_id: FrameId::new("earth").unwrap(),
        basis: FrameBasis::EcefWgs84,
        parent_frame_id: None,
        parent_transform: None,
        description: None,
    }
}

fn child(id: &str, parent: &str) -> FrameNode {
    FrameNode {
        frame_id: FrameId::new(id).unwrap(),
        basis: FrameBasis::Enu,
        parent_frame_id: Some(FrameId::new(parent).unwrap()),
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
            &FrameWorldId::new("survey").unwrap(),
            &FrameWorldRevisionId::new("revision-1").unwrap(),
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
            "world_id":"survey", "world_uri":"frames://world/survey", "display_name":"Survey",
            "description":"Complete tree", "head_revision_id":"revision-1", "revision":1,
            "created_at":"2026-01-01T00:00:00Z", "updated_at":"2026-01-01T00:00:00Z"
        })
    );
    assert_eq!(
        serde_json::from_value::<FrameWorldSummary>(wire).unwrap(),
        summary
    );
    let wire = serde_json::to_value(&revision).unwrap();
    assert_eq!(wire["world_uri"], "frames://world/survey");
    assert_eq!(
        wire["root_frame_uri"],
        "frames://world/survey/revision/revision-1/frame/earth"
    );
    assert_eq!(
        serde_json::from_value::<FrameWorldRevision>(wire).unwrap(),
        revision
    );
    let source = FrameSourceReference::from(&revision);
    let wire = serde_json::to_value(&source).unwrap();
    assert_eq!(wire["revision_id"], "revision-1");
    assert_eq!(
        serde_json::from_value::<FrameSourceReference>(wire).unwrap(),
        source
    );
    assert_eq!(
        revision.frame(&revision.root_frame_uri()).unwrap().frame_id,
        root().frame_id
    );
    let wrong_parent = FrameWorldRevisionUri::new(
        &FrameWorldId::new("other").unwrap(),
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
        ("world_id", serde_json::json!("other")),
        ("world_uri", serde_json::json!("frames://world/other")),
        ("revision_id", serde_json::json!("revision-other")),
        (
            "revision_uri",
            serde_json::json!("frames://world/other/revision/revision-1"),
        ),
        ("revision", serde_json::json!(0)),
        (
            "spec_digest",
            serde_json::json!(format!("sha256:{}", "0".repeat(64))),
        ),
        (
            "root_frame_uri",
            serde_json::json!("frames://world/survey/revision/revision-1/frame/camera"),
        ),
        (
            "root_frame_uri",
            serde_json::json!("frames://world/other/revision/revision-1/frame/earth"),
        ),
        (
            "root_frame_uri",
            serde_json::json!("frames://world/survey/revision/revision-other/frame/earth"),
        ),
        (
            "root_frame_uri",
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
    source["revision_id"] = "other".into();
    assert!(serde_json::from_value::<FrameSourceReference>(source).is_err());
}

#[test]
fn summary_admits_only_coherent_empty_or_published_heads() {
    let empty = FrameWorldSummary::new(
        FrameWorldId::new("survey").unwrap(),
        "Survey".into(),
        revision().created_at(),
    );
    let baseline = serde_json::to_value(&empty).unwrap();
    assert!(baseline.get("head_revision_id").is_none());
    assert_eq!(baseline["revision"], 0);
    assert_eq!(
        serde_json::from_value::<FrameWorldSummary>(baseline.clone()).unwrap(),
        empty
    );
    for (field, value) in [
        ("world_id", serde_json::json!("other")),
        ("world_uri", serde_json::json!("frames://world/other")),
        ("revision", serde_json::json!(1)),
        ("head_revision_id", serde_json::json!("revision-1")),
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
        r#"{"frames":[{"frame_id":"earth","basis":{"kind":"ecef_wgs84"}}]}"#
    );
    assert_eq!(
        tree.spec_digest().hex(),
        "3e36f3d4c9f0adc10416c017af7b567ab3b9e7fe654ffe8f51b1f697536206bc"
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
            "kind": "dynamic_stream", "stream_uri": uri.as_str(), "entity_path": entity.as_str()
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
                "kind": "dynamic_stream", "stream_uri": value, "entity_path": "vehicle/body"
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
        assert!(serde_json::from_value::<FrameParentTransform>(serde_json::json!({
            "kind": "dynamic_stream", "stream_uri": "producer://session/run", "entity_path": value
        })).is_err());
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
