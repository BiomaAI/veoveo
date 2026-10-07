use super::*;
use crate::contract::test_support as fixture;
use serde_json::{Value, json};

// These bytes qualify owner construction/admission only; no image decoding or GPU work.
fn produced() -> Value {
    let view = fixture::view();
    let policy = CapturePolicy {
        width_px: 640,
        height_px: 480,
        max_screen_error_px: 8.0,
        deadline_ms: 1000,
        deadline_behavior: DeadlineBehavior::ReturnBestAvailable,
        encoding: FrameEncoding::Jpeg,
    };
    let frame = CapturedFrame::builder(
        FrameId::parse("frame-1").unwrap(),
        &view,
        &fixture::composition(),
        fixture::now(),
        &policy,
    )
    .unwrap()
    .finish(
        fixture::now(),
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
        },
        FrameEncoding::Jpeg,
        b"capture bytes".to_vec(),
    )
    .unwrap();
    let scene = PreviewSceneRecord::new(
        &view,
        PreviewScenePolicy {
            width_px: 640,
            height_px: 480,
            max_screen_error_px: 8.0,
        },
        true,
        false,
        AttributionSet {
            lines: vec!["Fixture".into()],
        },
        vec![
            SceneTileRecord::new(
                TileUri::new(TileKey::from_bytes(b"tile")),
                glam::DMat4::IDENTITY.to_cols_array(),
                Some(64),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    json!({"layers":[LayerSummary {layer_id:LayerId::parse("fixture").unwrap(),
        label:"Fixture".into(),source_kind:LayerSourceKind::GooglePhotorealistic}],
        "composition":fixture::composition(),"view":view,"frame":frame.record(),
        "scene":scene,"closed":CloseViewResult {view_id:fixture::view().view_id().clone(),closed:true}})
}

fn decode(root: &str, value: Value) -> bool {
    match root {
        "layers" => serde_json::from_value::<Vec<LayerSummary>>(value).is_ok(),
        "composition" => serde_json::from_value::<SceneComposition>(value).is_ok(),
        "view" => serde_json::from_value::<ViewRecord>(value).is_ok(),
        "frame" => serde_json::from_value::<FrameRecord>(value).is_ok(),
        "scene" => serde_json::from_value::<PreviewSceneRecord>(value).is_ok(),
        "closed" => serde_json::from_value::<CloseViewResult>(value).is_ok(),
        _ => panic!("unknown App owner root"),
    }
}

#[test]
fn app_roots_from_checked_producers_admit_one_owned_spelling() {
    let actual = produced();
    let schema = schema_bundle();
    let validator = jsonschema::validator_for(schema.as_value()).unwrap();
    assert!(validator.is_valid(&actual));
    for (root, value) in actual.as_object().unwrap() {
        assert!(decode(root, value.clone()), "{root}");
        let cases = fixture::retired_field_cases(value);
        assert!(!cases.is_empty(), "{root} lacks renamed-field coverage");
        for (case, invalid) in cases {
            assert!(!decode(root, invalid.clone()), "admitted {root}{case}");
            let mut all = actual.clone();
            all[root] = invalid;
            assert!(!validator.is_valid(&all), "schema admitted {root}{case}");
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&actual).unwrap();
    bytes.push(b'\n');
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("app/testdata/contracts.json");
    if std::env::var_os("UPDATE_VIEW_APP_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &bytes).unwrap();
    }
    let captured: Value = serde_json::from_slice(
        &std::fs::read(path).expect("capture current checked View App fixtures"),
    )
    .expect("captured View App fixture is JSON");
    assert_eq!(captured, actual);
}
