use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_embedding_contract::{
    EmbeddingBatch, EmbeddingSpace, EmbeddingTask, EmbeddingText, EmbeddingVector,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum EmbedRequest {
    Document {
        texts: EmbeddingBatch,
    },
    Query {
        texts: EmbeddingBatch,
        task: EmbeddingTask,
    },
}
impl EmbedRequest {
    pub fn texts(&self) -> &[EmbeddingText] {
        match self {
            Self::Document { texts } | Self::Query { texts, .. } => texts.texts(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmbedResponse {
    pub space: EmbeddingSpace,
    pub vectors: Vec<EmbeddingVector>,
}
