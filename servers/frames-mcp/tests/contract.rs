//! The hosted library exposes the same values as the domain owner.
#[test]
fn contract_feature_exposes_shared_frames_types() {
    use veoveo_frames_contract::{
        FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri,
    };
    let revision = FrameWorldRevisionUri::new(
        &FrameWorldId::new("survey").unwrap(),
        &FrameWorldRevisionId::new("revision-1").unwrap(),
    );
    let frame: veoveo_frames_mcp::contract::WorldFrameUri =
        revision.frame(&FrameId::new("camera").unwrap());
    assert_eq!(frame.revision_uri(), revision);
    let _: veoveo_frames_contract::WorldFrameUri = frame;
}
