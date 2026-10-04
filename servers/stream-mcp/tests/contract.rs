fn check_schema<T: schemars::JsonSchema>(baseline: &serde_json::Value, name: &str) {
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(T)).unwrap(),
        baseline[name],
        "{name}"
    );
}
use veoveo_stream_mcp::contract::*;

#[path = "contract/replay.rs"]
mod replay;
#[path = "contract/resources.rs"]
mod resources;
#[path = "contract/responses.rs"]
mod responses;

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

    check_schema::<RunRecordingRequest>(&baseline, stringify!(RunRecordingRequest));
    check_schema::<SamplingPolicy>(&baseline, stringify!(SamplingPolicy));
    check_schema::<BoundingBox2D>(&baseline, stringify!(BoundingBox2D));
    check_schema::<Detection>(&baseline, stringify!(Detection));
    check_schema::<FrameDetections>(&baseline, stringify!(FrameDetections));
    check_schema::<AnalysisResults>(&baseline, stringify!(AnalysisResults));
    check_schema::<RunRecordingOutput>(&baseline, stringify!(RunRecordingOutput));
    check_schema::<AnalysisSummary>(&baseline, stringify!(AnalysisSummary));
    check_schema::<PipelineView>(&baseline, stringify!(PipelineView));
    check_schema::<PipelineProfile>(&baseline, stringify!(PipelineProfile));
    check_schema::<PerceptionOperation>(&baseline, stringify!(PerceptionOperation));
    check_schema::<ModelView>(&baseline, stringify!(ModelView));
    check_schema::<ModelFormat>(&baseline, stringify!(ModelFormat));
    check_schema::<RunView>(&baseline, stringify!(RunView));
    check_schema::<RunPage>(&baseline, stringify!(RunPage));
    check_schema::<LiveResultFrame>(&baseline, stringify!(LiveResultFrame));
    check_schema::<LiveResultsView>(&baseline, stringify!(LiveResultsView));
    check_schema::<EncodedVideoChunk>(&baseline, stringify!(EncodedVideoChunk));
    check_schema::<LivePreviewView>(&baseline, stringify!(LivePreviewView));
    check_schema::<IndexRange>(&baseline, stringify!(IndexRange));
    check_schema::<RecordingSourceIdentity>(&baseline, stringify!(RecordingSourceIdentity));
    check_schema::<RecordingSourceIdentityKind>(&baseline, stringify!(RecordingSourceIdentityKind));
    check_schema::<RecordingSourceSnapshot>(&baseline, stringify!(RecordingSourceSnapshot));
    check_schema::<RecordingVideoSelection>(&baseline, stringify!(RecordingVideoSelection));
    check_schema::<VideoTimelineKind>(&baseline, stringify!(VideoTimelineKind));
    check_schema::<StartLiveSessionRequest>(&baseline, stringify!(StartLiveSessionRequest));
    check_schema::<StopLiveSessionRequest>(&baseline, stringify!(StopLiveSessionRequest));
    check_schema::<StartLiveSessionOutput>(&baseline, stringify!(StartLiveSessionOutput));
    check_schema::<StopLiveSessionOutput>(&baseline, stringify!(StopLiveSessionOutput));
    check_schema::<LiveIngressView>(&baseline, stringify!(LiveIngressView));
    check_schema::<LiveVideoView>(&baseline, stringify!(LiveVideoView));
    check_schema::<LiveTransport>(&baseline, stringify!(LiveTransport));
    check_schema::<LiveSessionLifecycle>(&baseline, stringify!(LiveSessionLifecycle));
    check_schema::<LiveRecordingLifecycle>(&baseline, stringify!(LiveRecordingLifecycle));
    check_schema::<LiveRecordingOutputView>(&baseline, stringify!(LiveRecordingOutputView));
    check_schema::<LiveSessionView>(&baseline, stringify!(LiveSessionView));
    check_schema::<LiveSessionsPage>(&baseline, stringify!(LiveSessionsPage));
}

#[test]
fn sampling_variants_reject_extra_fields() {
    for mut input in [
        serde_json::json!({"mode":"every_frame"}),
        serde_json::json!({"mode":"every_nth","step":2}),
        serde_json::json!({"mode":"maximum_frames","count":3}),
    ] {
        assert!(serde_json::from_value::<SamplingPolicy>(input.clone()).is_ok());
        input["undeclared"] = serde_json::json!(true);
        assert!(serde_json::from_value::<SamplingPolicy>(input).is_err());
    }
}
