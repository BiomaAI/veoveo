use super::*;
use crate::{
    composition::test_support as resolved,
    contract::{test_support as fixture, *},
};
use serde_json::json;

fn request() -> ViewCaptureTaskRequest {
    let snapshot = ViewCaptureSnapshot::new(fixture::view(), resolved::resolved()).unwrap();
    let request: CaptureFrameRequest = serde_json::from_value(json!({
        "view_id":"view-1", "expected_revision":1, "scene_time":fixture::now(),
        "policy":{"width_px":100,"height_px":100,"max_screen_error_px":16.0,"deadline_ms":1000}
    }))
    .unwrap();
    ViewCaptureTaskRequest::new(request, snapshot).unwrap()
}

fn owner() -> TaskOwner {
    TaskOwner {
        principal_key: "operator".into(),
        principal_kind: veoveo_task_runtime::PrincipalKind::User,
        issuer: "https://identity.example".into(),
        subject: "operator".into(),
        profile: "operator".into(),
        tenant_key: Some("tenant".into()),
        data_labels: Default::default(),
        authority: fixture::authority().invocation,
    }
}

#[test]
fn saved_request_must_name_the_snapshot_view_and_revision() {
    let wire = serde_json::to_value(request()).unwrap();
    serde_json::from_value::<ViewCaptureTaskRequest>(wire.clone()).unwrap();
    for (path, replacement) in [
        ("/request/view_id", json!("other")),
        ("/request/expected_revision", json!(2)),
    ] {
        let mut invalid = wire.clone();
        *invalid.pointer_mut(path).unwrap() = replacement;
        assert!(serde_json::from_value::<ViewCaptureTaskRequest>(invalid).is_err());
    }
    let mut mismatch = request().request().clone();
    mismatch.expected_revision = 2;
    assert!(ViewCaptureTaskRequest::new(mismatch, request().snapshot().clone()).is_err());
}

#[test]
fn task_owner_admission_rejects_cross_owner_recovery_and_accepts_changed_policy() {
    let request = request();
    request.validate_owner(&owner()).unwrap();
    let mut changed = owner();
    changed.authority.policy_revision = veoveo_types::PolicyVersion::parse("r2").unwrap();
    request.validate_owner(&changed).unwrap();
    for changed in [
        TaskOwner {
            principal_key: "other".into(),
            ..owner()
        },
        TaskOwner {
            principal_key: String::new(),
            ..owner()
        },
        TaskOwner {
            authority: veoveo_types::InvocationAuthority {
                tenant: veoveo_types::TenantId::parse("other").unwrap(),
                ..owner().authority
            },
            ..owner()
        },
        TaskOwner {
            authority: veoveo_types::InvocationAuthority {
                work_context: veoveo_types::WorkContextId::parse("other").unwrap(),
                ..owner().authority
            },
            ..owner()
        },
    ] {
        assert!(request.validate_owner(&changed).is_err());
    }
}
