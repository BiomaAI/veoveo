use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ArtifactMetadata;

/// Public, provider-neutral summary of the provider job behind a completed task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GenerationPredictionSummary {
    pub id: String,
    pub model_id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<Value>,
    pub output_count: usize,
}

/// Structured content returned by generation-oriented `run` tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GenerationRunOutput {
    pub prediction: GenerationPredictionSummary,
    pub artifacts: Vec<ArtifactMetadata>,
}
