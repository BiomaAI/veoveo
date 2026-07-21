use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::catalog::{ModelConfig, ObservationConfig, PipelineConfig};
use crate::contract::{
    ConfidenceBasis, DecodePolicy, GroundingDetections, IndexRange, ObservationSampling,
    ReasonedEvent, ReasoningAnswer, ReasoningResults, ReasoningTask, RecordingVideoSelection,
    VideoTimelineKind,
};
use crate::grounding::grounded_track_ids;

pub const RUNNER_REQUEST_SCHEMA: &str = "veoveo.reason-runner-request/v1";
pub const RUNNER_RESPONSE_SCHEMA: &str = "veoveo.reason-runner-response/v1";
pub const REASONING_RESULTS_SCHEMA: &str = "veoveo.reason-results/v1";

pub const MAX_EVENT_LABEL_BYTES: usize = 256;
pub const MAX_EVENT_DESCRIPTION_BYTES: usize = 4_096;
pub const MAX_TRACK_CITATIONS_PER_EVENT: usize = 64;

#[derive(Clone, Debug)]
pub struct ReasonExecutor {
    runner: PathBuf,
    timeout: Duration,
    max_events: usize,
    max_answer_bytes: usize,
    max_response_bytes: u64,
}

pub struct ReasonAnalysisRequest<'a> {
    pub task_id: &'a str,
    pub input_mp4: &'a Path,
    pub decode_start_index: i64,
    pub input_width: u16,
    pub input_height: u16,
    pub timeline_kind: VideoTimelineKind,
    pub video: &'a RecordingVideoSelection,
    pub pipeline: &'a PipelineConfig,
    pub model: &'a ModelConfig,
    pub task: &'a ReasoningTask,
    pub sampling: ObservationSampling,
    pub decode: DecodePolicy,
    pub grounding: Option<&'a GroundingDetections>,
}

impl ReasonExecutor {
    pub fn new(
        runner: PathBuf,
        timeout: Duration,
        max_events: usize,
        max_answer_bytes: usize,
        max_response_bytes: u64,
    ) -> Result<Self> {
        ensure!(runner.is_absolute(), "reason runner path must be absolute");
        ensure!(
            timeout > Duration::ZERO,
            "reason runner timeout must be positive"
        );
        ensure!(max_events > 0, "max_events must be non-zero");
        ensure!(max_answer_bytes > 0, "max_answer_bytes must be non-zero");
        ensure!(
            max_response_bytes > 0,
            "max_response_bytes must be non-zero"
        );
        Ok(Self {
            runner,
            timeout,
            max_events,
            max_answer_bytes,
            max_response_bytes,
        })
    }

