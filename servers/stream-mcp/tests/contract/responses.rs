use serde_json::Value;
use veoveo_stream_mcp::contract::*;

const ID: &str = "01983da0-0000-7000-8000-000000000001";
const OTHER: &str = "01983da0-0000-7000-8000-000000000002";

fn output(id: &str, pipeline: &str) -> RunRecordingOutput {
    let mut wire: Value =
        serde_json::from_str(include_str!("../../testdata/run-output.json")).unwrap();
    wire["runUri"] = format!("stream://run/{id}").into();
    wire["resultUri"] = format!("stream://run/{id}/results").into();
    wire["pipelineUri"] = format!("stream://pipeline/{pipeline}").into();
    wire["resultsArtifact"]["metadata"]["provenance"]["runId"] = id.into();
    wire["resultsArtifact"]["metadata"]["provenance"]["pipelineId"] = pipeline.into();
    wire["annotationsArtifact"]["metadata"]["provenance"]["runId"] = id.into();
    serde_json::from_value(wire).unwrap()
}

fn run() -> RunView {
    RunView::new(
        ID.parse().unwrap(),
        "traffic".parse().unwrap(),
        RunDetails {
            status: veoveo_task_contract::TaskStatus::Succeeded,
            progress: 1.0,
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

fn pipeline_details() -> PipelineDetails {
    PipelineDetails {
        title: "Traffic".into(),
        description: "Traffic camera".into(),
        supports_recording_replay: true,
        supports_live_input: true,
    }
}

#[test]
fn catalog_ids_and_profile_model_relationships_are_checked() {
    let model = ModelView::new(
        "detector".parse().unwrap(),
        "Detector".into(),
        "Vehicles".into(),
        ModelFormat::TensorRtEngine,
    );
    let pipeline = PipelineView::perception(
        "traffic".parse().unwrap(),
        model.id().clone(),
        PerceptionOperation::ObjectDetection,
        false,
        pipeline_details(),
    );
    let model_wire = serde_json::to_value(model).unwrap();
    let pipeline_wire = serde_json::to_value(pipeline).unwrap();
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
    for (field, value) in [("id", "other"), ("uri", "stream://model/other")] {
        let mut invalid = model_wire.clone();
        invalid[field] = value.into();
        assert!(serde_json::from_value::<ModelView>(invalid).is_err());
    }
    for (field, value) in [("id", "other"), ("uri", "stream://pipeline/other")] {
        let mut invalid = pipeline_wire.clone();
        invalid[field] = value.into();
        assert!(serde_json::from_value::<PipelineView>(invalid).is_err());
    }
    let mut missing_model = pipeline_wire;
    missing_model.as_object_mut().unwrap().remove("modelUri");
    assert!(serde_json::from_value::<PipelineView>(missing_model).is_err());
    let pass = PipelineView::pass_through("relay".parse().unwrap(), pipeline_details());
    assert!(pass.model_uri().is_none());
    let mut pass_wire = serde_json::to_value(pass).unwrap();
    assert!(serde_json::from_value::<PipelineView>(pass_wire.clone()).is_ok());
    pass_wire["modelUri"] = "stream://model/detector".into();
    assert!(serde_json::from_value::<PipelineView>(pass_wire).is_err());
}

#[test]
fn output_addresses_cannot_disagree() {
    let wire = serde_json::to_value(output(ID, "traffic")).unwrap();
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<RunRecordingOutput>(wire.clone()).unwrap())
            .unwrap(),
        wire
    );
    for (field, address) in [
        ("runUri", format!("stream://run/{OTHER}")),
        ("resultUri", format!("stream://run/{OTHER}/results")),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = address.into();
        let error = serde_json::from_value::<RunRecordingOutput>(invalid).unwrap_err();
        assert!(error.to_string().contains("run and results URIs"));
        assert!(!error.to_string().contains(OTHER));
    }
}

#[test]
fn run_builder_and_wire_bind_output_to_the_task_and_pipeline() {
    let view = run().with_output(Some(output(ID, "traffic"))).unwrap();
    assert_eq!(view.task_id(), view.output().unwrap().run_id());
    let wire = serde_json::to_value(view).unwrap();
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<RunView>(wire.clone()).unwrap()).unwrap(),
        wire
    );
    for unrelated in [output(OTHER, "traffic"), output(ID, "different")] {
        assert!(run().with_output(Some(unrelated.clone())).is_err());
        let mut invalid = wire.clone();
        invalid["output"] = serde_json::to_value(unrelated).unwrap();
        assert!(serde_json::from_value::<RunView>(invalid).is_err());
    }
    for (field, value) in [
        ("taskId", OTHER.to_owned()),
        ("runUri", format!("stream://run/{OTHER}")),
        ("resultsUri", format!("stream://run/{OTHER}/results")),
        ("pipelineId", "different".into()),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value.into();
        assert!(
            serde_json::from_value::<RunView>(invalid).is_err(),
            "{field}"
        );
    }
    assert!(serde_json::to_value(run()).unwrap().get("output").is_none());
}

