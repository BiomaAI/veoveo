//! Shared, bounded HTTP adapter to the installation's GPU embedding runtime.
mod config;
mod wire;

pub use config::{EmbeddingClientConfig, EmbeddingEndpoint};
mod transport;
#[cfg(feature = "verification")]
pub mod verification;
use std::sync::Arc;
pub use veoveo_embedding_contract::{
    EmbeddingBatch, EmbeddingPriority, EmbeddingSpace, EmbeddingTask, EmbeddingText,
    EmbeddingVector, QualifiedEmbeddingRuntime,
};
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingClientError {
    #[error("embedding configuration: {0}")]
    Configuration(&'static str),
    #[error("embedding response: {0}")]
    Response(&'static str),
    #[error("embedding runtime returned HTTP {0}")]
    HttpStatus(u16),
    #[error("embedding runtime transport failed")]
    Transport,
    #[error("embedding operation exceeded its deadline")]
    Deadline,
    #[error(transparent)]
    Input(#[from] veoveo_embedding_contract::EmbeddingError),
}

struct Inner {
    runtime: QualifiedEmbeddingRuntime,
    transport: Arc<transport::Transport>,
}

/// Clones share request permits. Only an installation-admitted runtime constructs this client.
#[derive(Clone)]
pub struct EmbeddingClient(Arc<Inner>);
impl EmbeddingClient {
    pub async fn connect(config: EmbeddingClientConfig) -> Result<Self, EmbeddingClientError> {
        let transport = transport::Transport::connect(config.http, config.runtime.space()).await?;
        Ok(Self(Arc::new(Inner {
            runtime: config.runtime,
            transport,
        })))
    }
    pub fn space(&self) -> &EmbeddingSpace {
        self.0.runtime.space()
    }
    pub fn runtime(&self) -> &QualifiedEmbeddingRuntime {
        &self.0.runtime
    }

    pub async fn embed_documents(
        &self,
        batch: &EmbeddingBatch,
        priority: EmbeddingPriority,
    ) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
        self.embed(
            batch
                .texts()
                .iter()
                .map(|text| text.as_str().to_owned())
                .collect(),
            priority,
        )
        .await
    }

    pub async fn embed_query(
        &self,
        task: &EmbeddingTask,
        text: &EmbeddingText,
    ) -> Result<EmbeddingVector, EmbeddingClientError> {
        self.embed(
            vec![format_query(task, text)],
            EmbeddingPriority::Interactive,
        )
        .await?
        .pop()
        .ok_or(EmbeddingClientError::Response("query vector is missing"))
    }

    pub async fn embed_queries(
        &self,
        task: &EmbeddingTask,
        batch: &EmbeddingBatch,
        priority: EmbeddingPriority,
    ) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
        self.embed(
            batch
                .texts()
                .iter()
                .map(|text| format_query(task, text))
                .collect(),
            priority,
        )
        .await
    }

    async fn embed(
        &self,
        input: Vec<String>,
        priority: EmbeddingPriority,
    ) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
        self.0
            .transport
            .embed(input, priority, self.space())
            .await?
            .into_iter()
            .map(|values| EmbeddingVector::new(self.runtime(), values).map_err(Into::into))
            .collect()
    }
}

fn format_query(task: &EmbeddingTask, text: &EmbeddingText) -> String {
    format!("Instruct: {}\nQuery:{}", task.as_str(), text.as_str())
}
