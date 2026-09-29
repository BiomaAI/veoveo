use serde_json::{Value, json};
use veoveo_reason_mcp::contract::*;

const ID: &str = "01983da0-0000-7000-8000-000000000001";
const OTHER: &str = "01983da0-0000-7000-8000-000000000002";

pub(super) fn output(id: &str, pipeline: &str) -> AnalyzeRecordingOutput {
    let artifact = json!({
        "artifact_id": ID,
        "artifact_uri": format!("reason://artifact/{ID}"),
        "byte_len": 12,
        "created_at": "2026-09-28T00:00:00Z"
    });
    AnalyzeRecordingOutput::new(
        AnalysisId::parse(id).unwrap(),
        PipelineId::parse(pipeline).unwrap(),
        ModelId::parse("world-model").unwrap(),
        ReasoningSummary {
            observed_frames: 10,
            event_count: 1,
            elapsed_ms: 10,
            decode_start_index: 0,
            requested_start_index: 10,
            requested_end_index: 20,
        },
        serde_json::from_value(artifact.clone()).unwrap(),
        serde_json::from_value(artifact).unwrap(),
    )
}

fn analysis() -> AnalysisView {
    AnalysisView::new(
        AnalysisId::parse(ID).unwrap(),
        PipelineId::parse("traffic-events").unwrap(),
        AnalysisDetails {
            status: "succeeded".into(),
            progress: 1.0,
            task_kind: "detect_events".into(),
            recording_uri: veoveo_recording_mcp::contract::RecordingUri::new(
                veoveo_recording_mcp::contract::RecordingId::parse(OTHER).unwrap(),
            ),
            entity_path: "/camera/front".into(),
            timeline: "sensor_time".into(),
            created_at: "2026-09-28T00:00:00Z".into(),
            updated_at: "2026-09-28T00:01:00Z".into(),
        },
    )
}

#[test]
fn catalog_views_derive_identity_and_reject_disagreeing_wire_fields() {
    let model = ModelView::new(
        ModelId::parse("world-model").unwrap(),
        "World model".into(),
        "Video understanding".into(),
        ModelFormat::LocalCheckpoint,
    );
    let pipeline = PipelineView::new(
        PipelineId::parse("traffic-events").unwrap(),
        model.id().clone(),
        PipelineDetails {
            title: "Traffic events".into(),
            description: "Find events".into(),
            operation: PipelineOperation::VideoReasoning,
            prompt_revision: "v1".into(),
            observation_width: 640,
            observation_height: 480,
        },
    );
    let model_wire = serde_json::to_value(model).unwrap();
    let pipeline_wire = serde_json::to_value(pipeline).unwrap();
    assert_eq!(model_wire["id"], "world-model");
    assert_eq!(model_wire["uri"], "reason://model/world-model");
    assert_eq!(pipeline_wire["id"], "traffic-events");
    assert_eq!(pipeline_wire["uri"], "reason://pipeline/traffic-events");
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<ModelView>(model_wire.clone()).unwrap())
            .unwrap(),
        model_wire
    );
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<PipelineView>(pipeline_wire.clone()).unwrap()
        )
        .unwrap(),
        pipeline_wire
    );
    for field in ["id", "uri"] {
        let mut model = model_wire.clone();
        model[field] = if field == "id" {
            "different"
        } else {
            "reason://model/different"
        }
        .into();
        assert!(serde_json::from_value::<ModelView>(model).is_err());
        let mut pipeline = pipeline_wire.clone();
        pipeline[field] = if field == "id" {
            "different"
        } else {
            "reason://pipeline/different"
        }
        .into();
        assert!(serde_json::from_value::<PipelineView>(pipeline).is_err());
    }
}

#[test]
fn terminal_output_rejects_individually_valid_but_inconsistent_addresses() {
    let wire = serde_json::to_value(output(ID, "traffic-events")).unwrap();
    assert_eq!(wire["analysis_uri"], format!("reason://analysis/{ID}"));
    assert_eq!(
        wire["result_uri"],
        format!("reason://analysis/{ID}/results")
    );
    assert!(wire.get("source_clip_artifact").is_none());
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<AnalyzeRecordingOutput>(wire.clone()).unwrap()
        )
        .unwrap(),
        wire
    );
    for (field, address) in [
        ("analysis_uri", format!("reason://analysis/{OTHER}")),
        ("result_uri", format!("reason://analysis/{OTHER}/results")),
    ] {
        let mut value = wire.clone();
        value[field] = address.into();
        let error = serde_json::from_value::<AnalyzeRecordingOutput>(value).unwrap_err();
        assert!(error.to_string().contains("analysis and results URIs"));
        assert!(!error.to_string().contains(OTHER));
    }
}

#[test]
fn analysis_construction_binds_output_to_task_and_requested_pipeline() {
    assert!(
        analysis()
            .with_output(Some(output(OTHER, "traffic-events")))
            .is_err()
    );
    assert!(
        analysis()
            .with_output(Some(output(ID, "other-pipeline")))
            .is_err()
    );
    let view = analysis()
        .with_output(Some(output(ID, "traffic-events")))
        .unwrap();
    assert_eq!(view.task_id(), view.output().unwrap().analysis_id());
    assert_eq!(view.pipeline_id(), view.output().unwrap().pipeline_uri.id());
    assert_eq!(view.analysis_uri().id(), &view.task_id());
    assert_eq!(view.results_uri(), view.output().unwrap().result_uri());
}

#[test]
fn analysis_wire_rejects_cross_task_and_cross_pipeline_nested_products() {
    let wire = serde_json::to_value(
        analysis()
            .with_output(Some(output(ID, "traffic-events")))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<AnalysisView>(wire.clone()).unwrap())
            .unwrap(),
        wire
    );
    let replacements: [(&str, Value); 3] = [
        ("task_id", OTHER.into()),
        ("analysis_uri", format!("reason://analysis/{OTHER}").into()),
        (
            "results_uri",
            format!("reason://analysis/{OTHER}/results").into(),
        ),
    ];
    for (field, value) in replacements {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<AnalysisView>(invalid).is_err(),
            "{field}"
        );
    }
    for unrelated in [
        output(OTHER, "traffic-events"),
        output(ID, "other-pipeline"),
    ] {
        let mut invalid = wire.clone();
        invalid["output"] = serde_json::to_value(unrelated).unwrap();
        assert!(serde_json::from_value::<AnalysisView>(invalid).is_err());
    }
    let absent = serde_json::to_value(analysis()).unwrap();
    assert!(absent.get("output").is_none());
    assert!(
        serde_json::from_value::<AnalysisView>(absent)
            .unwrap()
            .output()
            .is_none()
    );
}

#[test]
fn recording_references_use_the_recording_owner_admission() {
    let valid = serde_json::to_value(analysis()).unwrap();
    serde_json::from_value::<AnalysisView>(valid.clone()).unwrap();
    for address in [
        "recording://recording/01983da0-0000-7000-8000-000000000002",
        "recording://recordings/01983da0-0000-7000-8000-000000000002/layers",
        "recording://recordings/01983da0-0000-4000-8000-000000000002",
    ] {
        let mut invalid = valid.clone();
        invalid["recording_uri"] = address.into();
        let error = serde_json::from_value::<AnalysisView>(invalid).unwrap_err();
        assert!(!error.to_string().contains(address));
    }
}
