//! Retry-safe file streaming from Recording Hub through the Gateway into the
//! authoritative Artifact plane.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, ensure};
use reqwest::header::{HOST, HeaderMap, HeaderValue};
use secrecy::ExposeSecret as _;
use url::Url;
use veoveo_artifact_contract::{ArtifactId, ArtifactMetadata};
use veoveo_artifact_contract::{PutArtifactRequest, StreamArtifactRequest};
use veoveo_recording_contract::RecordingProducerScope;
use veoveo_recording_forwarder::{
    config::ClientAssertionAlgorithm,
    oauth::{OAuthTokenProvider, OAuthTokenProviderConfig},
};
use veoveo_recording_store::RecordingLayerId;

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
    context: veoveo_recording_contract::RecordingPublisherContext,
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
        let context = veoveo_recording_contract::RecordingPublisherContext {
            client_id: config.client_id.clone(),
            profile: config.profile.clone(),
            protected_resource: config.protected_resource.as_str().to_owned(),
        };
        let tokens = OAuthTokenProvider::new(OAuthTokenProviderConfig {
            http: http.clone(),
            token_endpoint,
            token_transport_endpoint,
            protected_resource: config.protected_resource,
            client_id: config.client_id,
            scope: RecordingProducerScope::Publish,
            key_id: config.key_id,
            algorithm: config.algorithm,
            private_key_pem_file: config.private_key_pem_file,
        })?;
        Ok(Self {
            http,
            endpoint,
            tokens,
            context,
        })
    }

    pub fn publication_context(&self) -> &veoveo_recording_contract::RecordingPublisherContext {
        &self.context
    }

    pub async fn publish(
        &self,
        layer_id: RecordingLayerId,
        artifact: PutArtifactRequest,
        path: &Path,
        expected_byte_len: u64,
        expected_sha256: &veoveo_types::Sha256Digest,
    ) -> Result<ArtifactMetadata> {
        let artifact_id = ArtifactId::try_from(layer_id.as_uuid())
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
        expected_sha256: &veoveo_types::Sha256Digest,
    ) -> Result<ArtifactMetadata> {
        let request = StreamArtifactRequest {
            artifact_id,
            artifact,
            expected_byte_len,
            expected_sha256: veoveo_artifact_contract::UploadSha256::parse(expected_sha256.hex())?,
        };
        let descriptor = serde_json::to_string(&request)?;
        ensure!(
            descriptor.len() <= veoveo_artifact_contract::MAX_ARTIFACT_PUT_DESCRIPTOR_BYTES,
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
        admit_publication_response(
            &metadata,
            artifact_id,
            expected_byte_len,
            &request.artifact.metadata,
        )?;
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

fn admit_publication_response(
    metadata: &ArtifactMetadata,
    artifact_id: ArtifactId,
    expected_byte_len: u64,
    expected_metadata: &serde_json::Value,
) -> Result<()> {
    ensure!(
        metadata.artifact_uri == artifact_id.plane_uri()
            && metadata.byte_len == expected_byte_len
            && metadata.download_url.is_none()
            && &metadata.metadata == expected_metadata,
        "Artifact service returned mismatched recording layer metadata"
    );
    Ok(())
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
    #[test]
    fn publication_response_admits_only_expected_occurrence_length_and_plane_location() {
        let id = ArtifactId::new();
        let metadata = ArtifactMetadata {
            byte_len: 3,
            mime_type: None,
            filename: None,
            artifact_uri: id.plane_uri(),
            download_url: None,
            created_at: chrono::Utc::now(),
            release_state: Default::default(),
            compliance: Default::default(),
            metadata: serde_json::json!({}),
        };
        let mut owned = metadata.clone();
        owned.metadata =
            serde_json::to_value(veoveo_recording_contract::RecordingCaptureMetadata {
                recording_id: veoveo_recording_contract::RecordingId::new(),
                dataset_id: veoveo_recording_contract::RecordingDatasetId::new(),
                layer_kind: veoveo_recording_contract::RecordingLayerKind::Capture,
                schema_digest: veoveo_types::Sha256Digest::from_bytes([3; 32]),
            })
            .unwrap();
        admit_publication_response(&owned, id, 3, &owned.metadata).unwrap();
        for (current, retired) in [
            ("recordingId", "recording_id"),
            ("datasetId", "dataset_id"),
            ("layerKind", "layer_kind"),
            ("schemaDigest", "schema_digest"),
        ] {
            for mode in ["replacement", "mixed", "conflicting"] {
                let mut changed = owned.clone();
                let value = changed.metadata.get(current).unwrap().clone();
                changed.metadata[retired] = if mode == "conflicting" {
                    serde_json::json!("retired-conflict")
                } else {
                    value
                };
                if mode == "replacement" {
                    changed.metadata.as_object_mut().unwrap().remove(current);
                }
                assert!(
                    admit_publication_response(&changed, id, 3, &owned.metadata).is_err(),
                    "{current}/{mode}"
                );
            }
        }
        let mut changed = owned.clone();
        changed.metadata["recordingId"] =
            serde_json::json!(veoveo_recording_contract::RecordingId::new());
        assert!(admit_publication_response(&changed, id, 3, &owned.metadata).is_err());
        // The real response supplies no declared digest; integrity is checked
        // by the Artifact streaming request, not an invented metadata field.
        admit_publication_response(&metadata, id, 3, &metadata.metadata).unwrap();
        let mut wrong = metadata.clone();
        wrong.artifact_uri = ArtifactId::new().plane_uri();
        assert!(admit_publication_response(&wrong, id, 3, &metadata.metadata).is_err());
        let mut wrong = metadata.clone();
        wrong.byte_len = 4;
        assert!(admit_publication_response(&wrong, id, 3, &metadata.metadata).is_err());
        let mut wrong = metadata.clone();
        wrong.download_url = Some("https://unexpected.example/file".into());
        assert!(admit_publication_response(&wrong, id, 3, &metadata.metadata).is_err());
        let wrong = metadata
            .clone()
            .presented_under_scheme(&veoveo_types::ResourceScheme::parse("recording").unwrap());
        assert!(admit_publication_response(&wrong, id, 3, &metadata.metadata).is_err());
    }
}