fn ingress() -> LiveIngressView {
    LiveIngressView {
        transport: LiveTransport::RtpH264Udp,
        host: "stream-mcp".into(),
        port: 9001,
        payload_type: 97,
        clock_rate: 90_000,
        caps: "application/x-rtp".into(),
    }
}
fn video() -> LiveVideoView {
    LiveVideoView {
        codec: "avc1.42e01f".into(),
        width: 640,
        height: 480,
        frame_rate: 30,
        expected_bitrate_bps: 4_000_000,
    }
}

#[test]
fn live_builders_share_one_identity_and_decoders_reject_foreign_addresses() {
    let start = StartLiveSessionOutput::new(
        ID.parse().unwrap(),
        "traffic".parse().unwrap(),
        LiveStartDetails {
            ingress: ingress(),
            video: video(),
            recording_output: None,
            started_at: "2026-09-28T00:00:00Z".into(),
        },
    );
    let view = LiveSessionView::new(
        start.session_id(),
        start.pipeline_id().clone(),
        LiveSessionDetails {
            ingress: ingress(),
            video: video(),
            recording_output: None,
            lifecycle: LiveSessionLifecycle::Running,
            started_at: start.started_at.clone(),
            stopped_at: None,
            received_video_frames: 10,
            processed_frames: 10,
            newest_result_at: None,
            error: None,
        },
    );
    assert_eq!(start.result_uri(), view.session_uri());
    assert_eq!(start.results_uri(), view.results_uri());
    assert_eq!(start.preview_uri(), view.preview_uri());
    let stop = StopLiveSessionOutput {
        result_uri: start.result_uri(),
        lifecycle: LiveSessionLifecycle::Stopped,
        received_video_frames: 10,
        processed_frames: 10,
        recording_output: None,
        stopped_at: "2026-09-28T00:01:00Z".into(),
    };
    let stop_wire = serde_json::to_value(stop).unwrap();
    assert_eq!(
        stop_wire["resultUri"],
        serde_json::to_value(start.result_uri()).unwrap()
    );
    assert!(serde_json::from_value::<StopLiveSessionOutput>(stop_wire.clone()).is_ok());
    let mut missing = stop_wire;
    missing.as_object_mut().unwrap().remove("resultUri");
    assert!(serde_json::from_value::<StopLiveSessionOutput>(missing).is_err());
    let start_wire = serde_json::to_value(start).unwrap();
    let view_wire = serde_json::to_value(view).unwrap();
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<StartLiveSessionOutput>(start_wire.clone()).unwrap()
        )
        .unwrap(),
        start_wire
    );
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<LiveSessionView>(view_wire.clone()).unwrap())
            .unwrap(),
        view_wire
    );
    for (field, value) in [
        ("sessionId", OTHER.to_owned()),
        ("sessionUri", format!("stream://session/{OTHER}")),
        ("resultsUri", format!("stream://session/{OTHER}/results")),
        ("previewUri", format!("stream://session/{OTHER}/preview")),
    ] {
        let mut invalid = start_wire.clone();
        invalid[if field == "sessionUri" {
            "resultUri"
        } else {
            field
        }] = Value::String(value.clone());
        assert!(
            serde_json::from_value::<StartLiveSessionOutput>(invalid).is_err(),
            "{field}"
        );
        let mut invalid = view_wire.clone();
        invalid[field] = Value::String(value);
        assert!(
            serde_json::from_value::<LiveSessionView>(invalid).is_err(),
            "{field}"
        );
    }
    for (field, value) in [
        ("pipelineId", "other"),
        ("pipelineUri", "stream://pipeline/other"),
    ] {
        let mut invalid = view_wire.clone();
        invalid[field] = value.into();
        assert!(serde_json::from_value::<LiveSessionView>(invalid).is_err());
    }
}

