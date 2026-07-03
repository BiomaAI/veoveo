use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use object_store::{
    Attribute, Attributes, ObjectStore, ObjectStoreExt, PutOptions, aws::AmazonS3Builder,
    memory::InMemory, path::Path,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactObject, ArtifactPut, ServerResourceUris, artifact_object_key, now_utc,
};

use crate::state::DuckdbState;

#[derive(Debug, Clone)]
pub struct S3ArtifactConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub allow_http: bool,
}

#[derive(Clone)]
pub struct ArtifactRepository {
    inner: Arc<dyn ObjectStore>,
    state: DuckdbState,
    uris: ServerResourceUris,
    download_base_url: String,
}

impl ArtifactRepository {
    pub fn new(
        inner: Arc<dyn ObjectStore>,
        state: DuckdbState,
        uris: ServerResourceUris,
        download_base_url: impl Into<String>,
    ) -> Self {
        Self {
            inner,
            state,
            uris,
            download_base_url: download_base_url.into(),
        }
    }

    pub fn new_s3_compatible(
        config: S3ArtifactConfig,
        state: DuckdbState,
        uris: ServerResourceUris,
        download_base_url: impl Into<String>,
    ) -> Result<Self> {
        config.validate()?;
        let inner = AmazonS3Builder::from_env()
            .with_endpoint(config.endpoint)
            .with_bucket_name(config.bucket)
            .with_region(config.region)
            .with_allow_http(config.allow_http)
            .with_virtual_hosted_style_request(false)
            .build()
            .context("building S3-compatible artifact store")?;

        Ok(Self::new(Arc::new(inner), state, uris, download_base_url))
    }

    pub fn new_in_memory(
        state: DuckdbState,
        uris: ServerResourceUris,
        download_base_url: impl Into<String>,
    ) -> Self {
        Self::new(Arc::new(InMemory::new()), state, uris, download_base_url)
    }

    fn object_path(sha256: &str) -> Result<Path> {
        Path::parse(artifact_object_key(sha256)).context("building artifact object key")
    }

    fn download_url(&self, sha256: &str) -> String {
        format!(
            "{}/artifacts/{sha256}",
            self.download_base_url.trim_end_matches('/')
        )
    }
    pub async fn put(&self, artifact: ArtifactPut) -> Result<ArtifactMetadata> {
        let sha256 = hex::encode(Sha256::digest(&artifact.bytes));
        let path = Self::object_path(&sha256)?;

        if let Some(existing) = self.state.artifact(&sha256)?
            && self.inner.head(&path).await.is_ok()
        {
            return Ok(existing);
        }

        let mut put_options = PutOptions::default();
        if let Some(mime_type) = &artifact.mime_type {
            put_options.attributes =
                Attributes::from_iter([(Attribute::ContentType, mime_type.clone())]);
        }
        self.inner
            .put_opts(&path, artifact.bytes.clone().into(), put_options)
            .await
            .with_context(|| format!("writing artifact object {sha256}"))?;

        let metadata = ArtifactMetadata {
            sha256: sha256.clone(),
            byte_len: artifact.bytes.len() as u64,
            mime_type: artifact.mime_type,
            filename: artifact.filename,
            artifact_uri: self.uris.artifact_uri(&sha256),
            download_url: Some(self.download_url(&sha256)),
            created_at: now_utc(),
            compliance: artifact.compliance,
            metadata: match artifact.metadata {
                Value::Null => Value::Object(Default::default()),
                other => other,
            },
        };
        self.state.record_artifact(&metadata)?;
        Ok(metadata)
    }

    pub async fn get(&self, sha256: &str) -> Result<Option<ArtifactObject>> {
        let Some(metadata) = self.state.artifact(sha256)? else {
            return Ok(None);
        };
        let path = Self::object_path(sha256)?;
        let result = match self.inner.get(&path).await {
            Ok(result) => result,
            Err(object_store::Error::NotFound { .. }) => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("reading artifact object {sha256}")),
        };
        let bytes = result
            .bytes()
            .await
            .with_context(|| format!("reading artifact bytes {sha256}"))?
            .to_vec();
        Ok(Some(ArtifactObject { metadata, bytes }))
    }

    pub async fn head(&self, sha256: &str) -> Result<Option<ArtifactMetadata>> {
        let Some(metadata) = self.state.artifact(sha256)? else {
            return Ok(None);
        };
        let path = Self::object_path(sha256)?;
        match self.inner.head(&path).await {
            Ok(_) => Ok(Some(metadata)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading artifact metadata {sha256}")),
        }
    }

    pub async fn delete(&self, sha256: &str) -> Result<bool> {
        let path = Self::object_path(sha256)?;
        match self.inner.delete(&path).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
            Err(err) => {
                return Err(err).with_context(|| format!("deleting artifact object {sha256}"));
            }
        }
        Ok(self.state.delete_artifact_metadata(sha256)? > 0)
    }

    pub async fn delete_expired(&self, cutoff: DateTime<Utc>, now: DateTime<Utc>) -> Result<u64> {
        let mut deleted = 0;
        for artifact in self.state.list_artifacts()? {
            let retention_expired = artifact
                .compliance
                .retention_expires_at
                .is_some_and(|expires_at| expires_at <= now);
            if (artifact.created_at < cutoff || retention_expired)
                && self.delete(&artifact.sha256).await?
            {
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

impl S3ArtifactConfig {
    fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(&self.endpoint)
            .with_context(|| format!("parsing artifact endpoint `{}`", self.endpoint))?;
        match url.scheme() {
            "https" => Ok(()),
            "http" if self.allow_http => Ok(()),
            "http" => bail!(
                "artifact endpoint `{}` uses HTTP; pass --artifact-allow-http only for local development",
                self.endpoint
            ),
            scheme => bail!(
                "artifact endpoint `{}` uses unsupported scheme `{scheme}`",
                self.endpoint
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::S3ArtifactConfig;

    fn config(endpoint: &str, allow_http: bool) -> S3ArtifactConfig {
        S3ArtifactConfig {
            endpoint: endpoint.to_string(),
            bucket: "media-artifacts".to_string(),
            region: "us-east-1".to_string(),
            allow_http,
        }
    }

    #[test]
    fn artifact_endpoint_defaults_to_tls() {
        config("https://rustfs.example.com", false)
            .validate()
            .expect("https endpoint is valid");
    }

    #[test]
    fn artifact_http_requires_explicit_local_allowance() {
        let err = config("http://rustfs:9000", false)
            .validate()
            .expect_err("HTTP endpoint should fail closed");

        assert!(err.to_string().contains("--artifact-allow-http"));
        config("http://rustfs:9000", true)
            .validate()
            .expect("explicit local HTTP endpoint is valid");
    }
}
