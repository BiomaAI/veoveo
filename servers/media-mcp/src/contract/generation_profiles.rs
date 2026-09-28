//! Explicit retained-data and coordinated-rollback compatibility, owned by Media.
use super::{GenerationPredictionSummary, MediaGenerationError, MediaGenerationResult};
use serde::Deserialize;
use veoveo_artifact_contract::ArtifactMetadata;
use veoveo_types::TaskId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaGenerationProfile {
    UnversionedV0,
    V1,
}

/// A checked canonical result together with the profile admitted at the boundary.
/// Ordinary new producers serialize `MediaGenerationResult` directly as v1.
pub struct RetainedMediaGeneration {
    profile: MediaGenerationProfile,
    generation: MediaGenerationResult,
}

// An unknown version or incomplete v1 cannot fall through to this closed shape.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerationResultV0 {
    prediction: GenerationPredictionSummary,
    artifacts: Vec<ArtifactMetadata>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredResult {
    V1(MediaGenerationResult),
    V0(GenerationResultV0),
}

impl RetainedMediaGeneration {
    /// Decode only the declared retained profiles and bind their attribution to
    /// the supplied native Task. This conversion grants no resource access.
    pub fn decode(task: TaskId, value: serde_json::Value) -> Result<Self, MediaGenerationError> {
        let (profile, generation) = match serde_json::from_value::<StoredResult>(value)
            .map_err(|_| MediaGenerationError)?
        {
            StoredResult::V1(generation) => (MediaGenerationProfile::V1, generation),
            StoredResult::V0(legacy) => (
                MediaGenerationProfile::UnversionedV0,
                MediaGenerationResult::new(task, legacy.prediction, legacy.artifacts)?,
            ),
        };
        if generation.task_id() != task {
            return Err(MediaGenerationError);
        }
        Ok(Self {
            profile,
            generation,
        })
    }

    pub fn profile(&self) -> MediaGenerationProfile {
        self.profile
    }

    pub fn generation(&self) -> &MediaGenerationResult {
        &self.generation
    }

    pub fn into_generation(self) -> MediaGenerationResult {
        self.generation
    }
}