#[test]
fn recording_output_requires_its_canonical_result_uri() {
    let mut value = serde_json::to_value(output(ID, "traffic")).unwrap();
    assert_eq!(value["resultUri"], format!("stream://run/{ID}/results"));
    value.as_object_mut().unwrap().remove("resultUri");
    assert!(serde_json::from_value::<RunRecordingOutput>(value).is_err());
}

#[test]
fn recording_references_use_the_recording_owner_admission() {
    let valid = serde_json::to_value(run()).unwrap();
    serde_json::from_value::<RunView>(valid.clone()).unwrap();
    for address in [
        "recording://recording/01983da0-0000-7000-8000-000000000002",
        "recording://recordings/01983da0-0000-7000-8000-000000000002/layers",
        "recording://recordings/01983da0-0000-4000-8000-000000000002",
    ] {
        let mut invalid = valid.clone();
        invalid["recordingUri"] = address.into();
        let error = serde_json::from_value::<RunView>(invalid).unwrap_err();
        assert!(!error.to_string().contains(address));
    }
}

#[test]
fn terminal_artifact_provenance_and_visible_ranges_share_constructor_admission() {
    let good = output(ID, "traffic");
    let original = serde_json::to_value(&good).unwrap();
    for (pointer, value) in [
        ("/summary/requestedEndIndex", serde_json::json!(9)),
        ("/summary/decodeStartIndex", serde_json::json!(11)),
        (
            "/resultsArtifact/metadata/provenance/runId",
            serde_json::json!(OTHER),
        ),
        (
            "/resultsArtifact/metadata/provenance/pipelineId",
            serde_json::json!("other"),
        ),
        (
            "/annotationsArtifact/metadata/provenance/recordingId",
            serde_json::json!(OTHER),
        ),
        (
            "/annotationsArtifact/metadata/provenance/resultsArtifactUri",
            serde_json::json!(format!("stream://artifact/{OTHER}")),
        ),
        (
            "/annotationsArtifact/metadata/provenance/sourceSnapshotSha256",
            serde_json::json!("b".repeat(64)),
        ),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            serde_json::from_value::<RunRecordingOutput>(bad).is_err(),
            "{pointer}"
        );
    }
    assert!(
        RunRecordingOutput::new(
            OTHER.parse().unwrap(),
            good.pipeline_uri.id().clone(),
            good.model_uri.id().clone(),
            good.summary.clone(),
            good.results_artifact.clone(),
            good.annotations_artifact.clone()
        )
        .is_err()
    );
    let mut clip = good.results_artifact.clone();
    let mut clip_wire = serde_json::to_value(&clip).unwrap();
    clip_wire["artifactId"] = "01983da0-0000-7000-8000-000000000004".into();
    clip_wire["artifactUri"] = "stream://artifact/01983da0-0000-7000-8000-000000000004".into();
    clip = serde_json::from_value(clip_wire).unwrap();
    clip.metadata = serde_json::json!({"provenance":{
        "kind":"stream_source_clip", "runId":ID,
        "recordingId":"01983da0-0000-7000-8000-000000000000",
        "entityPath":"/camera", "timeline":"sensor_time", "decodeStartIndex":0,
        "sourceSnapshotSha256":"a".repeat(64)}});
    let with_clip = good.clone().with_source_clip(Some(clip.clone())).unwrap();
    assert!(
        serde_json::from_value::<RunRecordingOutput>(serde_json::to_value(with_clip).unwrap())
            .is_ok()
    );
    clip.metadata["provenance"]["decodeStartIndex"] = 1.into();
    assert!(good.with_source_clip(Some(clip)).is_err());
}

#[test]
fn admitted_products_emit_the_actual_object_schemas() {
    let schema = serde_json::to_value(schemars::schema_for!(AnalysisResults)).unwrap();
    assert_eq!(schema["type"], "object", "AnalysisResults");
    assert!(schema.get("$ref").is_none(), "AnalysisResults");
    let schema = serde_json::to_value(schemars::schema_for!(Detection)).unwrap();
    assert_eq!(schema["type"], "object", "Detection");
    assert!(schema.get("$ref").is_none(), "Detection");
    let schema = serde_json::to_value(schemars::schema_for!(RunRecordingOutput)).unwrap();
    assert_eq!(schema["type"], "object", "RunRecordingOutput");
    assert!(schema.get("$ref").is_none(), "RunRecordingOutput");
}

