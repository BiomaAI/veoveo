use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::ArtifactMetadata;

pub use veoveo_recording_video::{IndexRange, RecordingVideoSelection, VideoTimelineKind};

pub const MAX_PROMPT_BYTES: usize = 8_192;
pub const MAX_OBSERVATION_FRAMES: u32 = 1_024;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeRecordingRequest {
    pub video: RecordingVideoSelection,
    pub pipeline_id: String,
    pub task: ReasoningTask,
    #[serde(default)]
    pub sampling: ObservationSampling,
    #[serde(default)]
    pub decode: DecodePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grounding: Option<GroundingReference>,
    #[serde(default)]
    pub include_source_clip: bool,
}

/// One typed reasoning task over the selected video range.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReasoningTask {
    /// Describe what happens in the selected range.
    DescribeSegment {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    /// Detect events matching the prompt inside the selected range.
    DetectEvents { prompt: String },
    /// Answer one question about the selected range.
    AnswerQuestion { question: String },
}

impl ReasoningTask {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::DescribeSegment { .. } => "describe_segment",
            Self::DetectEvents { .. } => "detect_events",
            Self::AnswerQuestion { .. } => "answer_question",
        }
    }
}

/// How many decoded frames the model observes across the requested range.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObservationSampling {
    pub max_frames: u32,
}

impl Default for ObservationSampling {
    fn default() -> Self {
        Self { max_frames: 32 }
    }
}

/// Decode parameters. Greedy decoding is the deterministic default; sampled
/// decoding is opt-in and its parameters are recorded in the result.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecodePolicy {
    #[default]
    Greedy,
    Sampled {
        temperature: f32,
        top_p: f32,
        seed: u64,
    },
}

/// Reference to the governed results artifact of a completed perception
/// analysis over the same recording.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroundingReference {
    /// `perception://artifact/{uuidv7}` identity of a typed perception
    /// results artifact, exactly as a completed perception analysis
    /// presents it.
    pub results_artifact_uri: String,
}

