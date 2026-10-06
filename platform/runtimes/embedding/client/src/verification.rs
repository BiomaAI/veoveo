//! Candidate measurements for hardware verification, with no production admission or vector conversion.
mod reports;
pub use reports::*;

use crate::{config::HttpConfig, transport::Transport, *};
use secrecy::SecretString;
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use veoveo_embedding_contract::{EmbeddingExecutionProfile, EmbeddingExecutionProfileId};

#[derive(Debug)]
pub struct CandidateClientConfig {
    http: HttpConfig,
    profile: EmbeddingExecutionProfile,
}
impl CandidateClientConfig {
    pub fn new(
        endpoint: EmbeddingEndpoint,
        key: SecretString,
        profile: EmbeddingExecutionProfile,
    ) -> Self {
        Self {
            http: HttpConfig::new(endpoint, key),
            profile,
        }
    }
    pub fn with_deadline(mut self, deadline: Duration) -> Result<Self, EmbeddingClientError> {
        self.http.deadline(deadline)?;
        Ok(self)
    }
    pub fn with_concurrency(
        mut self,
        total: usize,
        bulk: usize,
    ) -> Result<Self, EmbeddingClientError> {
        self.http.concurrency(total, bulk)?;
        Ok(self)
    }
}
struct Inner {
    transport: Arc<Transport>,
    profile: EmbeddingExecutionProfile,
}

/// Measurements cannot satisfy Knowledge's Embeddings port or construct production vectors.
#[derive(Clone)]
pub struct CandidateEmbeddingClient(Arc<Inner>);
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateMeasurement {
    profile_id: EmbeddingExecutionProfileId,
    values: Vec<f32>,
}
impl CandidateMeasurement {
    pub fn profile_id(&self) -> &EmbeddingExecutionProfileId {
        &self.profile_id
    }
    pub fn values(&self) -> &[f32] {
        &self.values
    }
}
impl CandidateEmbeddingClient {
    pub async fn connect(config: CandidateClientConfig) -> Result<Self, EmbeddingClientError> {
        let transport = Transport::connect(config.http, config.profile.space()).await?;
        Ok(Self(Arc::new(Inner {
            transport,
            profile: config.profile,
        })))
    }
    pub fn profile(&self) -> &EmbeddingExecutionProfile {
        &self.0.profile
    }
    pub async fn embed_documents(
        &self,
        batch: &EmbeddingBatch,
        priority: EmbeddingPriority,
    ) -> Result<Vec<CandidateMeasurement>, EmbeddingClientError> {
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
    pub async fn embed_queries(
        &self,
        task: &EmbeddingTask,
        batch: &EmbeddingBatch,
        priority: EmbeddingPriority,
    ) -> Result<Vec<CandidateMeasurement>, EmbeddingClientError> {
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
    pub async fn embed_query(
        &self,
        task: &EmbeddingTask,
        text: &EmbeddingText,
    ) -> Result<CandidateMeasurement, EmbeddingClientError> {
        self.embed(
            vec![format_query(task, text)],
            EmbeddingPriority::Interactive,
        )
        .await?
        .pop()
        .ok_or(EmbeddingClientError::Response("query vector is missing"))
    }
    async fn embed(
        &self,
        input: Vec<String>,
        priority: EmbeddingPriority,
    ) -> Result<Vec<CandidateMeasurement>, EmbeddingClientError> {
        Ok(self
            .0
            .transport
            .embed(input, priority, self.profile().space())
            .await?
            .into_iter()
            .map(|values| CandidateMeasurement {
                profile_id: self.profile().id().clone(),
                values,
            })
            .collect())
    }
}
