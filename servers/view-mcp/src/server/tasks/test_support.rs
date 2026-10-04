use super::*;
use serde_json::json;

#[path = "../../../../../testing/fixtures/store.rs"]
pub(super) mod fixture;

pub(super) fn identity() -> GatewayInternalIdentity {
    let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../../testing/fixtures/gateway-request-context.json"
    ))
    .unwrap();
    let mut wire = fixtures[0].clone();
    wire["request_context"] = serde_json::Value::Null;
    for (field, value) in [
        ("issuer", json!("https://issuer.test")),
        ("profile", json!("operator")),
        ("server", json!("view")),
        ("jwt_id", json!("view-task-selection-test")),
        ("issued_at", json!("2026-09-28T00:00:00Z")),
        ("not_before", json!("2026-09-28T00:00:00Z")),
        ("expires_at", json!("2026-09-28T01:00:00Z")),
    ] {
        wire[field] = value;
    }
    serde_json::from_value(wire).unwrap()
}

#[path = "../../../../../testing/fixtures/connection_switch.rs"]
pub(super) mod connection_switch;

pub(super) fn capture(identity: &GatewayInternalIdentity) -> ViewCaptureTaskRequest {
    use crate::composition::{ResolvedSceneComposition, ResolvedSceneOverlay};
    use crate::contract::{test_support as data, *};
    let record = SceneComposition::new(
        data::request(),
        SceneCompositionAuthority {
            principal_id: identity.actor.id.clone(),
            invocation: identity.authority.clone(),
        },
        data::now(),
    )
    .unwrap();
    let overlays = record
        .overlays()
        .iter()
        .map(|overlay| {
            let SceneOverlayGeometrySource::Inline { geometry } = &overlay.geometry else {
                panic!("inline fixture");
            };
            ResolvedSceneOverlay {
                overlay: overlay.clone(),
                geometry: geometry.clone(),
            }
        })
        .collect();
    let view = ViewRecord::new(
        ViewId::parse("view-1").unwrap(),
        &record,
        data::camera(),
        data::now(),
    )
    .unwrap();
    let resolved = ResolvedSceneComposition::new(record, overlays, Default::default()).unwrap();
    let request = CaptureFrameRequest {
        view_id: view.view_id().clone(),
        expected_revision: view.revision(),
        scene_time: data::now(),
        policy: CapturePolicy {
            width_px: 64,
            height_px: 64,
            max_screen_error_px: 8.0,
            deadline_ms: 1000,
            deadline_behavior: DeadlineBehavior::ReturnBestAvailable,
            encoding: FrameEncoding::Jpeg,
        },
    };
    ViewCaptureTaskRequest::new(
        request,
        crate::state::ViewCaptureSnapshot::new(view, resolved).unwrap(),
    )
    .unwrap()
}

pub(super) fn completed(request: &ViewCaptureTaskRequest) -> serde_json::Value {
    use crate::contract::{test_support as data, *};
    let frame = CapturedFrame::builder(
        FrameId::parse("frame-1").unwrap(),
        request.snapshot().view(),
        request.snapshot().composition().record(),
        request.request().scene_time,
        &request.request().policy,
    )
    .unwrap()
    .finish(
        data::now(),
        FrameRenderReport {
            detail_complete: true,
            actual_max_screen_error_px: 4.0,
            visible_tile_count: 1,
            pending_tile_count: 0,
            rendered_overlay_count: 1,
            overlay_truncated: false,
            attribution: AttributionSet { lines: vec![] },
        },
        FrameEncoding::Jpeg,
        b"fixture capture bytes".to_vec(),
    )
    .unwrap();
    // This fixture proves metadata and payload identity, not image decoding or GPU rendering.
    serde_json::to_value(frame_tool_result(&frame).unwrap()).unwrap()
}

pub(super) async fn create(
    runtime: &veoveo_task_runtime::TaskRuntime,
    identity: &GatewayInternalIdentity,
    request: &ViewCaptureTaskRequest,
) -> TaskId {
    let id = runtime
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: runtime_owner(identity),
            server: SERVER_SLUG.into(),
            task_type: ViewTaskKind::CaptureFrame.name(),
            request: serde_json::to_value(request).unwrap(),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(120_000),
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap()
        .snapshot
        .task_id;
    runtime.claim(id, Duration::from_secs(90)).await.unwrap();
    id
}

pub(super) async fn finish(
    runtime: &veoveo_task_runtime::TaskRuntime,
    id: TaskId,
    result: serde_json::Value,
) {
    runtime
        .transition(
            id,
            TaskTransition::Succeeded {
                message: "capture completed".into(),
                result,
            },
        )
        .await
        .unwrap();
}
