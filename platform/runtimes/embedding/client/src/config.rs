use crate::EmbeddingClientError;
use secrecy::SecretString;
use std::time::Duration;
use url::Url;
use veoveo_embedding_contract::EmbeddingSpace;

/// Operator-configured runtime origin. Credentials never travel in the URL.
#[derive(Debug, Clone)]
pub struct EmbeddingEndpoint(Url);
impl EmbeddingEndpoint {
    pub fn parse(value: &str) -> Result<Self, EmbeddingClientError> {
        let url = Url::parse(value)
            .map_err(|_| EmbeddingClientError::Configuration("invalid runtime origin"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err(EmbeddingClientError::Configuration(
                "runtime requires an HTTP(S) origin without userinfo, path, query or fragment",
            ));
        }
        Ok(Self(url))
    }
    pub(crate) fn models(&self) -> Url {
        self.0.join("v1/models").expect("fixed runtime route")
    }
    pub(crate) fn embeddings(&self) -> Url {
        self.0.join("v1/embeddings").expect("fixed runtime route")
    }
}

#[derive(Debug)]
pub struct EmbeddingClientConfig {
    pub(crate) endpoint: EmbeddingEndpoint,
    pub(crate) key: SecretString,
    pub(crate) space: EmbeddingSpace,
    pub(crate) deadline: Duration,
    pub(crate) max_in_flight: usize,
    pub(crate) max_bulk_in_flight: usize,
}
impl EmbeddingClientConfig {
    pub fn new(endpoint: EmbeddingEndpoint, key: SecretString, space: EmbeddingSpace) -> Self {
        Self {
            endpoint,
            key,
            space,
            deadline: Duration::from_secs(60),
            max_in_flight: 4,
            max_bulk_in_flight: 1,
        }
    }
    pub fn with_deadline(mut self, deadline: Duration) -> Result<Self, EmbeddingClientError> {
        if deadline.is_zero() || deadline > Duration::from_secs(120) {
            return Err(EmbeddingClientError::Configuration(
                "embedding deadline must be positive and at most 120 seconds",
            ));
        }
        self.deadline = deadline;
        Ok(self)
    }
    pub fn with_concurrency(
        mut self,
        total: usize,
        bulk: usize,
    ) -> Result<Self, EmbeddingClientError> {
        if bulk == 0 || bulk >= total || total > 64 {
            return Err(EmbeddingClientError::Configuration(
                "embedding concurrency requires 1 <= bulk < total <= 64",
            ));
        }
        self.max_in_flight = total;
        self.max_bulk_in_flight = bulk;
        Ok(self)
    }
}
