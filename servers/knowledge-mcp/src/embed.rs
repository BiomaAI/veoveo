use crate::ServiceError;
use std::future::Future;
use veoveo_embedding_client::{EmbeddingClient, EmbeddingPriority};
use veoveo_embedding_contract::{
    EmbeddingBatch, EmbeddingSpace, EmbeddingTask, EmbeddingText, EmbeddingVector,
};

/// Production uses the shared HTTP client. Native fixtures supply synthetic vectors
/// to qualify lifecycle and policy without claiming inference acceptance.
pub trait Embeddings: Send + Sync {
    fn space(&self) -> &EmbeddingSpace;
    fn documents(
        &self,
        texts: EmbeddingBatch,
    ) -> impl Future<Output = Result<Vec<EmbeddingVector>, ServiceError>> + Send;
    fn query(
        &self,
        task: EmbeddingTask,
        text: EmbeddingText,
    ) -> impl Future<Output = Result<EmbeddingVector, ServiceError>> + Send;
}
impl Embeddings for EmbeddingClient {
    fn space(&self) -> &EmbeddingSpace {
        self.space()
    }
    async fn documents(&self, texts: EmbeddingBatch) -> Result<Vec<EmbeddingVector>, ServiceError> {
        Ok(self
            .embed_documents(&texts, EmbeddingPriority::Bulk)
            .await?)
    }
    async fn query(
        &self,
        task: EmbeddingTask,
        text: EmbeddingText,
    ) -> Result<EmbeddingVector, ServiceError> {
        Ok(self.embed_query(&task, &text).await?)
    }
}
