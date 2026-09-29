use serde_json::{Value, json};
use veoveo_stream_mcp::contract::*;

const ID: &str = "01983da0-0000-7000-8000-000000000001";
const OTHER: &str = "01983da0-0000-7000-8000-000000000002";

fn output(id: &str, pipeline: &str) -> RunRecordingOutput {
    let artifact = json!({"artifact_id": ID, "artifact_uri": format!("stream://artifact/{ID}"), "byte_len": 12, "created_at": "2026-09-28T00:00:00Z"});
    RunRecordingOutput::new(
        id.parse().unwrap(),
        pipeline.parse().unwrap(),
        "detector".parse().unwrap(),
        AnalysisSummary {
            processed_frames: 10,
            detection_count: 3,
            elapsed_ms: 10,
            decode_start_index: 0,
            requested_start_index: 10,
            requested_end_index: 20,
        },
        serde_json::from_value(artifact.clone()).unwrap(),
        serde_json::from_value(artifact).unwrap(),
    )
}

fn run() -> RunView {
    RunView::new(
        ID.parse().unwrap(),
        "traffic".parse().unwrap(),
        RunDetails {
            status: "succeeded".into(),
            progress: 1.0,
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
    missing_model.as_object_mut().unwrap().remove("model_uri");
    assert!(serde_json::from_value::<PipelineView>(missing_model).is_err());
    let pass = PipelineView::pass_through("relay".parse().unwrap(), pipeline_details());
    assert!(pass.model_uri().is_none());
    let mut pass_wire = serde_json::to_value(pass).unwrap();
    assert!(serde_json::from_value::<PipelineView>(pass_wire.clone()).is_ok());
    pass_wire["model_uri"] = "stream://model/detector".into();
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
        ("run_uri", format!("stream://run/{OTHER}")),
        ("result_uri", format!("stream://run/{OTHER}/results")),
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
        ("task_id", OTHER.to_owned()),
        ("run_uri", format!("stream://run/{OTHER}")),
        ("results_uri", format!("stream://run/{OTHER}/results")),
        ("pipeline_id", "different".into()),
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
        stop_wire["result_uri"],
        serde_json::to_value(start.result_uri()).unwrap()
    );
    assert!(serde_json::from_value::<StopLiveSessionOutput>(stop_wire.clone()).is_ok());
    let mut missing = stop_wire;
    missing.as_object_mut().unwrap().remove("result_uri");
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
        ("session_id", OTHER.to_owned()),
        ("session_uri", format!("stream://session/{OTHER}")),
        ("results_uri", format!("stream://session/{OTHER}/results")),
        ("preview_uri", format!("stream://session/{OTHER}/preview")),
    ] {
        let mut invalid = start_wire.clone();
        invalid[if field == "session_uri" {
            "result_uri"
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
        ("pipeline_id", "other"),
        ("pipeline_uri", "stream://pipeline/other"),
    ] {
        let mut invalid = view_wire.clone();
        invalid[field] = value.into();
        assert!(serde_json::from_value::<LiveSessionView>(invalid).is_err());
    }
}

#[test]
fn recording_output_requires_its_canonical_result_uri() {
    let mut value = serde_json::to_value(output(ID, "traffic")).unwrap();
    assert_eq!(value["result_uri"], format!("stream://run/{ID}/results"));
    value.as_object_mut().unwrap().remove("result_uri");
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
        invalid["recording_uri"] = address.into();
        let error = serde_json::from_value::<RunView>(invalid).unwrap_err();
        assert!(!error.to_string().contains(address));
    }
}
