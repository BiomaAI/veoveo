//! Shared transport mechanics for admitted production and candidate measurement clients.
use crate::config::HttpConfig;
use crate::{EmbeddingBatch, EmbeddingClientError, EmbeddingPriority, EmbeddingSpace, wire};
use reqwest::{
    Client, Response,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use secrecy::ExposeSecret;
use serde::de::DeserializeOwned;
use std::{sync::Arc, time::Duration};
use tokio::{sync::Semaphore, time::timeout};

pub(crate) struct Transport {
    endpoint: crate::EmbeddingEndpoint,
    http: Client,
    deadline: Duration,
    all: Semaphore,
    bulk: Semaphore,
}
impl Transport {
    pub(crate) async fn connect(
        config: HttpConfig,
        space: &EmbeddingSpace,
    ) -> Result<Arc<Self>, EmbeddingClientError> {
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

        let client = Arc::new(Self {
            endpoint: config.endpoint,
            http,
            deadline: config.deadline,
            all: Semaphore::new(config.max_in_flight),
            bulk: Semaphore::new(config.max_bulk_in_flight),
        });
        timeout(client.deadline, async {
            let response = client
                .http
                .get(client.endpoint.models())
                .send()
                .await
                .map_err(transport)?;
            read_json::<wire::ModelList>(response, 128 * 1024)
                .await?
                .validate(space)
        })
        .await
        .map_err(|_| EmbeddingClientError::Deadline)??;
        Ok(client)
    }
    pub(crate) async fn embed(
        &self,
        input: Vec<String>,
        priority: EmbeddingPriority,
        space: &EmbeddingSpace,
    ) -> Result<Vec<Vec<f32>>, EmbeddingClientError> {
        // The contract bounds text; include repeated instructions in the wire budget too.
        if input.iter().map(String::len).sum::<usize>() > EmbeddingBatch::MAX_BYTES {
            return Err(EmbeddingClientError::Input(
                veoveo_embedding_contract::EmbeddingError(
                    "formatted embedding batch exceeds 131072 UTF-8 bytes",
                ),
            ));
        }
        timeout(self.deadline, async {
            let _bulk = if priority == EmbeddingPriority::Bulk {
                Some(
                    self.bulk
                        .acquire()
                        .await
                        .map_err(|_| EmbeddingClientError::Transport)?,
                )
            } else {
                None
            };
            let _all = self
                .all
                .acquire()
                .await
                .map_err(|_| EmbeddingClientError::Transport)?;
            let request = wire::EmbeddingRequest {
                model: &space.model,
                input: &input,
                encoding_format: "float",
                use_activation: true,
                priority: match priority {
                    EmbeddingPriority::Interactive => 0,
                    EmbeddingPriority::Bulk => 10,
                },
            };
            let response = self
                .http
                .post(self.endpoint.embeddings())
                .json(&request)
                .send()
                .await
                .map_err(transport)?;
            read_json::<wire::EmbeddingResponse>(response, 8 * 1024 * 1024)
                .await?
                .validate(space, input.len())
        })
        .await
        .map_err(|_| EmbeddingClientError::Deadline)?
    }
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
