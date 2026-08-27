//! Retry-safe file streaming from Recording Hub through the Gateway into the
//! authoritative Artifact plane.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, ensure};
use reqwest::header::{HOST, HeaderMap, HeaderValue};
use secrecy::ExposeSecret as _;
use url::Url;
use veoveo_mcp_contract::{
    ArtifactId, ArtifactMetadata, PutArtifactRequest, StreamArtifactRequest,
};
use veoveo_platform_store::RecordingLayerId;
use veoveo_recording_forwarder::{
    config::ClientAssertionAlgorithm,
    oauth::{OAuthTokenProvider, OAuthTokenProviderConfig},
};

const PUBLICATION_SCOPE: &str = "recording:publish";
const MAXIMUM_PUBLICATION_SECONDS: u64 = 300;

pub struct GatewayLayerPublisherConfig {
    pub gateway_url: Url,
    pub gateway_transport_url: Option<Url>,
    pub protected_resource: Url,
    pub profile: String,
    pub client_id: String,
    pub private_key_pem_file: PathBuf,
    pub key_id: String,
    pub algorithm: ClientAssertionAlgorithm,
}

#[derive(Clone)]
pub struct GatewayLayerPublisher {
    http: reqwest::Client,
    endpoint: Url,
    tokens: OAuthTokenProvider,
}

impl GatewayLayerPublisher {
    pub fn new(config: GatewayLayerPublisherConfig) -> Result<Self> {
        validate_origin(&config.gateway_url, "gateway URL")?;
        let transport = config
            .gateway_transport_url
            .as_ref()
            .unwrap_or(&config.gateway_url);
        validate_transport_origin(transport)?;
        ensure!(
            config.protected_resource.origin() == config.gateway_url.origin(),
            "recording publication protected resource must use the canonical gateway origin"
        );
        ensure!(
            !config.profile.trim().is_empty(),
            "recording publication profile must not be empty"
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            HOST,
            HeaderValue::from_str(&canonical_authority(&config.gateway_url)?)?,
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .https_only(transport.scheme() == "https")
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(MAXIMUM_PUBLICATION_SECONDS))
            .build()?;
        let token_endpoint = config.gateway_url.join("oauth/token")?;
        let token_transport_endpoint =
            transport_url(&config.gateway_url, transport, &token_endpoint)?;
        let endpoint = transport_url(
            &config.gateway_url,
            transport,
            &config
                .gateway_url
                .join(&format!("recordings/{}/layers", config.profile))?,
        )?;
        let tokens = OAuthTokenProvider::new(OAuthTokenProviderConfig {
            http: http.clone(),
            token_endpoint,
            token_transport_endpoint,
            protected_resource: config.protected_resource,
            client_id: config.client_id,
            scope: PUBLICATION_SCOPE.to_owned(),
            key_id: config.key_id,
            algorithm: config.algorithm,
            private_key_pem_file: config.private_key_pem_file,
        })?;
        Ok(Self {
            http,
            endpoint,
            tokens,
        })
    }

    pub async fn publish(
        &self,
        layer_id: RecordingLayerId,
        artifact: PutArtifactRequest,
        path: &Path,
        expected_byte_len: u64,
        expected_sha256: &str,
    ) -> Result<ArtifactMetadata> {
        let artifact_id = ArtifactId::parse(layer_id.to_string())
            .context("recording layer ID is not a valid Artifact occurrence ID")?;
        self.publish_artifact(
            artifact_id,
            artifact,
            path,
            expected_byte_len,
            expected_sha256,
        )
        .await
    }

    pub async fn publish_artifact(
        &self,
        artifact_id: ArtifactId,
        artifact: PutArtifactRequest,
        path: &Path,
        expected_byte_len: u64,
        expected_sha256: &str,
    ) -> Result<ArtifactMetadata> {
        let request = StreamArtifactRequest {
            artifact_id,
            artifact,
            expected_byte_len,
            expected_sha256: expected_sha256.to_owned(),
        };
        let descriptor = serde_json::to_string(&request)?;
        ensure!(
            descriptor.len() <= veoveo_mcp_contract::MAX_ARTIFACT_PUT_DESCRIPTOR_BYTES,
            "recording layer publication descriptor exceeds the Artifact limit"
        );
        let mut token = self.tokens.access_token().await?;
        let mut response = self
            .send(path, expected_byte_len, &descriptor, &token)
            .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.tokens.invalidate(&token).await;
            token = self.tokens.access_token().await?;
            response = self
                .send(path, expected_byte_len, &descriptor, &token)
                .await?;
        }
        let status = response.status();
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "response body unavailable".to_owned());
            anyhow::bail!(
                "recording layer publication failed with HTTP {status}: {}",
                bounded_error(&message)
            );
        }
        let metadata = response
            .json::<ArtifactMetadata>()
            .await
            .context("decoding recording layer Artifact metadata")?;
        ensure!(
            metadata.artifact_id == artifact_id
                && metadata.byte_len == expected_byte_len
                && metadata.download_url.is_none(),
            "Artifact service returned mismatched recording layer metadata"
        );
        Ok(metadata)
    }

    async fn send(
        &self,
        path: &Path,
        expected_byte_len: u64,
        descriptor: &str,
        token: &secrecy::SecretString,
    ) -> Result<reqwest::Response> {
        let file = tokio::fs::File::open(path)
            .await
            .with_context(|| format!("opening normalized recording layer {}", path.display()))?;
        ensure!(
            file.metadata().await?.len() == expected_byte_len,
            "normalized recording layer length changed before publication"
        );
        self.http
            .post(self.endpoint.clone())
            .bearer_auth(token.expose_secret())
            .header("x-artifact-stream-put", descriptor)
            .header(reqwest::header::CONTENT_LENGTH, expected_byte_len)
            .body(reqwest::Body::wrap_stream(
                tokio_util::io::ReaderStream::new(file),
            ))
            .send()
            .await
            .context("streaming recording layer through Gateway")
    }
}

fn validate_origin(url: &Url, label: &str) -> Result<()> {
    ensure!(
        (url.scheme() == "https"
            || (url.scheme() == "http" && url.host_str().is_some_and(is_loopback_host)))
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "{label} must be an HTTPS or loopback HTTP origin"
    );
    Ok(())
}

fn validate_transport_origin(url: &Url) -> Result<()> {
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "gateway transport URL must be an HTTP(S) origin without credentials"
    );
    Ok(())
}

fn transport_url(canonical_base: &Url, transport_base: &Url, canonical: &Url) -> Result<Url> {
    ensure!(
        canonical.origin() == canonical_base.origin(),
        "canonical recording publication URL escaped the Gateway origin"
    );
    let mut transport = transport_base.clone();
    transport.set_path(canonical.path());
    transport.set_query(canonical.query());
    Ok(transport)
}

fn canonical_authority(url: &Url) -> Result<String> {
    let host = url.host_str().context("gateway URL has no host")?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn bounded_error(message: &str) -> String {
    message.chars().take(512).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_only_transport_origin() {
        let canonical = Url::parse("https://veoveo.example/recordings/operator/layers").unwrap();
        let transport = transport_url(
            &Url::parse("https://veoveo.example/").unwrap(),
            &Url::parse("http://mcp-gateway:8788/").unwrap(),
            &canonical,
        )
        .unwrap();
        assert_eq!(
            transport.as_str(),
            "http://mcp-gateway:8788/recordings/operator/layers"
        );
    }
}
