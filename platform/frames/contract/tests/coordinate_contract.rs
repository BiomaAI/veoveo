fn check_schema<T: schemars::JsonSchema>(baseline: &serde_json::Value, name: &str) {
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(T)).unwrap(),
        baseline[name],
        "{name}"
    );
}
#[test]
fn coordinate_contract_schemas_preserve_published_wire_shapes() {
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/coordinate-contract.schema.json")).unwrap();

    check_schema::<veoveo_frames_contract::CoordinateOperationId>(
        &baseline,
        "CoordinateOperationId",
    );
    check_schema::<veoveo_frames_contract::CoordinateOperationKind>(
        &baseline,
        "CoordinateOperationKind",
    );
    check_schema::<veoveo_frames_contract::CoordinateOperationProvenance>(
        &baseline,
        "CoordinateOperationProvenance",
    );
    check_schema::<veoveo_frames_contract::CoordinateOperationRef>(
        &baseline,
        "CoordinateOperationRef",
    );
    check_schema::<veoveo_frames_contract::CoordinateSpace>(&baseline, "CoordinateSpace");
    check_schema::<veoveo_frames_contract::FrameAxes>(&baseline, "FrameAxes");
    check_schema::<veoveo_frames_contract::FrameAxisDirection>(&baseline, "FrameAxisDirection");
    check_schema::<veoveo_frames_contract::FrameBasis>(&baseline, "FrameBasis");
    check_schema::<veoveo_frames_contract::FrameId>(&baseline, "FrameId");
    check_schema::<veoveo_frames_contract::FrameNode>(&baseline, "FrameNode");
    check_schema::<veoveo_frames_contract::FrameParentTransform>(&baseline, "FrameParentTransform");
    check_schema::<veoveo_frames_contract::FrameWorldId>(&baseline, "FrameWorldId");
    check_schema::<veoveo_frames_contract::FrameWorldRevision>(&baseline, "FrameWorldRevision");
    check_schema::<veoveo_frames_contract::FrameWorldRevisionId>(&baseline, "FrameWorldRevisionId");
    check_schema::<veoveo_frames_contract::FrameWorldRevisionUri>(
        &baseline,
        "FrameWorldRevisionUri",
    );
    check_schema::<veoveo_frames_contract::FrameWorldTree>(&baseline, "FrameWorldTree");
    check_schema::<veoveo_frames_contract::FrameWorldUri>(&baseline, "FrameWorldUri");
    check_schema::<veoveo_frames_contract::Wgs84Position>(&baseline, "Wgs84Position");
    check_schema::<veoveo_frames_contract::WorldFrameUri>(&baseline, "WorldFrameUri");
    check_schema::<veoveo_frames_contract::EcefPosition>(&baseline, "EcefPosition");
    check_schema::<veoveo_frames_contract::WorldFramePosition>(&baseline, "WorldFramePosition");
    check_schema::<veoveo_frames_contract::CoordinatePoint>(&baseline, "CoordinatePoint");
    check_schema::<veoveo_frames_contract::ConvertFrameRequest>(&baseline, "ConvertFrameRequest");
    check_schema::<veoveo_frames_contract::ConvertFrameOutput>(&baseline, "ConvertFrameOutput");
    check_schema::<veoveo_frames_contract::FrameSourceReference>(&baseline, "FrameSourceReference");
    check_schema::<veoveo_frames_contract::CreateWorldRequest>(&baseline, "CreateWorldRequest");
    check_schema::<veoveo_frames_contract::FrameWorldSummary>(&baseline, "FrameWorldSummary");
    check_schema::<veoveo_frames_contract::CreateWorldOutput>(&baseline, "CreateWorldOutput");
    check_schema::<veoveo_frames_contract::PublishWorldRequest>(&baseline, "PublishWorldRequest");
    check_schema::<veoveo_frames_contract::PublishWorldOutput>(&baseline, "PublishWorldOutput");
    check_schema::<veoveo_frames_contract::BatchTransformRequest>(
        &baseline,
        "BatchTransformRequest",
    );
    check_schema::<veoveo_frames_contract::BatchTransformOutput>(&baseline, "BatchTransformOutput");
}

use veoveo_frames_contract::{
    FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri, FrameWorldUri,
    WorldFrameUri,
};
use veoveo_types::{ResourceAddress, ResourceUri};

