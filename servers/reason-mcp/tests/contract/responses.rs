#[path = "recorded_source_fixture.rs"]
mod recorded_source_fixture;
use serde_json::{Value, json};
use veoveo_reason_mcp::contract::*;

const ID: &str = "01983da0-0000-7000-8000-000000000001";
const OTHER: &str = "01983da0-0000-7000-8000-000000000002";

pub(super) fn output(id: &str, pipeline: &str) -> AnalyzeRecordingOutput {
    let mut wire: Value = serde_json::to_value(recorded_source_fixture::output()).unwrap();
    wire["analysisUri"] = format!("reason://analysis/{id}").into();
    wire["resultUri"] = format!("reason://analysis/{id}/results").into();
    wire["pipelineUri"] = format!("reason://pipeline/{pipeline}").into();
    wire["resultsArtifact"]["metadata"]["provenance"]["analysisId"] = id.into();
    wire["resultsArtifact"]["metadata"]["provenance"]["pipelineId"] = pipeline.into();
    wire["annotationsArtifact"]["metadata"]["provenance"]["analysisId"] = id.into();
    wire["finding"]["pipelineId"] = pipeline.into();
    serde_json::from_value(wire).unwrap()
}

fn analysis() -> AnalysisView {
    AnalysisView::new(
        AnalysisId::parse(ID).unwrap(),
        PipelineId::parse("traffic-events").unwrap(),
        AnalysisDetails {
            status: veoveo_task_contract::TaskStatus::Succeeded,
            progress: 1.0,
            task_kind: ReasoningKind::DetectEvents,
            recording_uri: veoveo_recording_mcp::contract::RecordingUri::new(
                veoveo_recording_mcp::contract::RecordingId::parse(
                    "01983da0-0000-7000-8000-000000000000",
                )
                .unwrap(),
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
    assert_eq!(wire["analysisUri"], format!("reason://analysis/{ID}"));
    assert_eq!(wire["resultUri"], format!("reason://analysis/{ID}/results"));
    assert!(wire.get("sourceClipArtifact").is_none());
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<AnalyzeRecordingOutput>(wire.clone()).unwrap()
        )
        .unwrap(),
        wire
    );
    for (field, address) in [
        ("analysisUri", format!("reason://analysis/{OTHER}")),
        ("resultUri", format!("reason://analysis/{OTHER}/results")),
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
        ("taskId", OTHER.into()),
        ("analysisUri", format!("reason://analysis/{OTHER}").into()),
        (
            "resultsUri",
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
        invalid["recordingUri"] = address.into();
        let error = serde_json::from_value::<AnalysisView>(invalid).unwrap_err();
        assert!(!error.to_string().contains(address));
    }
}

#[test]
fn terminal_output_requires_current_finding_and_matching_model_identity() {
    let wire = serde_json::to_value(output(ID, "traffic-events")).unwrap();
    for (pointer, replacement) in [
        ("/pipelineUri", json!("reason://pipeline/other")),
        ("/modelUri", json!("reason://model/other")),
        ("/finding/pipelineId", json!("other")),
        ("/finding/modelId", json!("other")),
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
        ("/summary/eventCount", json!(2)),
        ("/summary/observedFrames", json!(0)),
        ("/summary/requestedStartIndex", json!(9)),
        (
            "/resultsArtifact/metadata/provenance/promptRevision",
            json!("other"),
        ),
        (
            "/resultsArtifact/metadata/provenance/taskKind",
            json!("answer_question"),
        ),
        (
            "/annotationsArtifact/metadata/provenance/recordingId",
            json!(OTHER),
        ),
        (
            "/annotationsArtifact/metadata/provenance/resultsArtifactUri",
            json!(format!("reason://artifact/{OTHER}")),
        ),
        (
            "/annotationsArtifact/metadata/provenance/sourceSnapshotSha256",
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
    let mut clip = wire["annotationsArtifact"].clone();
    clip["artifactId"] = "01983da0-0000-7000-8000-000000000004".into();
    clip["artifactUri"] = "reason://artifact/01983da0-0000-7000-8000-000000000004".into();
    clip["metadata"]["provenance"] = json!({
        "kind": "reason_source_clip", "analysisId": ID,
        "recordingId": wire["resultsArtifact"]["metadata"]["provenance"]["recordingId"],
        "entityPath": wire["finding"]["entityPath"],
        "timeline": wire["finding"]["timeline"],
        "decodeStartIndex": wire["summary"]["decodeStartIndex"],
        "sourceSnapshotSha256": wire["resultsArtifact"]["metadata"]["provenance"]["sourceSnapshotSha256"]
    });
    wire["sourceClipArtifact"] = clip;
    let admitted: AnalyzeRecordingOutput = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&admitted).unwrap(), wire);
    let without = output(ID, "traffic-events");
    assert!(
        without
            .with_source_clip(admitted.source_clip_artifact.clone())
            .is_ok()
    );
    for (field, value) in [
        ("analysisId", json!(OTHER)),
        ("recordingId", json!(OTHER)),
        ("entityPath", json!("/other")),
        ("timeline", json!("other")),
        ("decodeStartIndex", json!(1)),
        ("sourceSnapshotSha256", json!("a".repeat(64))),
    ] {
        let mut bad = wire.clone();
        bad["sourceClipArtifact"]["metadata"]["provenance"][field] = value;
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
    let mut clip = wire["annotationsArtifact"].clone();
    clip["artifactId"] = "01983da0-0000-7000-8000-000000000004".into();
    clip["artifactUri"] = "reason://artifact/01983da0-0000-7000-8000-000000000004".into();
    clip["metadata"]["provenance"] = serde_json::json!({
        "kind": "reason_source_clip", "analysisId": ID,
        "recordingId": "01983da0-0000-7000-8000-000000000000", "entityPath":"/camera/front", "timeline":"sensor_time",
        "decodeStartIndex": wire["summary"]["decodeStartIndex"],
        "sourceSnapshotSha256": wire["resultsArtifact"]["metadata"]["provenance"]["sourceSnapshotSha256"]
    });
    wire["sourceClipArtifact"] = clip;
    assert!(serde_json::from_value::<AnalyzeRecordingOutput>(wire.clone()).is_ok());
    for (target, source) in [
        ("annotationsArtifact", "resultsArtifact"),
        ("sourceClipArtifact", "resultsArtifact"),
        ("sourceClipArtifact", "annotationsArtifact"),
    ] {
        let mut bad = wire.clone();
        bad[target]["artifactId"] = wire[source]["artifactId"].clone();
        bad[target]["artifactUri"] = wire[source]["artifactUri"].clone();
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

#[test]
fn current_task_status_vocabulary_rejects_unknown_response_values() {
    let current = serde_json::to_value(analysis()).unwrap();
    assert_eq!(current["status"], "succeeded");
    for status in ["unknown", "cancelRequested", "cancel-requested"] {
        let mut changed = current.clone();
        changed["status"] = status.into();
        assert!(serde_json::from_value::<AnalysisView>(changed).is_err());
    }
}

#[test]
fn current_terminal_and_provenance_admit_one_owned_spelling() {
    let current = serde_json::to_value(recorded_source_fixture::output()).unwrap();
    assert!(serde_json::from_value::<AnalyzeRecordingOutput>(current.clone()).is_ok());
    for (path, key, retired) in [
        ("", "analysisUri", "analysis_uri"),
        ("", "resultUri", "result_uri"),
        ("", "pipelineUri", "pipeline_uri"),
        ("", "modelUri", "model_uri"),
        ("", "resultsArtifact", "results_artifact"),
        ("", "annotationsArtifact", "annotations_artifact"),
        ("/summary", "observedFrames", "observed_frames"),
        ("/summary", "elapsedMs", "elapsed_ms"),
        ("/summary", "eventCount", "event_count"),
        ("/summary", "decodeStartIndex", "decode_start_index"),
        ("/finding", "pipelineId", "pipeline_id"),
        ("/finding", "modelId", "model_id"),
        ("/finding", "promptRevision", "prompt_revision"),
        ("/finding", "confidenceBasis", "confidence_basis"),
        ("/finding", "requestedRange", "requested_range"),
        ("/finding", "sourceSnapshotSha256", "source_snapshot_sha256"),
        ("/finding/answer/events/0", "trackIds", "track_ids"),
        (
            "/resultsArtifact/metadata/provenance",
            "analysisId",
            "analysis_id",
        ),
        (
            "/resultsArtifact/metadata/provenance",
            "pipelineId",
            "pipeline_id",
        ),
        (
            "/resultsArtifact/metadata/provenance",
            "modelId",
            "model_id",
        ),
        (
            "/resultsArtifact/metadata/provenance",
            "promptRevision",
            "prompt_revision",
        ),
        (
            "/resultsArtifact/metadata/provenance",
            "taskKind",
            "task_kind",
        ),
        (
            "/resultsArtifact/metadata/provenance",
            "sourceSnapshotSha256",
            "source_snapshot_sha256",
        ),
        (
            "/annotationsArtifact/metadata/provenance",
            "analysisId",
            "analysis_id",
        ),
        (
            "/annotationsArtifact/metadata/provenance",
            "resultsArtifactUri",
            "results_artifact_uri",
        ),
        (
            "/annotationsArtifact/metadata/provenance",
            "sourceSnapshotSha256",
            "source_snapshot_sha256",
        ),
    ] {
        for mode in ["replacement", "mixed", "conflicting"] {
            let mut wire = current.clone();
            let fields = wire.pointer_mut(path).unwrap().as_object_mut().unwrap();
            let value = fields[key].clone();
            if mode == "replacement" {
                fields.remove(key);
            }
            fields.insert(
                retired.into(),
                if mode == "conflicting" {
                    serde_json::Value::Null
                } else {
                    value
                },
            );
            assert!(
                serde_json::from_value::<AnalyzeRecordingOutput>(wire).is_err(),
                "{path}/{retired} {mode}"
            );
        }
    }
    let mut old = current;
    old["schema"] = "veoveo.ai/reason-analysis/v1".into();
    assert!(serde_json::from_value::<AnalyzeRecordingOutput>(old).is_err());
}

#[test]
fn analysis_public_decoder_refuses_visible_source_and_kind_contradictions() {
    let current = serde_json::to_value(
        analysis()
            .with_output(Some(output(ID, "traffic-events")))
            .unwrap(),
    )
    .unwrap();
    for (field, value) in [
        ("recordingUri", format!("recording://recordings/{OTHER}")),
        ("entityPath", "/other".into()),
        ("timeline", "other_time".into()),
        ("taskKind", "answer_question".into()),
    ] {
        let mut changed = current.clone();
        *changed.pointer_mut(&format!("/{field}")).unwrap() = value.into();
        let error = serde_json::from_value::<AnalysisView>(changed).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("analysis output source and kind"),
            "{field}: {error}"
        );
    }
}
