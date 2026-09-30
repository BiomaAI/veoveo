//! Public live-recording consumers shared by focused and composed acceptance.
use super::*;
use veoveo_artifact_contract::ArtifactId;
use veoveo_reason_mcp::contract::{
    AnalyzeRecordingOutput, AnalyzeRecordingRequest, GroundingReference, ObservationSampling,
    ReasoningResults, ReasoningTask,
};
use veoveo_recording_contract::{RecordingDatasetId, RecordingView};
use veoveo_stream_mcp::contract::{
    AnalysisResults, IndexRange, RecordingSourceIdentityKind, RecordingSourceSnapshot,
    RecordingVideoSelection, RunRecordingOutput, RunRecordingRequest, SamplingPolicy,
    StreamArtifactUri,
};
use veoveo_uav_sim_mcp::contract::{CameraLifecycle, RecordingPublisherLifecycle, SimulationState};

#[derive(serde::Serialize)]
pub(super) struct RecordingAcceptance {
    schema: &'static str,
    video: RecordingVideoSelection,
    pub(super) stream_artifact_id: ArtifactId,
    reason_artifact_id: ArtifactId,
    processed_frames: u64,
    observed_frames: u64,
}

// TODO(foundations): Qualify this focused live-part path on the reference installation
// before repeating composed flight and its timing acceptance.
pub(crate) async fn verify(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    installation.operator.validate_credentials()?;
    let operator = OperatorClient {
        conformance,
        installation,
    };
    let result = analyze(&operator, &scenario).await?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

pub(super) async fn analyze(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<RecordingAcceptance> {
    let state: SimulationState = serde_json::from_value(
        tokio::time::timeout(
            Duration::from_secs(30),
            wait_for_recording_catalog(operator, scenario, Duration::from_secs(30)),
        )
        .await
        .context("UAV recording catalog readiness exceeded 30 seconds")??,
    )
    .context("decoding UAV recording state")?;
    let video = selection(&state, scenario)?;
    let catalog: RecordingView = serde_json::from_value(
        operator
            .resource(&video.recording_uri.to_string(), Duration::from_secs(60))
            .await?,
    )?;
    ensure!(
        catalog.recording_id == video.recording_uri.id()
            && catalog.state == veoveo_recording_contract::RecordingState::Live,
        "recording acceptance requires the selected live catalog entry"
    );

    // Each consumer snapshots acknowledged live parts through RecordingReader.
    // Archive projections cannot qualify that path, and failed Tasks are not retried.
    let replay = &scenario.stream.recording_replay;
    let request = RunRecordingRequest {
        video: video.clone(),
        pipeline_id: scenario.stream.live_pipeline_id.clone(),
        sampling: SamplingPolicy::MaximumFrames {
            count: replay.maximum_frames,
        },
        include_source_clip: true,
    };
    eprintln!(
        "Recording acceptance: Stream replay of {} at {}..={}",
        video.recording_uri, video.range.start, video.range.end
    );
    let stream: RunRecordingOutput = serde_json::from_value(
        operator
            .task_tool(
                "stream__run_recording",
                serde_json::to_value(request)?,
                Duration::from_secs(replay.task_timeout_seconds),
            )
            .await?,
    )
    .context("decoding Stream replay completion")?;
    ensure!(
        stream.summary.processed_frames > 0
            && stream.summary.requested_start_index == video.range.start
            && stream.summary.requested_end_index == video.range.end,
        "Stream replay omitted frames or changed the requested range"
    );
    let canonical_results = operator
        .resource(&stream.result_uri().to_string(), Duration::from_secs(60))
        .await?;
    let results: AnalysisResults = serde_json::from_value(canonical_results.clone())?;
    results.validate()?;
    ensure!(
        !results.frames.is_empty()
            && &results.pipeline_id == stream.pipeline_uri.id()
            && &results.model_id == stream.model_uri.id()
            && results.processed_frames == stream.summary.processed_frames,
        "Stream result resource does not match its completion"
    );
    assert_source(
        &results.recording_uri,
        &results.entity_path,
        &results.timeline,
        results.requested_range,
        &results.source_snapshot,
        &video,
        catalog.dataset_id,
    )?;
    let stream_artifact_id = stream.results_artifact.artifact_id();
    let published = download_governed_json_artifact(
        operator.conformance,
        operator.installation,
        &stream_artifact_id,
    )
    .await?;
    ensure!(
        published == canonical_results,
        "Stream result URI and published artifact disagree"
    );
    eprintln!("Recording acceptance: Stream replay passed; starting grounded Reason");

    let request = AnalyzeRecordingRequest {
        video: video.clone(),
        pipeline_id: "video-reasoning".parse()?,
        task: ReasoningTask::DescribeSegment {
            prompt: Some(scenario.reason.prompt.clone()),
        },
        sampling: ObservationSampling {
            max_frames: scenario.reason.maximum_frames,
        },
        decode: Default::default(),
        grounding: Some(GroundingReference {
            results_artifact_uri: StreamArtifactUri::parse(
                stream.results_artifact.artifact_uri.as_str(),
            )?,
        }),
        include_source_clip: false,
    };
    let reason: AnalyzeRecordingOutput = serde_json::from_value(
        operator
            .task_tool(
                "reason__analyze_recording",
                serde_json::to_value(request)?,
                Duration::from_secs(scenario.reason.task_timeout_seconds),
            )
            .await?,
    )
    .context("decoding Reason completion")?;
    ensure!(
        reason.summary.observed_frames > 0
            && reason.summary.requested_start_index == video.range.start
            && reason.summary.requested_end_index == video.range.end,
        "Reason omitted frames or changed the requested range"
    );
    let canonical_results = operator
        .resource(&reason.result_uri().to_string(), Duration::from_secs(60))
        .await?;
    let results: ReasoningResults = serde_json::from_value(canonical_results.clone())?;
    ensure!(
        &results.pipeline_id == reason.pipeline_uri.id()
            && &results.model_id == reason.model_uri.id()
            && results.observed_frames == reason.summary.observed_frames,
        "Reason result resource does not match its completion"
    );
    assert_source(
        &results.recording_uri,
        &results.entity_path,
        &results.timeline,
        results.requested_range,
        &results.source_snapshot,
        &video,
        catalog.dataset_id,
    )?;
    let reason_artifact_id = reason.results_artifact.artifact_id();
    let published = download_governed_json_artifact(
        operator.conformance,
        operator.installation,
        &reason_artifact_id,
    )
    .await?;
    ensure!(
        published == canonical_results,
        "Reason result URI and published artifact disagree"
    );
    eprintln!("Recording acceptance: grounded Reason passed");
    Ok(RecordingAcceptance {
        schema: "veoveo.ai/uav-recording-acceptance/v1",
        video,
        stream_artifact_id,
        reason_artifact_id,
        processed_frames: stream.summary.processed_frames,
        observed_frames: reason.summary.observed_frames,
    })
}

fn selection(
    state: &SimulationState,
    scenario: &UavAcceptanceScenario,
) -> Result<RecordingVideoSelection> {
    ensure!(
        state.session_id == scenario.session_id,
        "UAV returned another session"
    );
    let camera = state
        .cameras
        .iter()
        .find(|camera| camera.vehicle_id == scenario.vehicle_id)
        .context("selected UAV camera is absent")?;
    ensure!(
        camera.lifecycle == CameraLifecycle::Ready
            && camera.frames_observed >= 3
            && camera.last_access_unit_bytes > 0,
        "selected UAV camera must publish NVIDIA NVENC access units"
    );
    let mut recordings = state.recordings.iter().filter(|recording| {
        recording.active && recording.camera_streams.contains(&camera.entity_path)
    });
    let recording = recordings
        .next()
        .context("selected UAV camera has no active recording")?;
    ensure!(
        recordings.next().is_none(),
        "selected UAV camera has ambiguous active recordings"
    );
    ensure!(
        recording.publisher_lifecycle == RecordingPublisherLifecycle::Ready,
        "selected UAV recording publisher is not ready"
    );
    let recording_uri = recording
        .catalog
        .recording_uri()
        .context("UAV recording catalog is not ready")?
        .clone();
    let replay = &scenario.stream.recording_replay;
    let end = state.simulation_time_s - replay.range_lag_seconds;
    let start = end - replay.range_duration_seconds;
    ensure!(
        start.is_finite()
            && end.is_finite()
            && start >= 0.0
            && end > start
            && end * 1_000_000_000.0 < i64::MAX as f64,
        "UAV recording has insufficient or invalid simulation history"
    );
    Ok(RecordingVideoSelection {
        recording_uri,
        entity_path: camera.entity_path.clone(),
        timeline: "simulation_time".into(),
        range: IndexRange {
            start: (start * 1_000_000_000.0) as i64,
            end: (end * 1_000_000_000.0) as i64,
        },
    })
}

fn assert_source(
    recording: &veoveo_recording_contract::RecordingUri,
    entity: &str,
    timeline: &str,
    range: IndexRange,
    snapshot: &RecordingSourceSnapshot,
    requested: &RecordingVideoSelection,
    dataset: RecordingDatasetId,
) -> Result<()> {
    ensure!(
        recording == &requested.recording_uri
            && entity == requested.entity_path
            && timeline == requested.timeline
            && range.start == requested.range.start
            && range.end == requested.range.end
            && snapshot.recording_id == requested.recording_uri.id()
            && snapshot.dataset_id == dataset,
        "analysis result or source snapshot does not match the selected recording range"
    );
    ensure!(
        snapshot
            .sources
            .iter()
            .any(|source| source.kind == RecordingSourceIdentityKind::LiveIngestPart),
        "analysis did not capture acknowledged live ingest parts"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (SimulationState, UavAcceptanceScenario) {
        let scenario = UavAcceptanceScenario::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../showcase/uav-sim/scenarios/new-york-aerial.json"),
        )
        .unwrap();
        let mut state: SimulationState =
            serde_json::from_str(include_str!("../../tests/fixtures/world-ready.json")).unwrap();
        let recording_id = RecordingId::new();
        state
            .recordings
            .push(veoveo_uav_sim_mcp::contract::RecordingState {
                recording_key: "test-camera".parse().unwrap(),
                catalog: veoveo_uav_sim_mcp::contract::RecordingCatalog::Ready(
                    veoveo_recording_contract::RecordingUri::new(recording_id),
                ),
                active: true,
                publisher_lifecycle: RecordingPublisherLifecycle::Ready,
                queue_capacity: 64,
                queued_events: 0,
                dropped_events: 0,
                publisher_diagnostic: None,
                camera_streams: vec![state.cameras[0].entity_path.clone()],
                started_at: Utc::now(),
            });
        (state, scenario)
    }

    #[test]
    fn selection_rejects_wrong_inactive_ambiguous_or_unready_sources() {
        let (good, scenario) = fixture();
        let selected = selection(&good, &scenario).unwrap();
        assert_eq!(selected.entity_path, good.cameras[0].entity_path);
        assert_eq!(selected.range.end - selected.range.start, 25_000_000_000);
        for index in 0..7 {
            let mut state = good.clone();
            match index {
                0 => state.session_id = "other-session".parse().unwrap(),
                1 => state.recordings[0].active = false,
                2 => state.recordings.push(state.recordings[0].clone()),
                3 => state.recordings[0].camera_streams.clear(),
                4 => {
                    state.recordings[0].publisher_lifecycle = RecordingPublisherLifecycle::Degraded
                }
                5 => state.cameras[0].lifecycle = CameraLifecycle::Failed,
                _ => {
                    state.recordings[0].catalog =
                        veoveo_uav_sim_mcp::contract::RecordingCatalog::Pending { diagnostic: None }
                }
            }
            assert!(selection(&state, &scenario).is_err(), "case {index}");
        }
        for time in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e20] {
            let mut state = good.clone();
            state.simulation_time_s = time;
            assert!(selection(&state, &scenario).is_err());
        }
    }

    #[test]
    fn source_acceptance_rejects_archive_only_and_mismatched_results() {
        let (state, scenario) = fixture();
        let requested = selection(&state, &scenario).unwrap();
        let dataset = RecordingDatasetId::new();
        let snapshot = serde_json::json!({
            "recording_id": requested.recording_uri.id(), "dataset_id": dataset,
            "captured_at": Utc::now(), "sources": [{
                "layer_id": veoveo_recording_contract::RecordingLayerId::new(),
                "layer_name": "camera", "kind": "live_ingest_part", "part_sequence": 1,
                "byte_len": 100, "sha256": "ab".repeat(32)
            }]
        });
        let check = |snapshot: Value, video: &RecordingVideoSelection| {
            assert_source(
                &video.recording_uri,
                &video.entity_path,
                &video.timeline,
                video.range,
                &serde_json::from_value(snapshot).unwrap(),
                &requested,
                dataset,
            )
        };
        check(snapshot.clone(), &requested).unwrap();
        for index in 0..6 {
            let mut snapshot = snapshot.clone();
            let mut video = requested.clone();
            match index {
                0 => snapshot["recording_id"] = serde_json::to_value(RecordingId::new()).unwrap(),
                1 => {
                    snapshot["dataset_id"] =
                        serde_json::to_value(RecordingDatasetId::new()).unwrap()
                }
                2 => {
                    snapshot["sources"][0]["kind"] = "committed_layer".into();
                    snapshot["sources"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("part_sequence");
                }
                3 => video.entity_path = "/other-camera".into(),
                4 => video.timeline = "other-time".into(),
                _ => video.range.end -= 1,
            }
            assert!(check(snapshot, &video).is_err(), "case {index}");
        }
    }
}
