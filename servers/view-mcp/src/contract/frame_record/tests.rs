use super::*;
use crate::contract::test_support::{composition, now, view};
use serde_json::json;

fn policy() -> CapturePolicy {
    CapturePolicy {
        width_px: 640,
        height_px: 480,
        max_screen_error_px: 8.0,
        deadline_ms: 1000,
        deadline_behavior: DeadlineBehavior::ReturnBestAvailable,
        encoding: FrameEncoding::Jpeg,
    }
}
fn report() -> FrameRenderReport {
    FrameRenderReport {
        detail_complete: true,
        actual_max_screen_error_px: 4.0,
        visible_tile_count: 1,
        pending_tile_count: 0,
        rendered_overlay_count: 1,
        overlay_truncated: false,
        attribution: AttributionSet {
            lines: vec!["Fixture".into()],
        },
    }
}
fn frame() -> CapturedFrame {
    // Metadata/byte-identity fixture, not image or hardware acceptance.
    CapturedFrame::builder(
        FrameId::new("frame-1").unwrap(),
        &view(),
        &composition(),
        now(),
        &policy(),
    )
    .unwrap()
    .finish(
        now(),
        report(),
        FrameEncoding::Jpeg,
        b"capture bytes".to_vec(),
    )
    .unwrap()
}

#[test]
fn capture_derives_identity_parent_and_byte_metadata() {
    let frame = frame();
    let record = frame.record();
    assert_eq!(record.frame_uri().id(), record.frame_id());
    assert_eq!(record.view_id(), view().view_id());
    assert_eq!(record.composition_uri(), composition().composition_uri());
    assert_eq!(
        record.composition_digest_sha256(),
        composition().composition_digest_sha256()
    );
    assert_eq!(record.governed_inputs(), composition().governed_inputs());
    assert_eq!(record.resolved_camera(), view().resolved_camera());
    assert_eq!(record.byte_length(), frame.bytes().len() as u64);
    assert_eq!(
        *record.output_digest_sha256(),
        Sha256Digest::from_bytes(frame.bytes())
    );
    assert_eq!(record.mime_type(), "image/jpeg");
    let wire = serde_json::to_value(record).unwrap();
    assert_eq!(wire["mime_type"], "image/jpeg");
    assert!(wire.get("encoding").is_none());
    let decoded: FrameRecord = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
}

#[test]
fn capture_requires_matching_parent_time_encoding_and_nonempty_bytes() {
    let mut request = crate::contract::test_support::request();
    request.base_layer = LayerId::new("other-layer").unwrap();
    let other =
        SceneComposition::new(request, crate::contract::test_support::authority(), now()).unwrap();
    assert!(
        CapturedFrame::builder(
            FrameId::new("f").unwrap(),
            &view(),
            &other,
            now(),
            &policy()
        )
        .is_err()
    );
    let before = now() - chrono::Duration::seconds(1);
    for (captured_at, encoding, bytes) in [
        (before, FrameEncoding::Jpeg, vec![1]),
        (now(), FrameEncoding::Png, vec![1]),
        (now(), FrameEncoding::Jpeg, vec![]),
    ] {
        assert!(
            CapturedFrame::builder(
                FrameId::new("f").unwrap(),
                &view(),
                &composition(),
                now(),
                &policy()
            )
            .unwrap()
            .finish(captured_at, report(), encoding, bytes)
            .is_err()
        );
    }
    let mut invalid = policy();
    invalid.width_px = 0;
    assert!(
        CapturedFrame::builder(
            FrameId::new("f").unwrap(),
            &view(),
            &composition(),
            now(),
            &invalid
        )
        .is_err()
    );
}

