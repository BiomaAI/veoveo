use anyhow::{Result, anyhow};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    AccessLevel, ArtifactId, ArtifactMetadata, ArtifactObject, ArtifactPlane, ArtifactPlaneError,
    ArtifactPut, ArtifactWriteIdempotencyKey, IssueArtifactWriteCapabilityRequest,
    IssuedArtifactWriteCapability, PlaneCaller, PutArtifactRequest,
    RedeemArtifactWriteCapabilityRequest,
};

const SCHEME: &str = "perception";

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
        let redemption = RedeemArtifactWriteCapabilityRequest {
            capability_id: capability.capability_id,
            task_id: capability.task_id.clone(),
            idempotency_key,
            artifact: PutArtifactRequest {
                mime_type: artifact.mime_type,
                filename: artifact.filename,
                classification: artifact.compliance.classification,
                data_labels: artifact.compliance.data_labels,
                retention_expires_at: artifact.compliance.retention_expires_at,
                metadata: artifact.metadata,
            },
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
            Err(error) => Err(plane_err(error)),
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
            Err(error) => Err(plane_err(error)),
        }
    }
}

fn plane_err(error: ArtifactPlaneError) -> anyhow::Error {
    anyhow!("artifact plane error: {error}")
}