/// Bounded typed subset of perception detections embedded in the durable
/// request at submission time.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroundingDetections {
    pub schema: String,
    pub source_artifact_uri: String,
    pub frames: Vec<GroundingFrame>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroundingFrame {
    pub index: i64,
    pub detections: Vec<GroundingDetection>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroundingDetection {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_id: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReasoningAnswer {
    Description { text: String },
    Events { events: Vec<ReasonedEvent> },
    Answer { text: String },
}

impl ReasoningAnswer {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Description { .. } => "describe_segment",
            Self::Events { .. } => "detect_events",
            Self::Answer { .. } => "answer_question",
        }
    }

    pub fn event_count(&self) -> u64 {
        match self {
            Self::Events { events } => events.len() as u64,
            Self::Description { .. } | Self::Answer { .. } => 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReasonedEvent {
    /// Inclusive source-timeline index range of the event.
    pub range: IndexRange,
    pub label: String,
    pub description: String,
    /// Perception track identities cited from the request's grounding.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub track_ids: Vec<u64>,
}

/// Confidence provenance of a reasoning result. Reasoning output is
/// model-reported and never calibrated detector output.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBasis {
    ModelReported,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReasoningResults {
    pub schema: String,
    pub pipeline_id: String,
    pub model_id: String,
    pub recording_uri: String,
    pub entity_path: String,
    pub timeline: String,
    pub timeline_kind: VideoTimelineKind,
    pub requested_range: IndexRange,
    pub task: ReasoningTask,
    pub answer: ReasoningAnswer,
    pub observed_frames: u64,
    pub elapsed_ms: u64,
    pub prompt_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_digest: Option<String>,
    pub decode: DecodePolicy,
    pub confidence_basis: ConfidenceBasis,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct AnalyzeRecordingOutput {
    pub analysis_uri: String,
    pub results_uri: String,
    pub pipeline_uri: String,
    pub model_uri: String,
    pub summary: ReasoningSummary,
    pub results_artifact: ArtifactMetadata,
    pub annotations_artifact: ArtifactMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_clip_artifact: Option<ArtifactMetadata>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReasoningSummary {
    pub observed_frames: u64,
    pub event_count: u64,
    pub elapsed_ms: u64,
    pub decode_start_index: i64,
    pub requested_start_index: i64,
    pub requested_end_index: i64,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PipelineView {
    pub id: String,
    pub uri: String,
    pub title: String,
    pub description: String,
    pub operation: PipelineOperation,
    pub model_uri: String,
    pub prompt_revision: String,
    pub observation_width: u32,
    pub observation_height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineOperation {
    VideoReasoning,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ModelView {
    pub id: String,
    pub uri: String,
    pub title: String,
    pub description: String,
    pub format: ModelFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_digest: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    TensorRtLlmEngine,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct AnalysisView {
    pub analysis_uri: String,
    pub results_uri: String,
    pub task_id: String,
    pub status: String,
    pub progress: f64,
    pub pipeline_id: String,
    pub task_kind: String,
    pub recording_uri: String,
    pub entity_path: String,
    pub timeline: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<AnalyzeRecordingOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn validate_reasoning_task(task: &ReasoningTask) -> Result<()> {
    match task {
        ReasoningTask::DescribeSegment { prompt: None } => Ok(()),
        ReasoningTask::DescribeSegment {
            prompt: Some(prompt),
        }
        | ReasoningTask::DetectEvents { prompt } => validate_prompt("prompt", prompt),
        ReasoningTask::AnswerQuestion { question } => validate_prompt("question", question),
    }
}

pub fn validate_sampling(sampling: ObservationSampling) -> Result<()> {
    ensure!(
        sampling.max_frames > 0 && sampling.max_frames <= MAX_OBSERVATION_FRAMES,
        "sampling max_frames must be within 1..={MAX_OBSERVATION_FRAMES}"
    );
    Ok(())
}

pub fn validate_decode(decode: DecodePolicy) -> Result<()> {
    match decode {
        DecodePolicy::Greedy => Ok(()),
        DecodePolicy::Sampled {
            temperature, top_p, ..
        } => {
            ensure!(
                temperature.is_finite() && temperature > 0.0 && temperature <= 2.0,
                "sampled decode temperature must be within (0, 2]"
            );
            ensure!(
                top_p.is_finite() && top_p > 0.0 && top_p <= 1.0,
                "sampled decode top_p must be within (0, 1]"
            );
            Ok(())
        }
    }
}

fn validate_prompt(name: &str, value: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{name} must not be empty");
    ensure!(
        value.len() <= MAX_PROMPT_BYTES,
        "{name} exceeds {MAX_PROMPT_BYTES} bytes"
    );
    ensure!(
        value
            .chars()
            .all(|character| !character.is_control() || character == '\n' || character == '\t'),
        "{name} contains control characters"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_task_kinds_match_answer_kinds() {
        let task = ReasoningTask::DetectEvents {
            prompt: "vehicles entering the intersection".to_owned(),
        };
        let answer = ReasoningAnswer::Events { events: Vec::new() };
        assert_eq!(task.kind(), answer.kind());
    }

    #[test]
    fn empty_question_is_rejected() {
        let error = validate_reasoning_task(&ReasoningTask::AnswerQuestion {
            question: "   ".to_owned(),
        })
        .unwrap_err();
        assert!(error.to_string().contains("must not be empty"));
    }

    #[test]
    fn sampled_decode_requires_bounded_parameters() {
        assert!(validate_decode(DecodePolicy::Greedy).is_ok());
        assert!(
            validate_decode(DecodePolicy::Sampled {
                temperature: 0.0,
                top_p: 0.9,
                seed: 7,
            })
            .is_err()
        );
        assert!(
            validate_decode(DecodePolicy::Sampled {
                temperature: 0.7,
                top_p: 1.5,
                seed: 7,
            })
            .is_err()
        );
    }
}