#[test]
fn frame_decoding_rejects_inconsistent_identity_shape_and_detail() {
    let valid = serde_json::to_value(frame().record()).unwrap();
    for (field, value) in [
        ("frame_uri", json!("view://frame/other")),
        (
            "composition_uri",
            json!(CompositionUri::new(SceneCompositionId::from_stable_key(
                b"other"
            ))),
        ),
        ("view_revision", json!(0)),
        ("composition_revision", json!(2)),
        ("width_px", json!(0)),
        ("height_px", json!(0)),
        ("byte_length", json!(0)),
        ("mime_type", json!("image/webp")),
        ("actual_max_screen_error_px", json!(-1.0)),
        ("pending_tile_count", json!(1)),
    ] {
        let mut wire = valid.clone();
        wire[field] = value;
        assert!(
            serde_json::from_value::<FrameRecord>(wire).is_err(),
            "accepted {field}"
        );
    }
    let mut wire = valid.clone();
    wire["resolved_camera"]["position"]["latitude_degrees"] = json!(91);
    assert!(serde_json::from_value::<FrameRecord>(wire).is_err());
    let mut wire = valid.clone();
    wire["governed_inputs"][0]["license"] = json!("");
    assert!(serde_json::from_value::<FrameRecord>(wire).is_err());
    let mut wire = valid;
    let input = wire["governed_inputs"][0].clone();
    wire["governed_inputs"].as_array_mut().unwrap().push(input);
    assert!(serde_json::from_value::<FrameRecord>(wire).is_err());
}

#[test]
fn frame_byte_admission_checks_length_and_digest() {
    let (record, bytes) = frame().into_parts();
    assert!(CapturedFrame::from_record(record.clone(), bytes.clone()).is_ok());
    assert!(CapturedFrame::from_record(record.clone(), vec![1]).is_err());
    assert!(CapturedFrame::from_record(record, vec![0; bytes.len()]).is_err());
}

#[test]
fn capture_encoding_and_wire_schema_share_the_closed_media_profile() {
    for encoding in [FrameEncoding::Png, FrameEncoding::Jpeg] {
        let policy = CapturePolicy {
            encoding,
            ..policy()
        };
        let output = CapturedFrame::builder(
            FrameId::new("frame-1").unwrap(),
            &view(),
            &composition(),
            now(),
            &policy,
        )
        .unwrap()
        .finish(now(), report(), encoding, vec![1])
        .unwrap();
        let wire = serde_json::to_value(output.record()).unwrap();
        assert_eq!(wire["mime_type"], encoding.mime_type());
        assert_eq!(
            serde_json::from_value::<FrameRecord>(wire)
                .unwrap()
                .encoding(),
            encoding
        );
    }
    let schema = serde_json::to_value(schemars::schema_for!(FrameRecord)).unwrap();
    assert_eq!(
        schema["properties"]["mime_type"]["enum"],
        json!(["image/png", "image/jpeg"])
    );
}

#[test]
fn frame_admission_rejects_nonfinite_and_inconsistent_render_reports() {
    for actual in [f32::NAN, f32::INFINITY, -1.0] {
        let mut detail = report();
        detail.actual_max_screen_error_px = actual;
        assert!(
            CapturedFrame::builder(
                FrameId::new("f").unwrap(),
                &view(),
                &composition(),
                now(),
                &policy()
            )
            .unwrap()
            .finish(now(), detail, FrameEncoding::Jpeg, vec![1])
            .is_err()
        );
    }
    let mut detail = report();
    detail.pending_tile_count = 1;
    assert!(
        CapturedFrame::builder(
            FrameId::new("f").unwrap(),
            &view(),
            &composition(),
            now(),
            &policy()
        )
        .unwrap()
        .finish(now(), detail.clone(), FrameEncoding::Jpeg, vec![1])
        .is_err()
    );
    detail.detail_complete = false;
    assert!(
        CapturedFrame::builder(
            FrameId::new("f").unwrap(),
            &view(),
            &composition(),
            now(),
            &policy()
        )
        .unwrap()
        .finish(now(), detail, FrameEncoding::Jpeg, vec![1])
        .is_ok()
    );
}
