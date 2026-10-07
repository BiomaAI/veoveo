use super::*;
use crate::contract::test_support::view;
use serde_json::json;

fn tile(length: Option<u64>) -> SceneTileRecord {
    SceneTileRecord::new(
        TileUri::new(TileKey::from_bytes(b"tile")),
        DMat4::IDENTITY.to_cols_array(),
        length,
    )
    .unwrap()
}
fn policy() -> PreviewScenePolicy {
    PreviewScenePolicy {
        width_px: 640,
        height_px: 480,
        max_screen_error_px: 8.0,
    }
}
fn scene(
    tiles: Vec<SceneTileRecord>,
    complete: bool,
    truncated: bool,
) -> Result<PreviewSceneRecord, PreviewSceneError> {
    PreviewSceneRecord::new(
        &view(),
        policy(),
        complete,
        truncated,
        AttributionSet { lines: vec![] },
        tiles,
    )
}

#[test]
fn preview_derives_view_camera_origin_and_policy() {
    let scene = scene(vec![tile(Some(64))], true, false).unwrap();
    assert_eq!(scene.view_id(), view().view_id());
    assert_eq!(scene.composition_id(), view().composition_id());
    assert_eq!(scene.local_origin(), view().resolved_camera().position);
    assert_eq!(
        scene.local_from_ecef(),
        &crate::geodesy::world_from_ecef(scene.local_origin()).to_cols_array()
    );
    assert!(scene.detail_complete());
    let wire = serde_json::to_value(&scene).unwrap();
    let decoded: PreviewSceneRecord = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
}

#[test]
fn scene_tile_admission_checks_affine_transform_and_size_agreement() {
    for length in [
        None,
        Some(1),
        Some(MAX_TILE_RESOURCE_BYTES),
        Some(MAX_TILE_RESOURCE_BYTES + 1),
    ] {
        let tile = tile(length);
        assert_eq!(
            tile.oversize(),
            length.is_some_and(|v| v > MAX_TILE_RESOURCE_BYTES)
        );
        let wire = serde_json::to_value(&tile).unwrap();
        assert!(serde_json::from_value::<SceneTileRecord>(wire.clone()).is_ok());
        let mut corrupt = wire;
        corrupt["oversize"] = json!(!tile.oversize());
        assert!(serde_json::from_value::<SceneTileRecord>(corrupt).is_err());
    }
    let uri = TileUri::new(TileKey::from_bytes(b"tile"));
    assert!(SceneTileRecord::new(uri.clone(), DMat4::IDENTITY.to_cols_array(), Some(0)).is_err());
    for (index, value) in [
        (0, f64::NAN),
        (12, f64::INFINITY),
        (3, 0.1),
        (15, 2.0),
        (0, 0.0),
    ] {
        let mut matrix = DMat4::IDENTITY.to_cols_array();
        matrix[index] = value;
        assert!(SceneTileRecord::new(uri.clone(), matrix, Some(1)).is_err());
    }
    let scaled = DMat4::from_scale(glam::DVec3::new(2.0, 3.0, -4.0));
    assert!(SceneTileRecord::new(uri, scaled.to_cols_array(), Some(1)).is_ok());
}

#[test]
fn truncated_or_oversize_preview_reports_partial_detail() {
    assert!(
        !scene(vec![tile(Some(MAX_TILE_RESOURCE_BYTES + 1))], true, false)
            .unwrap()
            .detail_complete()
    );
    let truncated = scene(vec![tile(None); SCENE_MAX_TILES], true, true).unwrap();
    assert!(truncated.truncated());
    assert!(!truncated.detail_complete());
    assert!(scene(vec![tile(None); SCENE_MAX_TILES + 1], false, true).is_err());
    assert!(scene(vec![tile(None)], false, true).is_err());
    let mut wire = serde_json::to_value(truncated).unwrap();
    wire["detailComplete"] = json!(true);
    assert!(serde_json::from_value::<PreviewSceneRecord>(wire).is_err());
}

#[test]
fn preview_decoding_rejects_camera_origin_transform_and_policy_disagreement() {
    let valid = serde_json::to_value(scene(vec![tile(Some(64))], true, false).unwrap()).unwrap();
    for (field, value) in [
        ("viewRevision", json!(0)),
        ("widthPx", json!(0)),
        ("heightPx", json!(0)),
        ("maxScreenErrorPx", json!(0.1)),
        ("maxScreenErrorPx", json!(300)),
        ("truncated", json!(true)),
    ] {
        let mut wire = valid.clone();
        wire[field] = value;
        assert!(
            serde_json::from_value::<PreviewSceneRecord>(wire).is_err(),
            "accepted {field}"
        );
    }
    let mut wire = valid.clone();
    wire["localOrigin"]["latitudeDegrees"] = json!(0);
    assert!(serde_json::from_value::<PreviewSceneRecord>(wire).is_err());
    let mut wire = valid.clone();
    wire["localFromEcef"][12] = json!(wire["localFromEcef"][12].as_f64().unwrap() + 1.0);
    assert!(serde_json::from_value::<PreviewSceneRecord>(wire).is_err());
    let mut wire = valid;
    wire["localFromEcef"][0] = json!(2);
    assert!(serde_json::from_value::<PreviewSceneRecord>(wire).is_err());
}

#[test]
fn preview_transform_admission_allows_only_small_numeric_differences() {
    let valid = serde_json::to_value(scene(vec![tile(Some(64))], true, false).unwrap()).unwrap();
    let mut rounded = valid.clone();
    rounded["localFromEcef"][0] = json!(rounded["localFromEcef"][0].as_f64().unwrap() + 1e-13);
    rounded["localFromEcef"][13] = json!(rounded["localFromEcef"][13].as_f64().unwrap() + 1e-5);
    assert!(serde_json::from_value::<PreviewSceneRecord>(rounded).is_ok());
    for (index, error) in [(0, 1e-10), (13, 1e-3)] {
        let mut wire = valid.clone();
        wire["localFromEcef"][index] =
            json!(wire["localFromEcef"][index].as_f64().unwrap() + error);
        assert!(serde_json::from_value::<PreviewSceneRecord>(wire).is_err());
    }
}
