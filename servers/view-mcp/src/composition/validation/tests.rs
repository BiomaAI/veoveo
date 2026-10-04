use super::*;
use crate::composition::test_support as fixture;

#[test]
fn retained_inline_and_artifact_geometry_validate_after_byte_round_trip() {
    for resolved in [fixture::resolved(), fixture::with_artifact()] {
        resolved.validate().unwrap();
        let recovered: ResolvedSceneComposition =
            serde_json::from_slice(&serde_json::to_vec(&resolved).unwrap()).unwrap();
        recovered.validate().unwrap();
    }
}

#[test]
fn retained_geometry_cannot_replace_its_input_or_overlay_metadata() {
    let mut resolved = fixture::resolved();
    resolved.resolved_overlays[0]
        .overlay
        .style
        .marker_size_meters *= 2.0;
    assert_rejected(&resolved);
    let mut resolved = fixture::resolved();
    resolved.resolved_overlays.clear();
    assert_rejected(&resolved);
    for mut resolved in [fixture::resolved(), fixture::with_artifact()] {
        resolved.resolved_overlays[0].geometry = SceneOverlayGeometry::Marker {
            position: ScenePosition::LocalMeters {
                xyz_meters: [1.0, 2.0, 3.0],
            },
        };
        assert_rejected(&resolved);
    }
}

#[test]
fn retained_bytes_require_declared_inputs_correct_digests_and_no_extras() {
    let mut resolved = fixture::with_artifact();
    resolved.artifact_bytes.values_mut().next().unwrap().0[0] = b'x';
    assert_rejected(&resolved);
    let mut resolved = fixture::with_artifact();
    resolved.artifact_bytes.clear();
    assert_rejected(&resolved);
    let mut resolved = fixture::with_artifact();
    resolved.artifact_bytes.insert(
        SceneInputId::parse("extra").unwrap(),
        ResolvedArtifactBytes(vec![0]),
    );
    assert_rejected(&resolved);
}

#[test]
fn artifact_geometry_needs_local_frame_even_when_the_request_contains_no_inline_positions() {
    let mut resolved = fixture::with_artifact();
    let geometry = SceneOverlayGeometry::Marker {
        position: ScenePosition::LocalMeters {
            xyz_meters: [1.0, 2.0, 3.0],
        },
    };
    let bytes = serde_json::to_vec(&geometry).unwrap();
    let mut request = crate::contract::test_support::request();
    request.governed_inputs[0].digest_sha256 = Sha256Digest::from_bytes(&bytes);
    request.governed_inputs[0].media_type = Some(OVERLAY_ARTIFACT_MIME_TYPE.into());
    let input_id = request.governed_inputs[0].input_id.clone();
    request.overlays[0].geometry = SceneOverlayGeometrySource::Artifact {
        input_id: input_id.clone(),
    };
    resolved.resolved_overlays[0] = ResolvedSceneOverlay {
        overlay: request.overlays[0].clone(),
        geometry,
    };
    resolved.record = SceneComposition::new(
        request,
        crate::contract::test_support::authority(),
        crate::contract::test_support::now(),
    )
    .unwrap();
    resolved
        .artifact_bytes
        .insert(input_id, ResolvedArtifactBytes(bytes));
    assert!(
        resolved
            .validate()
            .unwrap_err()
            .to_string()
            .contains("Frames binding")
    );
}

#[test]
fn retained_artifact_limits_apply_before_geometry_work() {
    let mut resolved = fixture::with_artifact();
    resolved.artifact_bytes.values_mut().next().unwrap().0 =
        vec![0; MAX_OVERLAY_ARTIFACT_BYTES as usize + 1];
    assert!(
        resolved
            .validate()
            .unwrap_err()
            .to_string()
            .contains("per-artifact limit")
    );
    resolved.artifact_bytes.clear();
    for index in 0..5 {
        resolved.artifact_bytes.insert(
            SceneInputId::parse(format!("input-{index}")).unwrap(),
            ResolvedArtifactBytes(vec![0; MAX_OVERLAY_ARTIFACT_BYTES as usize]),
        );
    }
    assert!(
        resolved
            .validate()
            .unwrap_err()
            .to_string()
            .contains("composition limit")
    );
    let encoded = serde_json::Value::String(
        "A".repeat((MAX_OVERLAY_ARTIFACT_BYTES.div_ceil(3) * 4 + 4) as usize),
    );
    assert!(
        serde_json::from_value::<ResolvedArtifactBytes>(encoded)
            .unwrap_err()
            .to_string()
            .contains("encoded byte limit")
    );
}

fn assert_rejected(value: &ResolvedSceneComposition) {
    assert!(value.validate().is_err());
    assert!(
        ResolvedSceneComposition::new(
            value.record.clone(),
            value.resolved_overlays.clone(),
            value.artifact_bytes.clone()
        )
        .is_err()
    );
    assert!(
        serde_json::from_slice::<ResolvedSceneComposition>(&serde_json::to_vec(value).unwrap())
            .is_err()
    );
}
