use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::catalog::{
    GStreamerGraphConfig, ModelConfig, PerceptionProfile, PipelineConfig, TrackerConfig,
};
use crate::contract::{
    AnalysisResults, BoundingBox2D, Detection, FrameDetections, IndexRange,
    RecordingVideoSelection, SamplingPolicy, VideoTimelineKind,
};

pub const RUNNER_REQUEST_SCHEMA: &str = "veoveo.stream-recording-runner-request/v1";
pub const RUNNER_RESPONSE_SCHEMA: &str = "veoveo.stream-recording-runner-response/v1";
pub const STREAM_RESULTS_SCHEMA: &str = "veoveo.stream-results/v1";

#[derive(Clone, Debug)]
pub struct StreamExecutor {
    runner: PathBuf,
    timeout: Duration,
    max_frames: usize,
    max_detections_per_frame: usize,
    max_response_bytes: u64,
}

pub struct StreamAnalysisRequest<'a> {
    pub task_id: &'a str,
    pub input_mp4: &'a Path,
    pub decode_start_index: i64,
    pub input_width: u16,
    pub input_height: u16,
    pub timeline_kind: VideoTimelineKind,
    pub video: &'a RecordingVideoSelection,
    pub pipeline: &'a PipelineConfig,
    pub model: &'a ModelConfig,
    pub sampling: SamplingPolicy,
}

impl StreamExecutor {
    pub fn new(
        runner: PathBuf,
        timeout: Duration,
        max_frames: usize,
        max_detections_per_frame: usize,
        max_response_bytes: u64,
    ) -> Result<Self> {
        ensure!(runner.is_absolute(), "stream runner path must be absolute");
        ensure!(
            timeout > Duration::ZERO,
            "stream runner timeout must be positive"
        );
        ensure!(max_frames > 0, "max_frames must be non-zero");
        ensure!(
            max_detections_per_frame > 0,
            "max_detections_per_frame must be non-zero"
        );
        ensure!(
            max_response_bytes > 0,
            "max_response_bytes must be non-zero"
        );
        Ok(Self {
            runner,
            timeout,
            max_frames,
            max_detections_per_frame,
            max_response_bytes,
        })
    }

