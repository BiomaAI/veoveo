use super::*;
use crate::{
    composition::test_support as resolved,
    contract::{test_support as fixture, *},
};
use serde_json::json;

#[test]
fn snapshot_admission_checks_view_parent_layer_and_digest() {
    let valid = ViewCaptureSnapshot::new(fixture::view(), resolved::resolved()).unwrap();
    let wire = serde_json::to_value(valid).unwrap();
    for (path, replacement) in [
        ("/view/scene_layer", json!("other")),
        (
            "/composition/record/created_at",
            json!("2026-09-29T10:00:00Z"),
        ),
        (
            "/view/composition_digest_sha256",
            json!(Sha256Digest::from_bytes(b"other")),
        ),
        (
            "/composition/resolved_overlays/0/overlay/style/marker_size_meters",
            json!(200.0),
        ),
    ] {
        let mut invalid = wire.clone();
        *invalid.pointer_mut(path).unwrap() = replacement;
        assert!(
            serde_json::from_value::<ViewCaptureSnapshot>(invalid).is_err(),
            "accepted {path}"
        );
    }
    let mut invalid = wire;
    let id = SceneCompositionId::from_stable_key(b"other");
    invalid["view"]["composition_id"] = json!(id);
    invalid["view"]["composition_uri"] = json!(CompositionUri::new(id));
    assert!(serde_json::from_value::<ViewCaptureSnapshot>(invalid).is_err());
}

#[test]
fn snapshot_owner_requires_principal_tenant_and_work_context() {
    let snapshot = ViewCaptureSnapshot::new(fixture::view(), resolved::resolved()).unwrap();
    let authority = fixture::authority();
    let owner = ResourceOwner {
        principal_id: authority.principal_id,
        tenant: authority.invocation.tenant,
        work_context: authority.invocation.work_context,
    };
    snapshot.require_owner(&owner).unwrap();
    for other in [
        ResourceOwner {
            principal_id: veoveo_types::PrincipalId::parse("other").unwrap(),
            ..owner.clone()
        },
        ResourceOwner {
            tenant: veoveo_types::TenantId::parse("other").unwrap(),
            ..owner.clone()
        },
        ResourceOwner {
            work_context: veoveo_types::WorkContextId::parse("other").unwrap(),
            ..owner.clone()
        },
    ] {
        assert!(snapshot.require_owner(&other).is_err());
    }
}

#[test]
fn decoded_artifact_snapshot_preserves_captured_camera_after_live_revision_changes() {
    let composition = resolved::with_artifact();
    let mut view = ViewRecord::new(
        ViewId::parse("view-1").unwrap(),
        composition.record(),
        fixture::camera(),
        fixture::now(),
    )
    .unwrap();
    let snapshot = ViewCaptureSnapshot::new(view.clone(), composition).unwrap();
    let bytes = serde_json::to_vec(&snapshot).unwrap();
    let mut pose = view.resolved_camera().clone();
    pose.orientation.pitch_degrees = -20.0;
    view.replace_camera(CameraDefinition::Pose(pose), fixture::now())
        .unwrap();
    let recovered: ViewCaptureSnapshot = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(recovered.view().revision(), 1);
    assert_ne!(recovered.view().resolved_camera(), view.resolved_camera());
    assert_eq!(serde_json::to_vec(&recovered).unwrap(), bytes);
}
