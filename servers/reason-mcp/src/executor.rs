use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::catalog::{EngineConfig, ModelConfig, ObservationConfig, PipelineConfig};
use crate::contract::{
    AnalysisId, ConfidenceBasis, DecodePolicy, GroundingDetections, IndexRange, ModelId,
    ObservationSampling, PipelineId, ReasonedEvent, ReasoningAnswer, ReasoningResults,
    ReasoningTask, RecordingSourceSnapshot, RecordingVideoSelection, VideoTimelineKind,
};
use crate::grounding::grounded_track_ids;

pub const RUNNER_REQUEST_SCHEMA: &str = "veoveo.ai/reason-runner-request/v4";
pub const RUNNER_RESPONSE_SCHEMA: &str = "veoveo.ai/reason-runner-response/v2";
use crate::contract::REASONING_RESULTS_SCHEMA;

use crate::contract::{
    MAX_EVENT_DESCRIPTION_BYTES, MAX_EVENT_LABEL_BYTES, MAX_TRACK_CITATIONS_PER_EVENT,
};

#[derive(Clone, Debug)]
pub struct ReasonExecutor {
    runner: PathBuf,
    timeout: Duration,
    max_events: usize,
    max_answer_bytes: usize,
    max_response_bytes: u64,
}

pub struct ReasonAnalysisRequest<'a> {
    pub task_id: AnalysisId,
    pub input_mp4: &'a Path,
    pub decode_start_index: i64,
    pub input_width: u16,
    pub input_height: u16,
    pub timeline_kind: VideoTimelineKind,
    pub video: &'a RecordingVideoSelection,
    pub source_snapshot: &'a RecordingSourceSnapshot,
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
        ensure!(
            analysis.source_snapshot.recording_id == analysis.video.recording_uri.id(),
            "source snapshot does not match the selected recording"
        );
        let work = tempfile::Builder::new()
            .prefix("veoveo-reason-runner-")
            .tempdir()
            .context("creating reason runner workspace")?;
        let request_path = work.path().join("request.json");
        let response_path = work.path().join("response.json");
        let request = RunnerRequest {
            schema: RUNNER_REQUEST_SCHEMA.to_owned(),
            task_id: analysis.task_id,
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
                engine: analysis.model.engine,
            },
            task: analysis.task.clone(),
            grounding: analysis.grounding.cloned(),
            requested_range: analysis.video.range,
            decode_start_index: analysis.decode_start_index,
            sampling: analysis.sampling,
            decode: analysis.decode,
            max_events: u64::try_from(self.max_events)
                .context("max_events exceeds the runner wire limit")?,
            max_answer_bytes: u64::try_from(self.max_answer_bytes)
                .context("max_answer_bytes exceeds the runner wire limit")?,
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
        crate::contract::ReasoningResultsBuilder {
            schema: REASONING_RESULTS_SCHEMA.to_owned(),
            pipeline_id: analysis.pipeline.id.clone(),
            model_id: analysis.model.id.clone(),
            recording_uri: analysis.video.recording_uri.clone(),
            entity_path: analysis.video.entity_path.clone(),
            timeline: analysis.video.timeline.clone(),
            timeline_kind: analysis.timeline_kind,
            requested_range: analysis.video.range,
            source_snapshot: analysis.source_snapshot.clone(),
            task: analysis.task.clone(),
            answer: response.answer,
            observed_frames: response.observed_frames,
            elapsed_ms: response.elapsed_ms,
            prompt_revision: analysis.pipeline.prompt_revision.clone(),
            model_digest: analysis.model.model_digest.clone(),
            decode: analysis.decode,
            confidence_basis: ConfidenceBasis::ModelReported,
        }
        .build()
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

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RunnerRequest {
    #[schemars(schema_with = "runner_request_format_schema")]
    schema: String,
    task_id: AnalysisId,
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
    #[schemars(range(min = 1))]
    max_events: u64,
    #[schemars(range(min = 1))]
    max_answer_bytes: u64,
    #[schemars(range(min = 1))]
    max_response_bytes: u64,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RunnerPipeline {
    pipeline_id: PipelineId,
    prompt_template_path: PathBuf,
    prompt_revision: String,
    observation: ObservationConfig,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RunnerModel {
    model_id: ModelId,
    model_path: PathBuf,
    format: crate::contract::ModelFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_digest: Option<String>,
    engine: EngineConfig,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RunnerResponse {
    #[schemars(schema_with = "runner_response_format_schema")]
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
        veoveo_recording_video::contract::RecordingVideoSelectionBuilder {
            recording_uri: "recording://recordings/01983da0-0000-7000-8000-000000000000"
                .parse()
                .unwrap(),
            entity_path: "/camera/front".to_owned(),
            timeline: "sensor_time".to_owned(),
            range: IndexRange::new(120, 140).unwrap(),
        }
        .build()
        .unwrap()
    }

    fn pipeline() -> PipelineConfig {
        PipelineConfig {
            id: "video-reasoning".parse().unwrap(),
            title: "Video reasoning".to_owned(),
            description: String::new(),
            operation: crate::contract::PipelineOperation::VideoReasoning,
            model_id: "world-model".parse().unwrap(),
            prompt_template_path: "/etc/veoveo/reason/prompt-template.txt".into(),
            prompt_revision: "v1".to_owned(),
            observation: ObservationConfig {
                width: 640,
                height: 360,
                maximum_frames: 6,
            },
        }
    }

    fn source_snapshot() -> RecordingSourceSnapshot {
        veoveo_recording_video::contract::RecordingSourceSnapshotBuilder {
            recording_id: "01983da0-0000-7000-8000-000000000000".parse().unwrap(),
            dataset_id: "01983da0-0000-7000-8000-000000000010".parse().unwrap(),
            captured_at: chrono::Utc::now(),
            sources: vec![veoveo_recording_video::contract::RecordingSourceIdentityBuilder {
                layer_id: "01983da0-0000-7000-8000-000000000020".parse().unwrap(),
                layer_name: "camera".into(),
                layer_ordinal: Some(0),
                kind: veoveo_recording_video::contract::RecordingSourceIdentityKind::CommittedLayer,
                part_sequence: None,
                byte_len: 100.try_into().unwrap(),
                sha256: veoveo_types::Sha256Digest::from_bytes([0xaa; 32]),
            }.build().unwrap()],
        }
        .build()
        .unwrap()
    }

    fn model() -> ModelConfig {
        ModelConfig {
            id: "world-model".parse().unwrap(),
            title: "World model".to_owned(),
            description: String::new(),
            format: ModelFormat::LocalCheckpoint,
            model_path: "/models/world-model.engine".into(),
            model_digest: Some("sha256:test".to_owned()),
            engine: EngineConfig::Vllm {
                gpu_memory_utilization: 0.7,
                max_model_len: 8_192,
            },
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
            .validate_response(&task, IndexRange::new(0, 10).unwrap(), None, &response)
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
                    range: IndexRange::new(2, 4).unwrap(),
                    label: "vehicle passes".to_owned(),
                    description: "a vehicle crosses the frame".to_owned(),
                    track_ids: vec![7],
                }],
            },
            observed_frames: 4,
            elapsed_ms: 10,
        };
        let error = executor
            .validate_response(&task, IndexRange::new(0, 10).unwrap(), None, &response)
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
                    range: IndexRange::new(2, 40).unwrap(),
                    label: "vehicle passes".to_owned(),
                    description: "a vehicle crosses the frame".to_owned(),
                    track_ids: Vec::new(),
                }],
            },
            observed_frames: 4,
            elapsed_ms: 10,
        };
        let error = executor
            .validate_response(&task, IndexRange::new(0, 10).unwrap(), None, &response)
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
            "#!/bin/sh\nset -eu\ntest \"$1\" = --request-json\ntest \"$3\" = --response-json\ncp \"$2\" '{}'\nprintf '%s' '{{\"schema\":\"{}\",\"answer\":{{\"kind\":\"description\",\"text\":\"a quiet road\"}},\"observedFrames\":3,\"elapsedMs\":2}}' > \"$4\"\n",
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
        let source_snapshot = source_snapshot();
        let task = ReasoningTask::DescribeSegment { prompt: None };
        let results = executor
            .analyze(ReasonAnalysisRequest {
                task_id: "01983da0-0000-7000-8000-000000000001".parse().unwrap(),
                input_mp4: &input,
                decode_start_index: 100,
                input_width: 1920,
                input_height: 1080,
                timeline_kind: VideoTimelineKind::DurationNanoseconds,
                video: &video,
                source_snapshot: &source_snapshot,
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
        assert_eq!(results.source_snapshot, source_snapshot);
        assert_eq!(results.prompt_revision, "v1");
        assert_eq!(results.model_digest.as_deref(), Some("sha256:test"));
        assert_eq!(results.confidence_basis, ConfidenceBasis::ModelReported);
        let request: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&captured).unwrap()).unwrap();
        assert_eq!(request["decodeStartIndex"], 100);
        assert_eq!(request["requestedRange"]["start"], 120);
        assert_eq!(request["pipeline"]["observation"]["width"], 640);
        assert_eq!(request["pipeline"]["observation"]["maximumFrames"], 6);
        assert_eq!(request["model"]["engine"]["kind"], "vllm");
        assert_eq!(request["model"]["engine"]["gpuMemoryUtilization"], 0.7);
        assert_eq!(request["model"]["engine"]["maxModelLen"], 8_192);
        assert_eq!(request["decode"]["mode"], "greedy");
        assert_eq!(request["maxResponseBytes"], 1_000_000);
        std::fs::remove_file(&captured).unwrap();
        let mut wrong_video = video.clone().into_builder();
        wrong_video.recording_uri = "recording://recordings/01983da0-0000-7000-8000-000000000099"
            .parse()
            .unwrap();
        let wrong_video = wrong_video.build().unwrap();
        let error = executor
            .analyze(ReasonAnalysisRequest {
                task_id: "01983da0-0000-7000-8000-000000000001".parse().unwrap(),
                input_mp4: &input,
                decode_start_index: 100,
                input_width: 1920,
                input_height: 1080,
                timeline_kind: VideoTimelineKind::DurationNanoseconds,
                video: &wrong_video,
                source_snapshot: &source_snapshot,
                pipeline: &pipeline(),
                model: &model(),
                task: &task,
                sampling: ObservationSampling::default(),
                decode: DecodePolicy::Greedy,
                grounding: None,
            })
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "source snapshot does not match the selected recording"
        );
        assert!(
            !captured.exists(),
            "mismatched source must not dispatch the runner"
        );
    }
    #[test]
    fn private_protocol_schema_snapshot() {
        let snapshot = serde_json::json!({
            "request": schemars::generate::SchemaSettings::draft2020_12()
                .for_serialize().into_generator().into_root_schema_for::<RunnerRequest>(),
            "response": schemars::generate::SchemaSettings::draft2020_12()
                .for_deserialize().into_generator().into_root_schema_for::<RunnerResponse>(),
        });
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/private-protocol.schema.json");
        if std::env::var_os("UPDATE_PRIVATE_PROTOCOL_SCHEMAS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                serde_json::to_string_pretty(&snapshot).unwrap() + "\n",
            )
            .unwrap();
        }
        let maintained: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&path)
                .expect("generate the maintained private protocol schema snapshot"),
        )
        .unwrap();
        assert_eq!(
            snapshot, maintained,
            "private protocol schema snapshot drift"
        );
    }
    #[test]
    fn runner_response_rejects_nested_additions_and_integer_overflow() {
        let response = serde_json::json!({
            "schema": RUNNER_RESPONSE_SCHEMA,
            "answer": {"kind": "events", "events": [{
                "range": {"start": 0, "end": 1}, "label": "event",
                "description": "observed", "trackIds": [1]
            }]}, "observedFrames": 1, "elapsedMs": 0
        });
        assert!(serde_json::from_value::<RunnerResponse>(response.clone()).is_ok());
        for pointer in ["", "/answer", "/answer/events/0", "/answer/events/0/range"] {
            let mut changed = response.clone();
            changed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), true.into());
            assert!(
                serde_json::from_value::<RunnerResponse>(changed).is_err(),
                "{pointer}"
            );
        }
        for pointer in [
            "/observedFrames",
            "/elapsedMs",
            "/answer/events/0/trackIds/0",
        ] {
            let mut changed = response.clone();
            *changed.pointer_mut(pointer).unwrap() = (-1).into();
            assert!(
                serde_json::from_value::<RunnerResponse>(changed).is_err(),
                "{pointer}"
            );
        }
        let mut changed = response;
        changed["answer"]["events"][0]["range"]["end"] = serde_json::json!(u64::MAX);
        assert!(serde_json::from_value::<RunnerResponse>(changed).is_err());
    }
}

fn runner_request_format_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    veoveo_types::scalar_schema(
        schemars::json_schema!({"type":"string", "const": RUNNER_REQUEST_SCHEMA}),
        veoveo_types::ScalarNaming::builtin(veoveo_types::ScalarGrammar::FormatTag),
    )
    .expect("declared Reason format naming profile")
}
fn runner_response_format_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    veoveo_types::scalar_schema(
        schemars::json_schema!({"type":"string", "const": RUNNER_RESPONSE_SCHEMA}),
        veoveo_types::ScalarNaming::builtin(veoveo_types::ScalarGrammar::FormatTag),
    )
    .expect("declared Reason format naming profile")
}
