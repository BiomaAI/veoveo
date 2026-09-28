use veoveo_stream_mcp::contract::*;

#[path = "contract/replay.rs"]
mod replay;
#[path = "contract/resources.rs"]
mod resources;

#[test]
fn replay_requests_share_the_video_owner_and_preserve_defaults() {
    let request: RunRecordingRequest = serde_json::from_value(serde_json::json!({
        "video": {
            "recording_uri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
            "entity_path": "/camera/front",
            "timeline": "sensor_time",
            "range": {"start": 10, "end": 20}
        },
        "pipeline_id": "traffic"
    }))
    .unwrap();
    veoveo_recording_video::contract::validate_video_selection(&request.video).unwrap();
    assert!(matches!(request.sampling, SamplingPolicy::EveryFrame));
    assert!(!request.include_source_clip);
    assert_eq!(
        serde_json::to_value(request.sampling).unwrap(),
        serde_json::json!({"mode": "every_frame"})
    );
}

#[test]
fn live_admission_and_lifecycle_types_are_available_to_clients() {
    let request: StartLiveSessionRequest =
        serde_json::from_value(serde_json::json!({"pipeline_id": "traffic"})).unwrap();
    assert_eq!(request.pipeline_id.as_str(), "traffic");
    assert!(
        serde_json::from_value::<StartLiveSessionRequest>(serde_json::json!({
            "pipeline_id": "traffic", "launch": "untrusted graph"
        }))
        .is_err()
    );
    let page: LiveSessionsPage =
        serde_json::from_value(serde_json::json!({"sessions": [], "limit": 100})).unwrap();
    assert!(page.next_cursor.is_none());
    assert_eq!(
        serde_json::to_value(page).unwrap(),
        serde_json::json!({"sessions": [], "limit": 100})
    );
    for (kind, wire) in [
        (LiveSessionLifecycle::Starting, "starting"),
        (LiveSessionLifecycle::Running, "running"),
        (LiveSessionLifecycle::Failed, "failed"),
        (LiveSessionLifecycle::Stopped, "stopped"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<LiveSessionLifecycle>(wire.into()).unwrap(),
            kind
        );
    }
    assert_eq!(
        serde_json::to_value(LiveTransport::RtpH264Udp).unwrap(),
        "rtp_h264_udp"
    );
}

#[test]
fn schemas_preserve_the_published_contract() {
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/contract.schema.json")).unwrap();
    macro_rules! check { ($($ty:ty),+ $(,)?) => { $(assert_eq!(serde_json::to_value(schemars::schema_for!($ty)).unwrap(), baseline[stringify!($ty)], stringify!($ty));)+ }; }
    check!(
        RunRecordingRequest,
        SamplingPolicy,
        BoundingBox2D,
        Detection,
        FrameDetections,
        AnalysisResults,
        RunRecordingOutput,
        AnalysisSummary,
        PipelineView,
        PipelineProfile,
        PerceptionOperation,
        ModelView,
        ModelFormat,
        RunView,
        RunPage,
        LiveResultFrame,
        LiveResultsView,
        EncodedVideoChunk,
        LivePreviewView,
        IndexRange,
        RecordingSourceIdentity,
        RecordingSourceIdentityKind,
        RecordingSourceSnapshot,
        RecordingVideoSelection,
        VideoTimelineKind,
        StartLiveSessionRequest,
        StopLiveSessionRequest,
        StartLiveSessionOutput,
        StopLiveSessionOutput,
        LiveIngressView,
        LiveVideoView,
        LiveTransport,
        LiveSessionLifecycle,
        LiveRecordingLifecycle,
        LiveRecordingOutputView,
        LiveSessionView,
        LiveSessionsPage,
    );
}