    pub fn readiness(&self) -> Result<()> {
        let metadata = std::fs::metadata(&self.runner)
            .with_context(|| format!("reading stream runner {}", self.runner.display()))?;
        ensure!(metadata.is_file(), "stream runner is not a file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            ensure!(
                metadata.permissions().mode() & 0o111 != 0,
                "stream runner is not executable"
            );
        }
        Ok(())
    }

    pub async fn analyze(&self, analysis: StreamAnalysisRequest<'_>) -> Result<AnalysisResults> {
        let work = tempfile::Builder::new()
            .prefix("veoveo-stream-recording-runner-")
            .tempdir()
            .context("creating stream runner workspace")?;
        let request_path = work.path().join("request.json");
        let response_path = work.path().join("response.json");
        let graph = analysis
            .pipeline
            .recording_replay
            .as_ref()
            .context("selected pipeline does not admit recording replay")?;
        let profile = analysis
            .pipeline
            .profile
            .perception()
            .context("recording replay currently requires a perception profile")?;
        let request = RunnerRequest {
            schema: RUNNER_REQUEST_SCHEMA.to_owned(),
            task_id: analysis.task_id.to_owned(),
            input_mp4: analysis.input_mp4.to_path_buf(),
            input_width: analysis.input_width,
            input_height: analysis.input_height,
            response_json: response_path.clone(),
            pipeline: RunnerPipeline {
                pipeline_id: analysis.pipeline.id.clone(),
                graph: graph.clone(),
                profile: RunnerPerceptionProfile::from(profile),
            },
            model: RunnerModel {
                model_id: analysis.model.id.clone(),
                model_path: analysis.model.model_path.clone(),
                format: analysis.model.format,
            },
            requested_range: analysis.video.range,
            decode_start_index: analysis.decode_start_index,
            sampling: analysis.sampling,
            max_output_frames: self.max_frames,
            max_detections_per_frame: self.max_detections_per_frame,
            max_response_bytes: self.max_response_bytes,
        };
        tokio::fs::write(&request_path, serde_json::to_vec_pretty(&request)?)
            .await
            .context("writing stream runner request")?;
        let mut command = Command::new(&self.runner);
        command
            .arg("--request-json")
            .arg(&request_path)
            .arg("--response-json")
            .arg(&response_path)
            .kill_on_drop(true);
        let output = tokio::time::timeout(self.timeout, command.output())
            .await
            .context("DeepStream runner timed out")?
            .with_context(|| format!("starting DeepStream runner {}", self.runner.display()))?;
        ensure!(
            output.status.success(),
            "DeepStream runner failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        ensure!(
            output.stdout.is_empty(),
            "DeepStream runner must write only to its typed response file"
        );
        let response_metadata = tokio::fs::metadata(&response_path)
            .await
            .context("reading DeepStream runner response metadata")?;
        ensure!(
            response_metadata.len() <= self.max_response_bytes,
            "DeepStream runner response exceeds max_response_bytes ({})",
            self.max_response_bytes
        );
        let response_bytes = tokio::fs::read(&response_path)
            .await
            .context("reading DeepStream runner response")?;
        let response: RunnerResponse = serde_json::from_slice(&response_bytes)
            .context("parsing DeepStream runner response")?;
        self.validate_response(
            analysis.video.range,
            analysis.input_width,
            analysis.input_height,
            &response,
        )?;
        Ok(AnalysisResults {
            schema: STREAM_RESULTS_SCHEMA.to_owned(),
            pipeline_id: analysis.pipeline.id.clone(),
            model_id: analysis.model.id.clone(),
            recording_uri: analysis.video.recording_uri.clone(),
            entity_path: analysis.video.entity_path.clone(),
            timeline: analysis.video.timeline.clone(),
            timeline_kind: analysis.timeline_kind,
            requested_range: analysis.video.range,
            frames: response.frames,
            processed_frames: response.processed_frames,
            elapsed_ms: response.elapsed_ms,
        })
    }

    fn validate_response(
        &self,
        range: IndexRange,
        input_width: u16,
        input_height: u16,
        response: &RunnerResponse,
    ) -> Result<()> {
        ensure!(
            response.schema == RUNNER_RESPONSE_SCHEMA,
            "unsupported DeepStream runner response schema"
        );
        ensure!(
            response.frames.len() <= self.max_frames,
            "DeepStream runner returned too many frames"
        );
        let mut prior_index = None;
        for frame in &response.frames {
            ensure!(
                frame.index >= range.start && frame.index <= range.end,
                "DeepStream runner returned a frame outside the requested range"
            );
            if let Some(prior) = prior_index {
                ensure!(
                    frame.index > prior,
                    "DeepStream runner frames are not strictly ordered"
                );
            }
            prior_index = Some(frame.index);
            validate_frame(
                frame,
                input_width,
                input_height,
                self.max_detections_per_frame,
            )?;
        }
        ensure!(
            response.processed_frames >= response.frames.len() as u64,
            "processed_frames is smaller than returned frame count"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct RunnerRequest {
    schema: String,
    task_id: String,
    input_mp4: PathBuf,
    input_width: u16,
    input_height: u16,
    response_json: PathBuf,
    pipeline: RunnerPipeline,
    model: RunnerModel,
    requested_range: IndexRange,
    decode_start_index: i64,
    sampling: SamplingPolicy,
    max_output_frames: usize,
    max_detections_per_frame: usize,
    max_response_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct RunnerPipeline {
    pipeline_id: String,
    graph: GStreamerGraphConfig,
    profile: RunnerPerceptionProfile,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
struct RunnerPerceptionProfile {
    operation: crate::contract::PerceptionOperation,
    inference_config_path: PathBuf,
    tracker: Option<RunnerTracker>,
}

impl From<PerceptionProfile<'_>> for RunnerPerceptionProfile {
    fn from(value: PerceptionProfile<'_>) -> Self {
        Self {
            operation: value.operation,
            inference_config_path: value.inference_config_path.to_path_buf(),
            tracker: value.tracker.map(RunnerTracker::from),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct RunnerTracker {
    config_path: PathBuf,
    width: u32,
    height: u32,
}

impl From<&TrackerConfig> for RunnerTracker {
    fn from(value: &TrackerConfig) -> Self {
        Self {
            config_path: value.config_path.clone(),
            width: value.width,
            height: value.height,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct RunnerModel {
    model_id: String,
    model_path: PathBuf,
    format: crate::contract::ModelFormat,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunnerResponse {
    schema: String,
    frames: Vec<FrameDetections>,
    processed_frames: u64,
    elapsed_ms: u64,
}

pub fn validate_frame(
    frame: &FrameDetections,
    input_width: u16,
    input_height: u16,
    max_detections: usize,
) -> Result<()> {
    ensure!(
        frame.detections.len() <= max_detections,
        "DeepStream runner returned too many detections for one frame"
    );
    for detection in &frame.detections {
        validate_detection(detection, input_width, input_height)?;
    }
    Ok(())
}

fn validate_detection(detection: &Detection, input_width: u16, input_height: u16) -> Result<()> {
    ensure!(
        u16::try_from(detection.class_id).is_ok(),
        "detection class_id exceeds the Rerun annotation limit"
    );
    ensure!(
        !detection.label.trim().is_empty() && detection.label.len() <= 256,
        "detection label is empty or too long"
    );
    for (name, confidence) in [
        ("detection confidence", detection.confidence),
        ("tracker confidence", detection.tracker_confidence),
    ] {
        if let Some(confidence) = confidence {
            ensure!(
                confidence.is_finite() && (0.0..=1.0).contains(&confidence),
                "{name} must be within 0..=1"
            );
        }
    }
    let BoundingBox2D {
        x,
        y,
        width,
        height,
    } = detection.bounds;
    ensure!(
        [x, y, width, height].into_iter().all(f32::is_finite),
        "detection bounds must be finite"
    );
    ensure!(
        width > 0.0 && height > 0.0,
        "detection bounds must have positive size"
    );
    ensure!(
        x >= 0.0
            && y >= 0.0
            && x + width <= f32::from(input_width)
            && y + height <= f32::from(input_height),
        "detection bounds are outside the source frame"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_detection_is_rejected() {
        let detection = Detection {
            class_id: 1,
            label: "person".to_owned(),
            confidence: Some(1.5),
            tracker_confidence: None,
            bounds: BoundingBox2D {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            track_id: None,
        };
        assert!(validate_detection(&detection, 32, 32).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn typed_runner_request_preserves_rerun_index_origin() {
        use std::os::unix::fs::PermissionsExt as _;

        use crate::catalog::{GStreamerGraphConfig, PipelineProfileConfig};
        use crate::contract::{ModelFormat, PerceptionOperation, VideoTimelineKind};

        let workspace = tempfile::tempdir().unwrap();
        let runner = workspace.path().join("runner.sh");
        let captured = workspace.path().join("captured-request.json");
        let script = format!(
            "#!/bin/sh\nset -eu\ntest \"$1\" = --request-json\ntest \"$3\" = --response-json\ncp \"$2\" '{}'\nprintf '%s' '{{\"schema\":\"{}\",\"frames\":[{{\"index\":120,\"detections\":[]}}],\"processed_frames\":1,\"elapsed_ms\":2}}' > \"$4\"\n",
            captured.display(),
            RUNNER_RESPONSE_SCHEMA,
        );
        std::fs::write(&runner, script).unwrap();
        let mut permissions = std::fs::metadata(&runner).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&runner, permissions).unwrap();
        let input = workspace.path().join("input.mp4");
        std::fs::write(&input, []).unwrap();
        let executor =
            StreamExecutor::new(runner, Duration::from_secs(5), 10, 10, 1_000_000).unwrap();
        let video = RecordingVideoSelection {
            recording_uri: "recording://recordings/01983da0-0000-7000-8000-000000000000".to_owned(),
            entity_path: "/camera/front".to_owned(),
            timeline: "sensor_time".to_owned(),
            range: IndexRange {
                start: 120,
                end: 140,
            },
        };
        let pipeline = PipelineConfig {
            id: "detect".to_owned(),
            title: "Detect".to_owned(),
            description: String::new(),
            profile: PipelineProfileConfig::Perception {
                operation: PerceptionOperation::ObjectDetection,
                model_id: "detector".to_owned(),
                inference_config_path: "/etc/stream/detect.txt".into(),
                tracker: None,
            },
            recording_replay: Some(GStreamerGraphConfig {
                launch: "filesrc name=source ! fakesink name=results".to_owned(),
                source_element: Some("source".to_owned()),
                stream_muxer_element: Some("mux".to_owned()),
                inference_element: Some("infer".to_owned()),
                tracker_element: None,
                results_element: Some("results".to_owned()),
                encoded_output_element: None,
            }),
            live: None,
        };
        let model = ModelConfig {
            id: "detector".to_owned(),
            title: "Detector".to_owned(),
            description: String::new(),
            format: ModelFormat::TensorRtEngine,
            model_path: "/models/detector.engine".into(),
        };
        let results = executor
            .analyze(StreamAnalysisRequest {
                task_id: "01983da0-0000-7000-8000-000000000001",
                input_mp4: &input,
                decode_start_index: 100,
                input_width: 32,
                input_height: 32,
                timeline_kind: VideoTimelineKind::DurationNanoseconds,
                video: &video,
                pipeline: &pipeline,
                model: &model,
                sampling: SamplingPolicy::EveryFrame,
            })
            .await
            .unwrap();
        assert_eq!(results.frames[0].index, 120);
        let request: serde_json::Value =
            serde_json::from_slice(&std::fs::read(captured).unwrap()).unwrap();
        assert_eq!(request["decode_start_index"], 100);
        assert_eq!(request["input_width"], 32);
        assert_eq!(request["input_height"], 32);
        assert_eq!(request["requested_range"]["start"], 120);
        assert_eq!(request["max_response_bytes"], 1_000_000);
    }
}
