fn check_schema<T: schemars::JsonSchema>(
    current: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
) {
    current.insert(
        name.to_owned(),
        serde_json::to_value(schemars::schema_for!(T)).unwrap(),
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
            "recordingUri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
            "entityPath": "/camera/front",
            "timeline": "sensor_time",
            "range": {"start": 10, "end": 20}
        },
        "pipelineId": "traffic"
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
        serde_json::from_value(serde_json::json!({"pipelineId": "traffic"})).unwrap();
    assert_eq!(request.pipeline_id.as_str(), "traffic");
    assert!(
        serde_json::from_value::<StartLiveSessionRequest>(serde_json::json!({
            "pipelineId": "traffic", "launch": "untrusted graph"
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
    let mut current = serde_json::Map::new();

    check_schema::<RunRecordingRequest>(&mut current, stringify!(RunRecordingRequest));
    check_schema::<SamplingPolicy>(&mut current, stringify!(SamplingPolicy));
    check_schema::<StreamUsageMetadata>(&mut current, stringify!(StreamUsageMetadata));
    check_schema::<BoundingBox2D>(&mut current, stringify!(BoundingBox2D));
    check_schema::<Detection>(&mut current, stringify!(Detection));
    check_schema::<FrameDetections>(&mut current, stringify!(FrameDetections));
    check_schema::<AnalysisResults>(&mut current, stringify!(AnalysisResults));
    check_schema::<RunRecordingOutput>(&mut current, stringify!(RunRecordingOutput));
    check_schema::<AnalysisSummary>(&mut current, stringify!(AnalysisSummary));
    check_schema::<PipelineView>(&mut current, stringify!(PipelineView));
    check_schema::<PipelineProfile>(&mut current, stringify!(PipelineProfile));
    check_schema::<PerceptionOperation>(&mut current, stringify!(PerceptionOperation));
    check_schema::<ModelView>(&mut current, stringify!(ModelView));
    check_schema::<ModelFormat>(&mut current, stringify!(ModelFormat));
    check_schema::<RunView>(&mut current, stringify!(RunView));
    check_schema::<RunPage>(&mut current, stringify!(RunPage));
    check_schema::<LiveResultFrame>(&mut current, stringify!(LiveResultFrame));
    check_schema::<LiveResultsView>(&mut current, stringify!(LiveResultsView));
    check_schema::<EncodedVideoChunk>(&mut current, stringify!(EncodedVideoChunk));
    check_schema::<LivePreviewView>(&mut current, stringify!(LivePreviewView));
    check_schema::<IndexRange>(&mut current, stringify!(IndexRange));
    check_schema::<RecordingSourceIdentity>(&mut current, stringify!(RecordingSourceIdentity));
    check_schema::<RecordingSourceIdentityKind>(
        &mut current,
        stringify!(RecordingSourceIdentityKind),
    );
    check_schema::<RecordingSourceSnapshot>(&mut current, stringify!(RecordingSourceSnapshot));
    check_schema::<RecordingVideoSelection>(&mut current, stringify!(RecordingVideoSelection));
    check_schema::<VideoTimelineKind>(&mut current, stringify!(VideoTimelineKind));
    check_schema::<StartLiveSessionRequest>(&mut current, stringify!(StartLiveSessionRequest));
    check_schema::<StopLiveSessionRequest>(&mut current, stringify!(StopLiveSessionRequest));
    check_schema::<StartLiveSessionOutput>(&mut current, stringify!(StartLiveSessionOutput));
    check_schema::<StopLiveSessionOutput>(&mut current, stringify!(StopLiveSessionOutput));
    check_schema::<LiveIngressView>(&mut current, stringify!(LiveIngressView));
    check_schema::<LiveVideoView>(&mut current, stringify!(LiveVideoView));
    check_schema::<LiveTransport>(&mut current, stringify!(LiveTransport));
    check_schema::<LiveSessionLifecycle>(&mut current, stringify!(LiveSessionLifecycle));
    check_schema::<LiveRecordingLifecycle>(&mut current, stringify!(LiveRecordingLifecycle));
    check_schema::<LiveRecordingOutputView>(&mut current, stringify!(LiveRecordingOutputView));
    check_schema::<LiveSessionView>(&mut current, stringify!(LiveSessionView));
    check_schema::<LiveSessionsPage>(&mut current, stringify!(LiveSessionsPage));
    let current = serde_json::Value::Object(current);
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/contract.schema.json");
    if let Some(capture) = std::env::var_os("CAPTURE_STREAM_CONTRACT_SCHEMA") {
        use std::io::Write;
        let capture = std::path::PathBuf::from(capture);
        assert!(
            capture.is_absolute(),
            "capture requires an absolute scratch path"
        );
        assert!(
            capture
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .starts_with(std::env::temp_dir().canonicalize().unwrap()),
            "capture parent must be an existing scratch directory"
        );
        let bytes = serde_json::to_string_pretty(&current).unwrap() + "\n";
        assert!(bytes.len() <= 1024 * 1024, "capture exceeds 1 MiB");
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(capture)
            .unwrap();
        output.write_all(bytes.as_bytes()).unwrap();
        output.sync_all().unwrap();
        return;
    }
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(current, baseline, "published Stream contract schema drift");
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

#[test]
fn current_format_tags_declare_scalar_role_and_profiles_refuse_unknown_members() {
    for schema in [
        schemars::schema_for!(StreamResultsSchema),
        schemars::schema_for!(LiveResultsSchema),
        schemars::schema_for!(LivePreviewSchema),
    ] {
        assert_eq!(
            veoveo_types::naming_profile(&schema, veoveo_types::NamingSchemaContext::new(&schema))
                .unwrap()
                .unwrap()
                .role(),
            &veoveo_types::NamingRole::Scalar {
                profile: veoveo_types::ScalarNaming::builtin(
                    veoveo_types::ScalarGrammar::FormatTag
                )
            }
        );
    }
    for current in [
        serde_json::json!({"kind":"pass_through"}),
        serde_json::json!({"kind":"perception","operation":"object_detection","tracking":false}),
    ] {
        assert!(serde_json::from_value::<PipelineProfile>(current.clone()).is_ok());
        let mut mixed = current;
        mixed["undeclared"] = true.into();
        assert!(serde_json::from_value::<PipelineProfile>(mixed).is_err());
    }
}
