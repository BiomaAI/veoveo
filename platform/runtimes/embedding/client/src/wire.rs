//! The supported vLLM Embeddings API subset. Additional provider fields are ignored.
use crate::EmbeddingClientError;
use serde::{Deserialize, Serialize};
use veoveo_embedding_contract::{EmbeddingModelId, EmbeddingSpace, EmbeddingVector};

#[derive(Serialize)]
pub(crate) struct EmbeddingRequest<'a> {
    pub model: &'a EmbeddingModelId,
    pub input: &'a [String],
    pub encoding_format: &'static str,
    pub dimensions: u16,
    pub use_activation: bool,
    pub priority: i32,
}

#[derive(Deserialize)]
pub(crate) struct ModelList {
    pub object: String,
    pub data: Vec<ModelCard>,
}
#[derive(Deserialize)]
pub(crate) struct ModelCard {
    pub id: EmbeddingModelId,
    pub object: String,
}
impl ModelList {
    pub fn validate(&self, space: &EmbeddingSpace) -> Result<(), EmbeddingClientError> {
        let mut matching = self.data.iter().filter(|card| card.id == space.model);
        if self.object != "list"
            || !matching.next().is_some_and(|card| card.object == "model")
            || matching.next().is_some()
        {
            return Err(EmbeddingClientError::Response(
                "runtime must advertise the configured embedding model exactly once",
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub(crate) struct EmbeddingResponse {
    object: String,
    model: EmbeddingModelId,
    data: Vec<EmbeddingItem>,
}
#[derive(Deserialize)]
struct EmbeddingItem {
    index: usize,
    object: String,
    embedding: Vec<f32>,
}
impl EmbeddingResponse {
    pub fn validate(
        self,
        space: &EmbeddingSpace,
        count: usize,
    ) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
        if self.object != "list" || self.model != space.model || self.data.len() != count {
            return Err(EmbeddingClientError::Response(
                "embedding response model or vector count differs from the request",
            ));
        }
        let mut ordered = vec![None; count];
        for item in self.data {
            if item.object != "embedding" || item.index >= count || ordered[item.index].is_some() {
                return Err(EmbeddingClientError::Response(
                    "embedding response has an invalid or duplicate input index",
                ));
            }
            ordered[item.index] = Some(EmbeddingVector::new(space.clone(), item.embedding)?);
        }
        ordered
            .into_iter()
            .map(|vector| {
                vector.ok_or(EmbeddingClientError::Response(
                    "embedding response omits an input index",
                ))
            })
            .collect()
    }
}
