//! Checked generation result and Media-owned Artifact attribution.
use super::{GenerationPredictionSummary, MediaGenerationUri, MediaPredictionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};
use veoveo_artifact_contract::{ArtifactMetadata, ArtifactUri};
use veoveo_types::{ResourceScheme, TaskId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaOutputArtifactMetadata {
    pub task_id: TaskId,
    pub job_id: MediaPredictionId,
    #[schemars(with = "String")]
    pub model_id: super::MediaModelId,
    pub output_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaGenerationError;
impl fmt::Display for MediaGenerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Media generation result requires matching Task, prediction, result address and ordered output attribution")
    }
}
impl std::error::Error for MediaGenerationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum ResultSchema {
    #[serde(rename = "veoveo.ai/media-generation/v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ResultWire", into = "ResultWire")]
pub struct MediaGenerationResult {
    task_id: TaskId,
    result_uri: MediaGenerationUri,
    prediction: GenerationPredictionSummary,
    artifacts: Vec<ArtifactMetadata>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ResultWire {
    schema: ResultSchema,
    task_id: TaskId,
    result_uri: MediaGenerationUri,
    prediction: GenerationPredictionSummary,
    artifacts: Vec<ArtifactMetadata>,
}

impl MediaGenerationResult {
    pub const SCHEMA: &str = "veoveo.ai/media-generation/v1";

    /// All outputs must be attributed to this Task and prediction in provider order.
    /// ```compile_fail
    /// use veoveo_media_mcp::contract::{MediaGenerationResult, GenerationPredictionSummary};
    /// fn wrong(summary: GenerationPredictionSummary) {
    ///     MediaGenerationResult::new("raw-task", summary, vec![]);
    /// }
    /// ```
    pub fn new(
        task_id: TaskId,
        prediction: GenerationPredictionSummary,
        artifacts: Vec<ArtifactMetadata>,
    ) -> Result<Self, MediaGenerationError> {
        if task_id.as_uuid().get_version_num() != 7
            || task_id.as_uuid().get_variant() != uuid::Variant::RFC4122
            || prediction.status != "completed"
            || prediction.output_count != artifacts.len()
        {
            return Err(MediaGenerationError);
        }
        let scheme = ResourceScheme::new("media").expect("declared Media scheme");
        let mut identities = BTreeSet::new();
        for (index, artifact) in artifacts.iter().enumerate() {
            let attribution: MediaOutputArtifactMetadata =
                serde_json::from_value(artifact.metadata.clone())
                    .map_err(|_| MediaGenerationError)?;
            if attribution.task_id != task_id
                || attribution.job_id != prediction.id
                || attribution.model_id != prediction.model_id
                || attribution.output_index != index
                || artifact.artifact_uri != ArtifactUri::presented(&scheme, artifact.artifact_id())
                || artifact.download_url.is_some()
                || !identities.insert(artifact.artifact_id())
            {
                return Err(MediaGenerationError);
            }
        }
        Ok(Self {
            task_id,
            result_uri: MediaGenerationUri::new(prediction.id.clone()),
            prediction,
            artifacts,
        })
    }
    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
    pub fn result_uri(&self) -> &MediaGenerationUri {
        &self.result_uri
    }
    pub fn prediction(&self) -> &GenerationPredictionSummary {
        &self.prediction
    }
    pub fn artifacts(&self) -> &[ArtifactMetadata] {
        &self.artifacts
    }
}
impl TryFrom<ResultWire> for MediaGenerationResult {
    type Error = MediaGenerationError;
    fn try_from(wire: ResultWire) -> Result<Self, Self::Error> {
        let result = Self::new(wire.task_id, wire.prediction, wire.artifacts)?;
        if result.result_uri != wire.result_uri {
            return Err(MediaGenerationError);
        }
        Ok(result)
    }
}
impl From<MediaGenerationResult> for ResultWire {
    fn from(result: MediaGenerationResult) -> Self {
        Self {
            schema: ResultSchema::V1,
            task_id: result.task_id,
            result_uri: result.result_uri,
            prediction: result.prediction,
            artifacts: result.artifacts,
        }
    }
}
