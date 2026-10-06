use serde_json::json;

use super::*;

const ID: &str = "01983da0-0000-7000-8000-000000000001";
const CAPABILITY: &str = "01983da0-0000-7000-8000-000000000002";

fn artifact() -> ArtifactObject {
    ArtifactObject {
        metadata: serde_json::from_value(json!({
            "artifact_id": ID, "artifact_uri": format!("artifact://{ID}"),
            "byte_len": 1, "created_at": "2026-09-28T00:00:00Z",
            "compliance": {"classification":"restricted", "data_labels":["grounding-label"]}
        }))
        .unwrap(),
        bytes: include_bytes!("../../../../stream-mcp/testdata/replay-results-v1.json").to_vec(),
    }
}

fn selection() -> RecordingVideoSelection {
    serde_json::from_value(json!({
        "recording_uri":"recording://recordings/01983da0-0000-7000-8000-000000000000",
        "entity_path":"/camera/front", "timeline":"sensor_time", "range":{"start":10,"end":20}
    }))
    .unwrap()
}

fn admitted() -> GroundingInput {
    GroundingInput::from_artifact(
        &StreamArtifactUri::new(ID.parse().unwrap()),
        &selection(),
        artifact(),
    )
    .unwrap()
}

#[test]
fn actual_artifact_labels_and_classification_become_output_requirements() {
    let input = admitted();
    let labels: BTreeSet<DataLabelId> = ["restricted", "grounding-label"]
        .into_iter()
        .map(|v| DataLabelId::parse(v).unwrap())
        .collect();
    assert_eq!(input.required_labels(), &labels);
    let grounding = input.into_detections();
    assert_eq!(grounding.source_artifact_uri.artifact_id().to_string(), ID);
    assert_eq!(
        grounding
            .frames
            .iter()
            .map(|frame| frame.index)
            .collect::<Vec<_>>(),
        vec![10, 20]
    );
}

#[test]
fn admission_checks_the_actual_artifact_occurrence_and_video_selection() {
    assert!(
        GroundingInput::from_artifact(
            &StreamArtifactUri::new(CAPABILITY.parse().unwrap()),
            &selection(),
            artifact()
        )
        .is_err()
    );
    let mut selected = selection().into_builder();
    selected.timeline = "other-time".into();
    assert!(
        GroundingInput::from_artifact(
            &StreamArtifactUri::new(ID.parse().unwrap()),
            &selected.build().unwrap(),
            artifact()
        )
        .is_err()
    );
}
