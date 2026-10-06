use serde_json::{Value, json};
use veoveo_reason_mcp::contract::*;

const ID: &str = "01983da0-0000-7000-8000-000000000001";
const OTHER: &str = "01983da0-0000-7000-8000-000000000002";

pub(super) fn output(id: &str, pipeline: &str) -> AnalyzeRecordingOutput {
    let mut wire: Value =
        serde_json::from_str(include_str!("../../testdata/analysis-output-v1.json")).unwrap();
    wire["analysis_uri"] = format!("reason://analysis/{id}").into();
    wire["result_uri"] = format!("reason://analysis/{id}/results").into();
    wire["pipeline_uri"] = format!("reason://pipeline/{pipeline}").into();
    wire["results_artifact"]["metadata"]["provenance"]["analysis_id"] = id.into();
    wire["results_artifact"]["metadata"]["provenance"]["pipeline_id"] = pipeline.into();
    wire["annotations_artifact"]["metadata"]["provenance"]["analysis_id"] = id.into();
    wire["finding"]["pipeline_id"] = pipeline.into();
    serde_json::from_value(wire).unwrap()
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

#[test]
fn terminal_output_requires_current_finding_and_matching_model_identity() {
    let wire = serde_json::to_value(output(ID, "traffic-events")).unwrap();
    for (pointer, replacement) in [
        ("/pipeline_uri", json!("reason://pipeline/other")),
        ("/model_uri", json!("reason://model/other")),
        ("/finding/pipeline_id", json!("other")),
        ("/finding/model_id", json!("other")),
    ] {
        let mut corrupt = wire.clone();
        *corrupt.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            serde_json::from_value::<AnalyzeRecordingOutput>(corrupt).is_err(),
            "{pointer}"
        );
    }
    let mut missing = wire;
    missing.as_object_mut().unwrap().remove("finding");
    assert!(serde_json::from_value::<AnalyzeRecordingOutput>(missing).is_err());
}

#[test]
fn finding_totals_and_artifact_provenance_are_checked_at_terminal_decode() {
    let original = serde_json::to_value(output(ID, "traffic-events")).unwrap();
    for (pointer, value) in [
        ("/summary/event_count", json!(2)),
        ("/summary/observed_frames", json!(0)),
        ("/summary/requested_start_index", json!(9)),
        (
            "/results_artifact/metadata/provenance/prompt_revision",
            json!("other"),
        ),
        (
            "/results_artifact/metadata/provenance/task_kind",
            json!("answer_question"),
        ),
        (
            "/annotations_artifact/metadata/provenance/recording_id",
            json!(OTHER),
        ),
        (
            "/annotations_artifact/metadata/provenance/results_artifact_uri",
            json!(format!("reason://artifact/{OTHER}")),
        ),
        (
            "/annotations_artifact/metadata/provenance/source_snapshot_sha256",
            json!("a".repeat(64)),
        ),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            serde_json::from_value::<AnalyzeRecordingOutput>(bad).is_err(),
            "{pointer}"
        );
    }
    let good: AnalyzeRecordingOutput = serde_json::from_value(original).unwrap();
    assert!(
        AnalyzeRecordingOutput::new(
            OTHER.parse().unwrap(),
            good.finding.clone(),
            good.summary.clone(),
            good.results_artifact.clone(),
            good.annotations_artifact.clone()
        )
        .is_err()
    );
}

#[test]
fn optional_source_clip_binds_visible_finding_and_decode_start() {
    let mut wire = serde_json::to_value(output(ID, "traffic-events")).unwrap();
    let mut clip = wire["annotations_artifact"].clone();
    clip["artifact_id"] = "01983da0-0000-7000-8000-000000000004".into();
    clip["artifact_uri"] = "reason://artifact/01983da0-0000-7000-8000-000000000004".into();
    clip["metadata"]["provenance"] = json!({
        "kind": "reason_source_clip", "analysis_id": ID,
        "recording_id": wire["results_artifact"]["metadata"]["provenance"]["recording_id"],
        "entity_path": wire["finding"]["entity_path"],
        "timeline": wire["finding"]["timeline"],
        "decode_start_index": wire["summary"]["decode_start_index"],
        "source_snapshot_sha256": wire["results_artifact"]["metadata"]["provenance"]["source_snapshot_sha256"]
    });
    wire["source_clip_artifact"] = clip;
    let admitted: AnalyzeRecordingOutput = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&admitted).unwrap(), wire);
    let without = output(ID, "traffic-events");
    assert!(
        without
            .with_source_clip(admitted.source_clip_artifact.clone())
            .is_ok()
    );
    for (field, value) in [
        ("analysis_id", json!(OTHER)),
        ("recording_id", json!(OTHER)),
        ("entity_path", json!("/other")),
        ("timeline", json!("other")),
        ("decode_start_index", json!(1)),
        ("source_snapshot_sha256", json!("a".repeat(64))),
    ] {
        let mut bad = wire.clone();
        bad["source_clip_artifact"]["metadata"]["provenance"][field] = value;
        assert!(
            serde_json::from_value::<AnalyzeRecordingOutput>(bad).is_err(),
            "{field}"
        );
    }
}

#[test]
fn admitted_products_emit_the_actual_object_schemas() {
    let schema = serde_json::to_value(schemars::schema_for!(ReasoningResults)).unwrap();
    assert_eq!(schema["type"], "object", "ReasoningResults");
    assert!(schema.get("$ref").is_none(), "ReasoningResults");
    let schema = serde_json::to_value(schemars::schema_for!(AnalyzeRecordingOutput)).unwrap();
    assert_eq!(schema["type"], "object", "AnalyzeRecordingOutput");
    assert!(schema.get("$ref").is_none(), "AnalyzeRecordingOutput");
}

#[test]
fn terminal_roles_cannot_reuse_one_immutable_artifact_occurrence() {
    let mut wire = serde_json::to_value(output(ID, "traffic-events")).unwrap();
    let mut clip = wire["annotations_artifact"].clone();
    clip["artifact_id"] = "01983da0-0000-7000-8000-000000000004".into();
    clip["artifact_uri"] = "reason://artifact/01983da0-0000-7000-8000-000000000004".into();
    clip["metadata"]["provenance"] = serde_json::json!({
        "kind": "reason_source_clip", "analysis_id": ID,
        "recording_id": "01983da0-0000-7000-8000-000000000000", "entity_path":"/camera/front", "timeline":"sensor_time",
        "decode_start_index": wire["summary"]["decode_start_index"],
        "source_snapshot_sha256": wire["results_artifact"]["metadata"]["provenance"]["source_snapshot_sha256"]
    });
    wire["source_clip_artifact"] = clip;
    assert!(serde_json::from_value::<AnalyzeRecordingOutput>(wire.clone()).is_ok());
    for (target, source) in [
        ("annotations_artifact", "results_artifact"),
        ("source_clip_artifact", "results_artifact"),
        ("source_clip_artifact", "annotations_artifact"),
    ] {
        let mut bad = wire.clone();
        bad[target]["artifact_id"] = wire[source]["artifact_id"].clone();
        bad[target]["artifact_uri"] = wire[source]["artifact_uri"].clone();
        assert!(
            serde_json::from_value::<AnalyzeRecordingOutput>(bad.clone()).is_err(),
            "{target}/{source}"
        );
        assert!(
            serde_json::from_value::<AnalyzeRecordingOutputBuilder>(bad)
                .unwrap()
                .build()
                .is_err()
        );
    }
}
