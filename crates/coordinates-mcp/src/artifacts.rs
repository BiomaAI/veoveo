//! Artifact access for coordinates-mcp, backed by the shared artifact plane.

use anyhow::{Result, anyhow};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    AccessLevel, ArtifactMetadata, ArtifactObject, ArtifactPlane, ArtifactPlaneError, ArtifactPut,
    ArtifactSha256, PlaneCaller, PutArtifactRequest,
};

const SCHEME: &str = "coordinates";

#[derive(Clone)]
pub struct ArtifactRepository {
    plane: HttpArtifactPlane,
}

impl ArtifactRepository {
    pub fn new(service_url: impl Into<String>) -> Self {
        Self {
            plane: HttpArtifactPlane::new(service_url),
        }
    }

    pub fn with_client(service_url: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            plane: HttpArtifactPlane::with_client(service_url, http),
        }
    }

    pub async fn put(
        &self,
        caller: &PlaneCaller,
        artifact: ArtifactPut,
    ) -> Result<ArtifactMetadata> {
        let request = PutArtifactRequest {
            mime_type: artifact.mime_type,
            filename: artifact.filename,
            classification: artifact.compliance.classification,
            data_labels: artifact.compliance.data_labels,
            retention_expires_at: artifact.compliance.retention_expires_at,
            metadata: artifact.metadata,
        };
        self.plane
            .put(caller, request, artifact.bytes)
            .await
            .map(|m| m.presented_under_scheme(SCHEME))
            .map_err(plane_err)
    }

    pub async fn get(&self, caller: &PlaneCaller, sha256: &str) -> Result<Option<ArtifactObject>> {
        let sha = parse_sha(sha256)?;
        match self.plane.get(caller, &sha, AccessLevel::Read).await {
            Ok(mut object) => {
                object.metadata = object.metadata.presented_under_scheme(SCHEME);
                Ok(Some(object))
            }
            Err(ArtifactPlaneError::NotFound) => Ok(None),
            Err(other) => Err(plane_err(other)),
        }
    }

    pub async fn head(
        &self,
        caller: &PlaneCaller,
        sha256: &str,
    ) -> Result<Option<ArtifactMetadata>> {
        let sha = parse_sha(sha256)?;
        match self.plane.head(caller, &sha).await {
            Ok(metadata) => Ok(Some(metadata.presented_under_scheme(SCHEME))),
            Err(ArtifactPlaneError::NotFound) => Ok(None),
            Err(other) => Err(plane_err(other)),
        }
    }

    pub async fn resolve(&self, caller: &PlaneCaller, uri: &str) -> Result<ArtifactObject> {
        let mut object = self.plane.resolve(caller, uri).await.map_err(plane_err)?;
        object.metadata = object.metadata.presented_under_scheme(SCHEME);
        Ok(object)
    }
}

fn parse_sha(sha256: &str) -> Result<ArtifactSha256> {
    ArtifactSha256::new(sha256).map_err(|e| anyhow!("invalid artifact sha: {e}"))
}

fn plane_err(err: ArtifactPlaneError) -> anyhow::Error {
    anyhow!("artifact plane error: {err}")
}
