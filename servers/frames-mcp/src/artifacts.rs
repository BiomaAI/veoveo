//! Artifact access for Frames MCP, backed by the shared artifact plane.

use anyhow::{Result, anyhow};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    AccessLevel, ArtifactId, ArtifactMetadata, ArtifactObject, ArtifactPlane, ArtifactPlaneError,
    ArtifactPut, ArtifactWriteIdempotencyKey, IssueArtifactWriteCapabilityRequest,
    IssuedArtifactWriteCapability, PlaneCaller, PutArtifactRequest,
    RedeemArtifactWriteCapabilityRequest,
};

const SCHEME: &str = "frames";

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

    pub async fn issue_write_capability(
        &self,
        caller: &PlaneCaller,
        request: &IssueArtifactWriteCapabilityRequest,
    ) -> Result<IssuedArtifactWriteCapability> {
        self.plane
            .issue_write_capability(caller, request)
            .await
            .map_err(plane_err)
    }

    pub async fn put_with_capability(
        &self,
        capability: &IssuedArtifactWriteCapability,
        idempotency_key: ArtifactWriteIdempotencyKey,
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
        let redemption = RedeemArtifactWriteCapabilityRequest {
            capability_id: capability.capability_id,
            task_id: capability.task_id.clone(),
            idempotency_key,
            artifact: request,
        };
        self.plane
            .redeem_write_capability(&capability.secret, &redemption, artifact.bytes)
            .await
            .map(|metadata| metadata.presented_under_scheme(SCHEME))
            .map_err(plane_err)
    }

    pub async fn get(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
    ) -> Result<Option<ArtifactObject>> {
        match self.plane.get(caller, artifact_id, AccessLevel::Read).await {
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
        artifact_id: &ArtifactId,
    ) -> Result<Option<ArtifactMetadata>> {
        match self.plane.head(caller, artifact_id).await {
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

fn plane_err(err: ArtifactPlaneError) -> anyhow::Error {
    anyhow!("artifact plane error: {err}")
}
