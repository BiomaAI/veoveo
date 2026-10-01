//! Shared, bounded HTTP adapter to the installation's GPU embedding runtime.
mod config;
mod wire;

pub use config::{EmbeddingClientConfig, EmbeddingEndpoint};
use reqwest::{
    Client, Response,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use secrecy::ExposeSecret;
use serde::de::DeserializeOwned;
use std::{sync::Arc, time::Duration};
use tokio::{sync::Semaphore, time::timeout};
pub use veoveo_embedding_contract::{
    EmbeddingBatch, EmbeddingPriority, EmbeddingSpace, EmbeddingTask, EmbeddingText,
    EmbeddingVector,
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
    endpoint: EmbeddingEndpoint,
    space: EmbeddingSpace,
    http: Client,
    deadline: Duration,
    all: Semaphore,
    bulk: Semaphore,
}

/// Clones share the same request and bulk budgets. Construct one client per replica.
#[derive(Clone)]
pub struct EmbeddingClient(Arc<Inner>);
impl EmbeddingClient {
    pub async fn connect(config: EmbeddingClientConfig) -> Result<Self, EmbeddingClientError> {
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            return Err(EmbeddingClientError::Configuration(
                "install the process rustls crypto provider before connecting to embeddings",
            ));
        }
        let key = config.key.expose_secret();
        if key.is_empty() || key.len() > 8192 || key.bytes().any(|b| !b.is_ascii_graphic()) {
            return Err(EmbeddingClientError::Configuration(
                "embedding API key must contain 1..=8192 visible ASCII bytes",
            ));
        }
        let mut authorization = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| EmbeddingClientError::Configuration("invalid embedding API key"))?;
        authorization.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, authorization);
        let http = Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .connect_timeout(Duration::from_secs(3))
            .timeout(config.deadline)
            .build()
            .map_err(|_| {
                EmbeddingClientError::Configuration("cannot initialize embedding HTTP transport")
            })?;
        let client = Self(Arc::new(Inner {
            endpoint: config.endpoint,
            space: config.space,
            http,
            deadline: config.deadline,
            all: Semaphore::new(config.max_in_flight),
            bulk: Semaphore::new(config.max_bulk_in_flight),
        }));
        timeout(client.0.deadline, async {
            let response = client
                .0
                .http
                .get(client.0.endpoint.models())
                .send()
                .await
                .map_err(transport)?;
            read_json::<wire::ModelList>(response, 128 * 1024)
                .await?
                .validate(&client.0.space)
        })
        .await
        .map_err(|_| EmbeddingClientError::Deadline)??;
        Ok(client)
    }
    pub fn space(&self) -> &EmbeddingSpace {
        &self.0.space
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
        // The contract bounds text; include repeated instructions in the wire budget too.
        if input.iter().map(String::len).sum::<usize>() > EmbeddingBatch::MAX_BYTES {
            return Err(EmbeddingClientError::Input(
                veoveo_embedding_contract::EmbeddingError(
                    "formatted embedding batch exceeds 131072 UTF-8 bytes",
                ),
            ));
        }
        timeout(self.0.deadline, async {
            let _bulk = if priority == EmbeddingPriority::Bulk {
                Some(
                    self.0
                        .bulk
                        .acquire()
                        .await
                        .map_err(|_| EmbeddingClientError::Transport)?,
                )
            } else {
                None
            };
            let _all = self
                .0
                .all
                .acquire()
                .await
                .map_err(|_| EmbeddingClientError::Transport)?;
            let request = wire::EmbeddingRequest {
                model: &self.0.space.model,
                input: &input,
                encoding_format: "float",
                dimensions: self.0.space.dimension.get(),
                use_activation: true,
                priority: match priority {
                    EmbeddingPriority::Interactive => 0,
                    EmbeddingPriority::Bulk => 10,
                },
            };
            let response = self
                .0
                .http
                .post(self.0.endpoint.embeddings())
                .json(&request)
                .send()
                .await
                .map_err(transport)?;
            read_json::<wire::EmbeddingResponse>(response, 8 * 1024 * 1024)
                .await?
                .validate(&self.0.space, input.len())
        })
        .await
        .map_err(|_| EmbeddingClientError::Deadline)?
    }
}

fn format_query(task: &EmbeddingTask, text: &EmbeddingText) -> String {
    format!("Instruct: {}\nQuery:{}", task.as_str(), text.as_str())
}

fn transport(error: reqwest::Error) -> EmbeddingClientError {
    if error.is_timeout() {
        EmbeddingClientError::Deadline
    } else {
        EmbeddingClientError::Transport
    }
}

async fn read_json<T: DeserializeOwned>(
    mut response: Response,
    max_bytes: usize,
) -> Result<T, EmbeddingClientError> {
    if !response.status().is_success() {
        return Err(EmbeddingClientError::HttpStatus(response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|len| len > max_bytes as u64)
    {
        return Err(EmbeddingClientError::Response(
            "runtime response exceeds its byte limit",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if chunk.len() > max_bytes - bytes.len() {
            return Err(EmbeddingClientError::Response(
                "runtime response exceeds its byte limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        EmbeddingClientError::Response(
            "runtime response does not match the supported JSON contract",
        )
    })
}