#[test]
fn world_catalog_cursors_preserve_typed_positions_and_round_trip() {
    use veoveo_frames_contract::{FrameWorldCursor, FrameWorldsUri};
    let root = FrameWorldsUri::new(None);
    assert_eq!(root.as_str(), FrameWorldsUri::ROOT);
    assert_eq!(FrameWorldsUri::parse(root.as_str()).unwrap(), root);
    for value in ["mission-alpha", "ENU:mission.01", &"x".repeat(128)] {
        let id = FrameWorldId::parse(value).unwrap();
        let cursor = FrameWorldCursor::new(&id);
        assert_eq!(cursor.after(), &id);
        assert_eq!(FrameWorldCursor::parse(cursor.as_str()).unwrap(), cursor);
        let uri = FrameWorldsUri::new(Some(&cursor));
        assert_eq!(FrameWorldsUri::parse(uri.as_str()).unwrap(), uri);
        assert_eq!(uri.cursor(), Some(&cursor));
        assert_eq!(
            serde_json::from_str::<FrameWorldsUri>(&serde_json::to_string(&uri).unwrap()).unwrap(),
            uri
        );
        assert_eq!(
            <FrameWorldsUri as ResourceAddress>::parse(&uri.to_uri().unwrap()).unwrap(),
            uri
        );
    }
}

#[test]
fn world_catalog_rejects_wrong_cursor_envelopes_and_ambiguous_uris() {
    use veoveo_frames_contract::{FrameWorldCursor, FrameWorldsUri};
    for value in [
        serde_json::json!({"version":2,"collection":"frames://worlds","after":"world"}),
        serde_json::json!({"version":1,"collection":"frames://usage","after":"world"}),
        serde_json::json!({"version":1,"collection":"frames://worlds","after":".."}),
        serde_json::json!({"version":1,"collection":"frames://worlds","after":"world","extra":true}),
    ] {
        let wire = hex::encode(serde_json::to_vec(&value).unwrap());
        assert!(FrameWorldCursor::parse(wire).is_err());
    }
    for wire in ["", "zz", "a", &"00".repeat(513)] {
        assert!(FrameWorldCursor::parse(wire).is_err());
    }
    let cursor = FrameWorldCursor::new(&FrameWorldId::parse("world").unwrap());
    for value in [
        "frames://worlds/".to_owned(),
        "frames://worlds?".to_owned(),
        "frames://worlds?cursor=".to_owned(),
        "frames://worlds?unknown=1".to_owned(),
        "frames://user@worlds".to_owned(),
        "frames://worlds:42".to_owned(),
        "frames://worlds#fragment".to_owned(),
        "other://worlds".to_owned(),
        format!(
            "frames://worlds?cursor={}&cursor={}",
            cursor.as_str(),
            cursor.as_str()
        ),
        format!(
            "frames://worlds?cursor={}&%63ursor={}",
            cursor.as_str(),
            cursor.as_str()
        ),
        format!("frames://worlds?cursor={}&unknown=1", cursor.as_str()),
        format!("frames://worlds?%63ursor={}", cursor.as_str()),
    ] {
        assert!(FrameWorldsUri::parse(&value).is_err(), "{value}");
    }
}

#[test]
fn world_revision_and_frame_addresses_preserve_ids_and_wire_spelling() {
    for suffix in [
        "mission-alpha",
        "ENU:mission.01",
        "_",
        "a-b.c:9",
        &"x".repeat(128),
    ] {
        let world_id = FrameWorldId::parse(suffix).unwrap();
        let revision_id = FrameWorldRevisionId::parse(suffix).unwrap();
        let frame_id = FrameId::parse(suffix).unwrap();
        let world = FrameWorldUri::new(&world_id);
        let revision = world.revision(&revision_id);
        let frame = revision.frame(&frame_id);
        assert_eq!(world.as_str(), format!("frames://world/{suffix}"));
        assert_eq!(
            revision.as_str(),
            format!("frames://world/{suffix}/revision/{suffix}")
        );
        assert_eq!(
            frame.as_str(),
            format!("frames://world/{suffix}/revision/{suffix}/frame/{suffix}")
        );
        assert_eq!(world.world_id(), world_id);
        assert_eq!(revision.world_id(), world_id);
        assert_eq!(revision.revision_id(), revision_id);
        assert_eq!(frame.revision_uri(), revision);
        assert_eq!(frame.frame_id(), frame_id);
        fn roundtrip<T: ResourceAddress + PartialEq + std::fmt::Debug>(value: &T) {
            assert_eq!(T::parse(&value.to_uri().unwrap()).unwrap(), *value);
        }
        roundtrip(&world);
        roundtrip(&revision);
        roundtrip(&frame);
        assert_eq!(
            serde_json::from_value::<WorldFrameUri>(serde_json::to_value(&frame).unwrap()).unwrap(),
            frame
        );
    }
}

