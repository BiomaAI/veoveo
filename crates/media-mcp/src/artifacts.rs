use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use object_store::{ObjectStore, ObjectStoreExt, aws::AmazonS3Builder, path::Path};
use serde_json::Value;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactObject, ArtifactPut, ArtifactStore as ArtifactStoreContract,
    ProviderUris, artifact_object_key, now_iso,
};

use crate::state::SqliteState;

#[derive(Debug, Clone)]
pub struct S3ArtifactConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub allow_http: bool,
}

#[derive(Clone)]
pub struct S3ArtifactStore {
    inner: Arc<dyn ObjectStore>,
    state: SqliteState,
    uris: ProviderUris,
    download_base_url: String,
}

impl S3ArtifactStore {
    pub fn new(
        config: S3ArtifactConfig,
        state: SqliteState,
        uris: ProviderUris,
        download_base_url: impl Into<String>,
    ) -> Result<Self> {
        let inner = AmazonS3Builder::from_env()
            .with_endpoint(config.endpoint)
            .with_bucket_name(config.bucket)
            .with_region(config.region)
            .with_allow_http(config.allow_http)
            .with_virtual_hosted_style_request(false)
            .build()
            .context("building S3-compatible artifact store")?;

        Ok(Self {
            inner: Arc::new(inner),
            state,
            uris,
            download_base_url: download_base_url.into(),
        })
    }

    fn object_path(sha256: &str) -> Result<Path> {
        Path::parse(&artifact_object_key(sha256)).context("building artifact object key")
    }

    fn download_url(&self, sha256: &str) -> String {
        format!(
            "{}/artifacts/{sha256}",
            self.download_base_url.trim_end_matches('/')
        )
    }
}

#[async_trait]
impl ArtifactStoreContract for S3ArtifactStore {
    async fn put(&self, artifact: ArtifactPut) -> Result<ArtifactMetadata> {
        let sha256 = hex::encode(Sha256::digest(&artifact.bytes));
        let path = Self::object_path(&sha256)?;

        if let Some(existing) = self.state.artifact(&sha256)?
            && self.inner.head(&path).await.is_ok()
        {
            return Ok(existing);
        }

        self.inner
            .put(&path, artifact.bytes.clone().into())
            .await
            .with_context(|| format!("writing artifact object {sha256}"))?;

        let metadata = ArtifactMetadata {
            sha256: sha256.clone(),
            byte_len: artifact.bytes.len() as u64,
            mime_type: artifact.mime_type,
            filename: artifact.filename,
            artifact_uri: self.uris.artifact_uri(&sha256),
            download_url: Some(self.download_url(&sha256)),
            created_at: now_iso(),
            compliance: artifact.compliance,
            metadata: match artifact.metadata {
                Value::Null => Value::Object(Default::default()),
                other => other,
            },
        };
        self.state.record_artifact(&metadata)?;
        Ok(metadata)
    }

    async fn get(&self, sha256: &str) -> Result<Option<ArtifactObject>> {
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

    async fn head(&self, sha256: &str) -> Result<Option<ArtifactMetadata>> {
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
}