    pub fn readiness(&self) -> Result<()> {
        let metadata = std::fs::metadata(&self.runner)
            .with_context(|| format!("reading reason runner {}", self.runner.display()))?;
        ensure!(metadata.is_file(), "reason runner is not a file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            ensure!(
                metadata.permissions().mode() & 0o111 != 0,
                "reason runner is not executable"
            );
        }
        Ok(())
    }

    pub async fn analyze(&self, analysis: ReasonAnalysisRequest<'_>) -> Result<ReasoningResults> {
        let work = tempfile::Builder::new()
            .prefix("veoveo-reason-runner-")
            .tempdir()
            .context("creating reason runner workspace")?;
        let request_path = work.path().join("request.json");
        let response_path = work.path().join("response.json");
        let request = RunnerRequest {
            schema: RUNNER_REQUEST_SCHEMA.to_owned(),
            task_id: analysis.task_id.to_owned(),
            input_mp4: analysis.input_mp4.to_path_buf(),
            input_width: analysis.input_width,
            input_height: analysis.input_height,
            response_json: response_path.clone(),
            pipeline: RunnerPipeline {
                pipeline_id: analysis.pipeline.id.clone(),
                prompt_template_path: analysis.pipeline.prompt_template_path.clone(),
                prompt_revision: analysis.pipeline.prompt_revision.clone(),
                observation: analysis.pipeline.observation,
            },
            model: RunnerModel {
                model_id: analysis.model.id.clone(),
                model_path: analysis.model.model_path.clone(),
                format: analysis.model.format,
                model_digest: analysis.model.model_digest.clone(),
            },
            task: analysis.task.clone(),
            grounding: analysis.grounding.cloned(),
            requested_range: analysis.video.range,
            decode_start_index: analysis.decode_start_index,
            sampling: analysis.sampling,
            decode: analysis.decode,
            max_events: self.max_events,
            max_answer_bytes: self.max_answer_bytes,
            max_response_bytes: self.max_response_bytes,
        };
        tokio::fs::write(&request_path, serde_json::to_vec_pretty(&request)?)
            .await
            .context("writing reason runner request")?;
        let mut command = Command::new(&self.runner);
        command
            .arg("--request-json")
            .arg(&request_path)
            .arg("--response-json")
            .arg(&response_path)
            .kill_on_drop(true);
        let output = tokio::time::timeout(self.timeout, command.output())
            .await
            .context("reason runner timed out")?
            .with_context(|| format!("starting reason runner {}", self.runner.display()))?;
        ensure!(
            output.status.success(),
            "reason runner failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        ensure!(
            output.stdout.is_empty(),
            "reason runner must write only to its typed response file"
        );
        let response_metadata = tokio::fs::metadata(&response_path)
            .await
            .context("reading reason runner response metadata")?;
        ensure!(
            response_metadata.len() <= self.max_response_bytes,
            "reason runner response exceeds max_response_bytes ({})",
            self.max_response_bytes
        );
        let response_bytes = tokio::fs::read(&response_path)
            .await
            .context("reading reason runner response")?;
        let response: RunnerResponse =
            serde_json::from_slice(&response_bytes).context("parsing reason runner response")?;
        self.validate_response(
            analysis.task,
            analysis.video.range,
            analysis.grounding,
            &response,
        )?;
        Ok(ReasoningResults {
            schema: REASONING_RESULTS_SCHEMA.to_owned(),
            pipeline_id: analysis.pipeline.id.clone(),
            model_id: analysis.model.id.clone(),
            recording_uri: analysis.video.recording_uri.clone(),
            entity_path: analysis.video.entity_path.clone(),
            timeline: analysis.video.timeline.clone(),
            timeline_kind: analysis.timeline_kind,
            requested_range: analysis.video.range,
            task: analysis.task.clone(),
            answer: response.answer,
            observed_frames: response.observed_frames,
            elapsed_ms: response.elapsed_ms,
            prompt_revision: analysis.pipeline.prompt_revision.clone(),
            model_digest: analysis.model.model_digest.clone(),
            decode: analysis.decode,
            confidence_basis: ConfidenceBasis::ModelReported,
        })
    }

    fn validate_response(
        &self,
        task: &ReasoningTask,
        range: IndexRange,
        grounding: Option<&GroundingDetections>,
        response: &RunnerResponse,
    ) -> Result<()> {
        ensure!(
            response.schema == RUNNER_RESPONSE_SCHEMA,
            "unsupported reason runner response schema"
        );
        ensure!(
            response.answer.kind() == task.kind(),
            "reason runner answered `{}` for a `{}` task",
            response.answer.kind(),
            task.kind()
        );
        ensure!(
            response.observed_frames > 0,
            "reason runner reported zero observed frames"
        );
        match &response.answer {
            ReasoningAnswer::Description { text } | ReasoningAnswer::Answer { text } => {
                validate_answer_text(text, self.max_answer_bytes)?;
            }
            ReasoningAnswer::Events { events } => {
                ensure!(
                    events.len() <= self.max_events,
                    "reason runner returned too many events"
                );
                let cited_tracks = grounding.map(grounded_track_ids).unwrap_or_default();
                let mut prior_start = None;
                for event in events {
                    validate_event(event, range, grounding.is_some(), &cited_tracks)?;
                    if let Some(prior) = prior_start {
                        ensure!(
                            event.range.start >= prior,
                            "reason runner events are not ordered by start index"
                        );
                    }
                    prior_start = Some(event.range.start);
                }
            }
        }
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
    task: ReasoningTask,
    #[serde(skip_serializing_if = "Option::is_none")]
    grounding: Option<GroundingDetections>,
    requested_range: IndexRange,
    decode_start_index: i64,
    sampling: ObservationSampling,
    decode: DecodePolicy,
    max_events: usize,
    max_answer_bytes: usize,
    max_response_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct RunnerPipeline {
    pipeline_id: String,
    prompt_template_path: PathBuf,
    prompt_revision: String,
    observation: ObservationConfig,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct RunnerModel {
    model_id: String,
    model_path: PathBuf,
    format: crate::contract::ModelFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunnerResponse {
    schema: String,
    answer: ReasoningAnswer,
    observed_frames: u64,
    elapsed_ms: u64,
}

fn validate_answer_text(text: &str, max_answer_bytes: usize) -> Result<()> {
    ensure!(
        !text.trim().is_empty(),
        "reason runner returned an empty answer"
    );
    ensure!(
        text.len() <= max_answer_bytes,
        "reason runner answer exceeds {max_answer_bytes} bytes"
    );
    Ok(())
}

fn validate_event(
    event: &ReasonedEvent,
    range: IndexRange,
    has_grounding: bool,
    cited_tracks: &BTreeSet<u64>,
) -> Result<()> {
    ensure!(
        event.range.start <= event.range.end,
        "event range start exceeds its end"
    );
    ensure!(
        range.contains(event.range),
        "reason runner returned an event outside the requested range"
    );
    ensure!(
        !event.label.trim().is_empty() && event.label.len() <= MAX_EVENT_LABEL_BYTES,
        "event label is empty or too long"
    );
    ensure!(
        !event.description.trim().is_empty()
            && event.description.len() <= MAX_EVENT_DESCRIPTION_BYTES,
        "event description is empty or too long"
    );
    ensure!(
        event.track_ids.len() <= MAX_TRACK_CITATIONS_PER_EVENT,
        "event cites too many tracks"
    );
    if !event.track_ids.is_empty() {
        ensure!(
            has_grounding,
            "event cites track identities without grounding"
        );
        for track_id in &event.track_ids {
            ensure!(
                cited_tracks.contains(track_id),
                "event cites track {track_id} that the grounding does not contain"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ObservationConfig;
    use crate::contract::ModelFormat;

    fn selection() -> RecordingVideoSelection {
        RecordingVideoSelection {
            recording_uri: "recording://recordings/01983da0-0000-7000-8000-000000000000".to_owned(),
            entity_path: "/camera/front".to_owned(),
            timeline: "sensor_time".to_owned(),
            range: IndexRange {
                start: 120,
                end: 140,
            },
        }
    }

    fn pipeline() -> PipelineConfig {
        PipelineConfig {
            id: "video-reasoning".to_owned(),
            title: "Video reasoning".to_owned(),
            description: String::new(),
            operation: crate::contract::PipelineOperation::VideoReasoning,
            model_id: "world-model".to_owned(),
            prompt_template_path: "/etc/veoveo/reason/prompt-template.txt".into(),
            prompt_revision: "v1".to_owned(),
            observation: ObservationConfig {
                width: 640,
                height: 360,
            },
        }
    }

    fn model() -> ModelConfig {
        ModelConfig {
            id: "world-model".to_owned(),
            title: "World model".to_owned(),
            description: String::new(),
            format: ModelFormat::LocalCheckpoint,
            model_path: "/models/world-model.engine".into(),
            model_digest: Some("sha256:test".to_owned()),
        }
    }

    fn executor(runner: PathBuf) -> ReasonExecutor {
        ReasonExecutor::new(runner, Duration::from_secs(5), 100, 10_000, 1_000_000).unwrap()
    }

    #[test]
    fn mismatched_answer_kind_is_rejected() {
        let executor = executor("/usr/local/bin/reason-runner".into());
        let task = ReasoningTask::AnswerQuestion {
            question: "what happened?".to_owned(),
        };
        let response = RunnerResponse {
            schema: RUNNER_RESPONSE_SCHEMA.to_owned(),
            answer: ReasoningAnswer::Events { events: Vec::new() },
            observed_frames: 4,
            elapsed_ms: 10,
        };
        let error = executor
            .validate_response(&task, IndexRange { start: 0, end: 10 }, None, &response)
            .unwrap_err();
        assert!(error.to_string().contains("answered"));
    }

    #[test]
    fn ungrounded_track_citation_is_rejected() {
        let executor = executor("/usr/local/bin/reason-runner".into());
        let task = ReasoningTask::DetectEvents {
            prompt: "vehicles".to_owned(),
        };
        let response = RunnerResponse {
            schema: RUNNER_RESPONSE_SCHEMA.to_owned(),
            answer: ReasoningAnswer::Events {
                events: vec![ReasonedEvent {
                    range: IndexRange { start: 2, end: 4 },
                    label: "vehicle passes".to_owned(),
                    description: "a vehicle crosses the frame".to_owned(),
                    track_ids: vec![7],
                }],
            },
            observed_frames: 4,
            elapsed_ms: 10,
        };
        let error = executor
            .validate_response(&task, IndexRange { start: 0, end: 10 }, None, &response)
            .unwrap_err();
        assert!(error.to_string().contains("without grounding"));
    }

    #[test]
    fn out_of_range_event_is_rejected() {
        let executor = executor("/usr/local/bin/reason-runner".into());
        let task = ReasoningTask::DetectEvents {
            prompt: "vehicles".to_owned(),
        };
        let response = RunnerResponse {
            schema: RUNNER_RESPONSE_SCHEMA.to_owned(),
            answer: ReasoningAnswer::Events {
                events: vec![ReasonedEvent {
                    range: IndexRange { start: 2, end: 40 },
                    label: "vehicle passes".to_owned(),
                    description: "a vehicle crosses the frame".to_owned(),
                    track_ids: Vec::new(),
                }],
            },
            observed_frames: 4,
            elapsed_ms: 10,
        };
        let error = executor
            .validate_response(&task, IndexRange { start: 0, end: 10 }, None, &response)
            .unwrap_err();
        assert!(error.to_string().contains("outside the requested range"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn typed_runner_request_preserves_rerun_index_origin() {
        use std::os::unix::fs::PermissionsExt as _;

        let workspace = tempfile::tempdir().unwrap();
        let runner = workspace.path().join("runner.sh");
        let captured = workspace.path().join("captured-request.json");
        let script = format!(
            "#!/bin/sh\nset -eu\ntest \"$1\" = --request-json\ntest \"$3\" = --response-json\ncp \"$2\" '{}'\nprintf '%s' '{{\"schema\":\"{}\",\"answer\":{{\"kind\":\"description\",\"text\":\"a quiet road\"}},\"observed_frames\":3,\"elapsed_ms\":2}}' > \"$4\"\n",
            captured.display(),
            RUNNER_RESPONSE_SCHEMA,
        );
        std::fs::write(&runner, script).unwrap();
        let mut permissions = std::fs::metadata(&runner).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&runner, permissions).unwrap();
        let input = workspace.path().join("input.mp4");
        std::fs::write(&input, []).unwrap();
        let executor = executor(runner);
        let video = selection();
        let task = ReasoningTask::DescribeSegment { prompt: None };
        let results = executor
            .analyze(ReasonAnalysisRequest {
                task_id: "01983da0-0000-7000-8000-000000000001",
                input_mp4: &input,
                decode_start_index: 100,
                input_width: 1920,
                input_height: 1080,
                timeline_kind: VideoTimelineKind::DurationNanoseconds,
                video: &video,
                pipeline: &pipeline(),
                model: &model(),
                task: &task,
                sampling: ObservationSampling::default(),
                decode: DecodePolicy::Greedy,
                grounding: None,
            })
            .await
            .unwrap();
        assert_eq!(results.observed_frames, 3);
        assert_eq!(results.prompt_revision, "v1");
        assert_eq!(results.model_digest.as_deref(), Some("sha256:test"));
        assert_eq!(results.confidence_basis, ConfidenceBasis::ModelReported);
        let request: serde_json::Value =
            serde_json::from_slice(&std::fs::read(captured).unwrap()).unwrap();
        assert_eq!(request["decode_start_index"], 100);
        assert_eq!(request["requested_range"]["start"], 120);
        assert_eq!(request["pipeline"]["observation"]["width"], 640);
        assert_eq!(request["decode"]["mode"], "greedy");
        assert_eq!(request["max_response_bytes"], 1_000_000);
    }
}