#[test]
fn terminal_roles_cannot_reuse_one_immutable_artifact_occurrence() {
    let mut wire = serde_json::to_value(output(ID, "traffic")).unwrap();
    let mut clip = wire["annotationsArtifact"].clone();
    clip["artifactId"] = "01983da0-0000-7000-8000-000000000004".into();
    clip["artifactUri"] = "stream://artifact/01983da0-0000-7000-8000-000000000004".into();
    clip["metadata"]["provenance"] = serde_json::json!({
        "kind": "stream_source_clip", "runId": ID,
        "recordingId": "01983da0-0000-7000-8000-000000000000", "entityPath":"/camera/front", "timeline":"sensor_time",
        "decodeStartIndex": wire["summary"]["decodeStartIndex"],
        "sourceSnapshotSha256": wire["resultsArtifact"]["metadata"]["provenance"]["sourceSnapshotSha256"]
    });
    wire["sourceClipArtifact"] = clip;
    assert!(serde_json::from_value::<RunRecordingOutput>(wire.clone()).is_ok());
    for (target, source) in [
        ("annotationsArtifact", "resultsArtifact"),
        ("sourceClipArtifact", "resultsArtifact"),
        ("sourceClipArtifact", "annotationsArtifact"),
    ] {
        let mut bad = wire.clone();
        bad[target]["artifactId"] = wire[source]["artifactId"].clone();
        bad[target]["artifactUri"] = wire[source]["artifactUri"].clone();
        assert!(
            serde_json::from_value::<RunRecordingOutput>(bad.clone()).is_err(),
            "{target}/{source}"
        );
        assert!(
            serde_json::from_value::<RunRecordingOutputBuilder>(bad)
                .unwrap()
                .build()
                .is_err()
        );
    }
}

#[test]
fn current_task_status_vocabulary_rejects_unknown_response_values() {
    let current = serde_json::to_value(run()).unwrap();
    assert_eq!(current["status"], "succeeded");
    for status in ["unknown", "cancelRequested", "cancel-requested"] {
        let mut changed = current.clone();
        changed["status"] = status.into();
        assert!(serde_json::from_value::<RunView>(changed).is_err());
    }
}

#[test]
fn run_public_decoder_refuses_visible_recording_contradiction() {
    let mut current =
        serde_json::to_value(run().with_output(Some(output(ID, "traffic"))).unwrap()).unwrap();
    *current.pointer_mut("/recordingUri").unwrap() =
        format!("recording://recordings/{OTHER}").into();
    let error = serde_json::from_value::<RunView>(current).unwrap_err();
    assert!(
        error.to_string().contains("run output recording"),
        "{error}"
    );
}

#[test]
fn run_public_decoder_refuses_visible_clip_source_contradictions() {
    let mut wire = serde_json::to_value(output(ID, "traffic")).unwrap();
    let mut clip = wire["annotationsArtifact"].clone();
    clip["artifactId"] = "01983da0-0000-7000-8000-000000000004".into();
    clip["artifactUri"] = "stream://artifact/01983da0-0000-7000-8000-000000000004".into();
    clip["metadata"]["provenance"] = serde_json::json!({
        "kind": "stream_source_clip", "runId": ID,
        "recordingId": "01983da0-0000-7000-8000-000000000000",
        "entityPath": "/camera/front", "timeline": "sensor_time", "decodeStartIndex": 0,
        "sourceSnapshotSha256": wire["resultsArtifact"]["metadata"]["provenance"]["sourceSnapshotSha256"]
    });
    wire["sourceClipArtifact"] = clip;
    let current = serde_json::to_value(
        run()
            .with_output(Some(serde_json::from_value(wire).unwrap()))
            .unwrap(),
    )
    .unwrap();
    assert!(serde_json::from_value::<RunView>(current.clone()).is_ok());
    for (pointer, value) in [("/entityPath", "/other"), ("/timeline", "other_time")] {
        let mut changed = current.clone();
        *changed.pointer_mut(pointer).unwrap() = value.into();
        let error = serde_json::from_value::<RunView>(changed).unwrap_err();
        assert!(
            error.to_string().contains("run clip entity and timeline"),
            "{pointer}: {error}"
        );
    }
}
