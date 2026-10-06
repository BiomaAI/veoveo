use crate::ServiceError;
use std::future::Future;
use veoveo_embedding_client::{EmbeddingClient, EmbeddingPriority};
use veoveo_embedding_contract::{
    EmbeddingBatch, EmbeddingSpace, EmbeddingTask, EmbeddingText, EmbeddingVector,
    QualifiedEmbeddingRuntime,
};

/// Production uses the shared HTTP client. Native fixtures supply synthetic vectors
/// to qualify lifecycle and policy without claiming inference acceptance.
pub trait Embeddings: Send + Sync {
    fn runtime(&self) -> &QualifiedEmbeddingRuntime;
    fn space(&self) -> &EmbeddingSpace {
        self.runtime().space()
    }
    fn documents(
        &self,
        texts: EmbeddingBatch,
    ) -> impl Future<Output = Result<Vec<EmbeddingVector>, ServiceError>> + Send;
    fn queries(
        &self,
        task: EmbeddingTask,
        texts: EmbeddingBatch,
    ) -> impl Future<Output = Result<Vec<EmbeddingVector>, ServiceError>> + Send;
    fn query(
        &self,
        task: EmbeddingTask,
        text: EmbeddingText,
    ) -> impl Future<Output = Result<EmbeddingVector, ServiceError>> + Send;
}
impl Embeddings for EmbeddingClient {
    fn runtime(&self) -> &QualifiedEmbeddingRuntime {
        self.runtime()
    }
    async fn documents(&self, texts: EmbeddingBatch) -> Result<Vec<EmbeddingVector>, ServiceError> {
        Ok(self
            .embed_documents(&texts, EmbeddingPriority::Bulk)
            .await?)
    }
    async fn queries(
        &self,
        task: EmbeddingTask,
        texts: EmbeddingBatch,
    ) -> Result<Vec<EmbeddingVector>, ServiceError> {
        Ok(self
            .embed_queries(&task, &texts, EmbeddingPriority::Interactive)
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
