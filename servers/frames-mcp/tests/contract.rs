//! The hosted library exposes the same values as the domain owner.
#[test]
fn contract_feature_exposes_shared_frames_types() {
    use veoveo_frames_contract::{
        FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri,
    };
    let revision = FrameWorldRevisionUri::new(
        &FrameWorldId::parse("survey").unwrap(),
        &FrameWorldRevisionId::parse("revision-1").unwrap(),
    );
    let frame: veoveo_frames_mcp::contract::WorldFrameUri =
        revision.frame(&FrameId::parse("camera").unwrap());
    assert_eq!(frame.revision_uri(), revision);
    let _: veoveo_frames_contract::WorldFrameUri = frame;
}

#[test]
fn hosted_addresses_compose_the_domain_owners_and_reject_uri_aliases() {
    use veoveo_artifact_contract::ArtifactId;
    use veoveo_frames_mcp::contract::*;
    use veoveo_types::{ResourceAddress, TaskId};
    let world = FrameWorldUri::new(&FrameWorldId::parse("survey").unwrap());
    let revision = world.revision(&FrameWorldRevisionId::parse("revision-1").unwrap());
    let task = TaskId::new();
    let cursor = FrameWorldCursor::new(&world.world_id());
    let usage_cursor = FrameUsageCursor::new(task).unwrap();
    let addresses = [
        FramesResource::Docs,
        FramesResource::Document(FramesDocument::Agents),
        FramesResource::Document(FramesDocument::Design),
        FramesResource::Contract,
        FramesResource::WorkspaceApp,
        FramesResource::Worlds(FrameWorldsUri::new(None)),
        FramesResource::Worlds(FrameWorldsUri::new(Some(&cursor))),
        FramesResource::World(world.clone()),
        FramesResource::Revision(revision.clone()),
        FramesResource::Frame(revision.frame(&FrameId::parse("camera").unwrap())),
        FramesResource::Operation(FrameOperationUri::new(
            &CoordinateOperationId::parse("conversion-1").unwrap(),
        )),
        FramesResource::Usage(FrameUsageIndexUri::new(None)),
        FramesResource::Usage(FrameUsageIndexUri::new(Some(&usage_cursor))),
        FramesResource::TaskUsage(FrameTaskUsageUri::new(task).unwrap()),
        FramesResource::Artifact(ArtifactId::new()),
    ];
    for address in addresses {
        let uri = address.to_uri().unwrap();
        assert_eq!(FramesResource::parse(uri.as_str()).unwrap(), address);
        assert_eq!(
            serde_json::from_value::<FramesResource>(serde_json::json!(uri)).unwrap(),
            address
        );
        for invalid in [
            format!("{uri}#fragment"),
            format!("{uri}/extra"),
            uri.as_str().replace("frames", "other"),
        ] {
            if invalid != uri.as_str() {
                assert!(FramesResource::parse(&invalid).is_err(), "{invalid}");
            }
        }
    }
    for invalid in [
        "frames://docs/unknown",
        "frames://docs/%61gents",
        "frames://contract?extra=x",
        "frames://artifact/00000000-0000-0000-0000-000000000000",
        "frames://world/survey?cursor=x",
        "frames://worlds?unknown=x",
        "frames://world/survey/revision",
        "ui://frames/workspace.html?token=secret",
        "frames://world/survey/revision/revision-1/frame/camera/extra",
    ] {
        assert!(FramesResource::parse(invalid).is_err(), "{invalid}");
    }
}