#[test]
fn admission_rejects_relative_and_malformed_frame_identities() {
    for invalid in [
        "",
        ".",
        "..",
        "bad/id",
        "bad id",
        "bad%id",
        "x?y",
        "x#y",
        "x@y",
        "ü",
        &"x".repeat(129),
    ] {
        assert!(FrameId::parse(invalid).is_err(), "{invalid}");
        assert!(veoveo_frames_contract::CoordinateOperationId::parse(invalid).is_err());
        assert!(FrameWorldId::parse(invalid).is_err(), "{invalid}");
        assert!(FrameWorldRevisionId::parse(invalid).is_err(), "{invalid}");
        assert!(serde_json::from_value::<FrameWorldId>(serde_json::json!(invalid)).is_err());
    }
}

#[test]
fn address_parsing_rejects_aliases_wrong_routes_and_unexpected_components() {
    for invalid in [
        "frames://world",
        "frames://world/",
        "frames://world/a/extra",
        "map://world/a",
        "frames://user@world/a",
        "frames://world:123/a",
        "frames://world/a?",
        "frames://world/a?cursor=x",
        "frames://world/a?x=1&x=2",
        "frames://world/a#fragment",
        "frames://world/%61",
        "frames://world/a%2Fb",
        "frames://world/../a",
        "frames://world/a/.",
        "frames://world/a\\b",
        "frames://world/a%",
        "frames://world/{id}",
        "FRAMES://world/a",
        " frames://world/a",
        "frames://world/a\n",
    ] {
        assert!(FrameWorldUri::parse(invalid).is_err(), "{invalid}");
    }
    for invalid in [
        "frames://world/a/revision/b/frame/c",
        "frames://world/a/revisions/b",
        "frames://world/a/revision/",
        "frames://world/a/revision/..",
        "frames://world/a/revision/b?x=1",
        "frames://world/a/revision/%62",
    ] {
        assert!(FrameWorldRevisionUri::parse(invalid).is_err(), "{invalid}");
    }
    for invalid in [
        "frames://world/a/frame/c",
        "frames://world/a/revision/b/frames/c",
        "frames://world/a/revision/b/frame/c/extra",
        "frames://world/a/revision/b/frame/",
        "frames://world/a/revision/b/frame/%2E",
        "frames://world/a/revision/b/frame/c?x=1",
    ] {
        let wire = ResourceUri::new(invalid).unwrap();
        assert!(
            <WorldFrameUri as ResourceAddress>::parse(&wire).is_err(),
            "{invalid}"
        );
        assert!(serde_json::from_value::<WorldFrameUri>(serde_json::json!(invalid)).is_err());
    }
}

#[test]
fn operation_addresses_and_references_keep_their_identity_in_agreement() {
    use veoveo_frames_contract::{
        CoordinateOperationId, CoordinateOperationRef, FrameOperationUri,
    };
    use veoveo_types::ResourceAddress;
    let id = CoordinateOperationId::parse("op-01950000-0000-7000-8000-000000000001").unwrap();
    let uri = FrameOperationUri::new(&id);
    assert_eq!(
        uri.as_str(),
        "frames://operation/op-01950000-0000-7000-8000-000000000001"
    );
    assert_eq!(FrameOperationUri::parse(uri.as_str()).unwrap(), uri);
    assert_eq!(
        <FrameOperationUri as ResourceAddress>::parse(&uri.to_uri().unwrap()).unwrap(),
        uri
    );
    let reference =
        CoordinateOperationRef::new(id.clone(), "2026-01-01T00:00:00Z".parse().unwrap());
    let wire = serde_json::to_value(&reference).unwrap();
    assert_eq!(
        wire,
        serde_json::json!({"operation_id":id,"operation_uri":uri,"created_at":"2026-01-01T00:00:00Z"})
    );
    assert_eq!(
        serde_json::from_value::<CoordinateOperationRef>(wire.clone()).unwrap(),
        reference
    );
    let mut wrong = wire;
    wrong["operation_id"] = serde_json::json!("op-other");
    assert!(serde_json::from_value::<CoordinateOperationRef>(wrong).is_err());
    for wrong in [
        "frames://operation",
        "frames://operation/",
        "frames://operation/x/extra",
        "frames://operation/x?",
        "frames://operation/x?q=y",
        "frames://operation/x#fragment",
        "frames://operation/%78",
        "frames://user@operation/x",
        "frames://operation:123/x",
        "Frames://operation/x",
        "frames://operation/..",
        "map://operation/x",
    ] {
        assert!(FrameOperationUri::parse(wrong).is_err(), "{wrong}");
    }
}
