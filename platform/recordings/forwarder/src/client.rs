use anyhow::{Context, Result, ensure};
use prost::Message;
use secrecy::ExposeSecret;
use url::Url;
use veoveo_recording_protocol::{
    DISCOVERY_PATH, MEDIA_TYPE, PROTOCOL_VERSION, REQUIRED_SCOPE, STREAMS_PATH,
    v1::{
        AppendRecordingBatchResult, FinishRecordingStreamRequest, FinishRecordingStreamResult,
        IngestError, OpenRecordingStreamRequest, RecordingBatch, RecordingIngestDiscovery,
        RecordingStream,
    },
};

use crate::oauth::{
    AuthorizationServerMetadata, OAuthTokenProvider, authorization_server_metadata_url,
};

#[derive(Debug, thiserror::Error)]
#[error("recording ingest returned HTTP {status}: {message}")]
pub struct IngestRequestError {
    pub status: reqwest::StatusCode,
    pub message: String,
    pub retry_after_seconds: Option<u64>,
}

#[derive(Clone)]
pub struct RecordingIngestClient {
    http: reqwest::Client,
    streams_endpoint: Url,
    maximum_batch_bytes: u64,
    tokens: OAuthTokenProvider,
}

impl RecordingIngestClient {
    pub async fn discover(
        http: reqwest::Client,
        gateway_url: &Url,
        expected_protected_resource: &Url,
        token_builder: impl FnOnce(Url) -> Result<OAuthTokenProvider>,
    ) -> Result<Self> {
        let discovery_url = gateway_url.join(DISCOVERY_PATH.trim_start_matches('/'))?;
        let response = http
            .get(discovery_url)
            .send()
            .await
            .context("requesting recording ingest discovery")?
            .error_for_status()
            .context("recording ingest discovery failed")?;
        ensure_media_type(response.headers())?;
        let discovery = RecordingIngestDiscovery::decode(response.bytes().await?)?;
        ensure!(
            discovery.protocol_version == PROTOCOL_VERSION
                && discovery.required_scope == REQUIRED_SCOPE
                && discovery.protected_resource == expected_protected_resource.as_str(),
            "recording ingest discovery does not match the configured protocol and resource"
        );
        ensure!(
            discovery.maximum_batch_bytes > 0,
            "gateway advertised a zero batch limit"
        );
        let issuer = Url::parse(&discovery.authorization_server)?;
        let metadata_url = authorization_server_metadata_url(&issuer)?;
        let metadata = http
            .get(metadata_url)
            .send()
            .await
            .context("requesting OAuth authorization-server metadata")?
            .error_for_status()?
            .json::<AuthorizationServerMetadata>()
            .await?;
        ensure!(
            metadata.issuer == issuer.as_str(),
            "OAuth issuer metadata mismatch"
        );
        let token_endpoint = Url::parse(&metadata.token_endpoint)?;
        let streams_endpoint = Url::parse(&discovery.streams_endpoint)?;
        ensure!(
            streams_endpoint.origin() == gateway_url.origin()
                && streams_endpoint.path() == STREAMS_PATH,
            "gateway advertised a non-canonical or cross-origin streams endpoint"
        );
        let tokens = token_builder(token_endpoint)?;
        Ok(Self {
            http,
            streams_endpoint,
            maximum_batch_bytes: discovery.maximum_batch_bytes,
            tokens,
        })
    }

    pub fn maximum_batch_bytes(&self) -> u64 {
        self.maximum_batch_bytes
    }

    pub async fn open(&self, request: &OpenRecordingStreamRequest) -> Result<RecordingStream> {
        self.post_protobuf(self.streams_endpoint.clone(), request)
            .await
    }

    pub async fn append(
        &self,
        stream_id: &str,
        batch: &RecordingBatch,
    ) -> Result<AppendRecordingBatchResult> {
        let url = self
            .stream_url(stream_id)?
            .join(&format!("batches/{}", batch.sequence))?;
        self.request_protobuf(reqwest::Method::PUT, url, batch)
            .await
    }

    pub async fn finish(&self, stream_id: &str) -> Result<FinishRecordingStreamResult> {
        let url = self.stream_url(stream_id)?.join("finish")?;
        self.post_protobuf(url, &FinishRecordingStreamRequest {})
            .await
    }

    fn stream_url(&self, stream_id: &str) -> Result<Url> {
        let uuid = uuid::Uuid::parse_str(stream_id)?;
        ensure!(
            uuid.get_version_num() == 7,
            "recording stream ID is not UUIDv7"
        );
        Ok(Url::parse(&format!(
            "{}/{stream_id}/",
            self.streams_endpoint.as_str().trim_end_matches('/')
        ))?)
    }

    async fn post_protobuf<Request, Response>(
        &self,
        url: Url,
        request: &Request,
    ) -> Result<Response>
    where
        Request: Message,
        Response: Message + Default,
    {
        self.request_protobuf(reqwest::Method::POST, url, request)
            .await
    }

    async fn request_protobuf<Request, Response>(
        &self,
        method: reqwest::Method,
        url: Url,
        request: &Request,
    ) -> Result<Response>
    where
        Request: Message,
        Response: Message + Default,
    {
        let mut retried_token = false;
        loop {
            let token = self.tokens.access_token().await?;
            let response = self
                .http
                .request(method.clone(), url.clone())
                .bearer_auth(token.expose_secret())
                .header(reqwest::header::CONTENT_TYPE, MEDIA_TYPE)
                .body(request.encode_to_vec())
                .send()
                .await
                .context("sending recording ingest request")?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && !retried_token {
                self.tokens.invalidate(&token).await;
                retried_token = true;
                continue;
            }
            ensure_media_type(response.headers())?;
            let status = response.status();
            let bytes = response.bytes().await?;
            if status.is_success() {
                return Ok(Response::decode(bytes)?);
            }
            let error = IngestError::decode(bytes).unwrap_or(IngestError {
                code: 0,
                message: "gateway returned an invalid protobuf error".to_owned(),
                expected_sequence: None,
                retry_after_seconds: None,
            });
            return Err(IngestRequestError {
                status,
                message: error.message,
                retry_after_seconds: error.retry_after_seconds,
            }
            .into());
        }
    }
}

fn ensure_media_type(headers: &reqwest::header::HeaderMap) -> Result<()> {
    ensure!(
        headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            == Some(MEDIA_TYPE),
        "recording ingest response has the wrong media type"
    );
    Ok(())
}
