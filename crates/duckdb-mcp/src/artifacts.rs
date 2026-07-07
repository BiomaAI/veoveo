//! Artifact access for duckdb-mcp, backed by the shared artifact plane.
//!
//! duckdb no longer owns a private bucket or artifact metadata tables. It calls
//! the central artifact service with the caller's forwarded gateway identity;
//! the plane stamps tenant/owner, records the grant ledger, encrypts per tenant,
//! and enforces every read/write. See `TECH_DESIGN.md`, "shared artifact plane".

use anyhow::{Result, anyhow};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    AccessLevel, ArtifactMetadata, ArtifactObject, ArtifactPlane, ArtifactPlaneError, ArtifactPut,
    ArtifactSha256, PlaneCaller, PutArtifactRequest,
};

/// Thin handle to the shared artifact plane. Cloneable; wraps a pooled client.
#[derive(Clone)]
pub struct ArtifactRepository {
    plane: HttpArtifactPlane,
}

impl ArtifactRepository {
    /// Connect to the artifact service at `service_url` (e.g.
    /// `http://artifact-service:8790`).
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

    /// Store bytes on the plane on the caller's behalf. Tenant and owner are
    /// stamped by the service from the verified identity, so any client-supplied
    /// `compliance.tenant_id` / `owner_id` are intentionally ignored here.
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
            .map_err(plane_err)
    }

    /// Fetch bytes + metadata if the caller may read them. `Ok(None)` for a
    /// missing artifact; a denial surfaces as an error so it is never silently
    /// treated as absent.
    pub async fn get(
        &self,
        caller: &PlaneCaller,
        sha256: &str,
    ) -> Result<Option<ArtifactObject>> {
        let sha = parse_sha(sha256)?;
        match self.plane.get(caller, &sha, AccessLevel::Read).await {
            Ok(object) => Ok(Some(object)),
            Err(ArtifactPlaneError::NotFound) => Ok(None),
            Err(other) => Err(plane_err(other)),
        }
    }

    /// Metadata only, gated at read.
    pub async fn head(
        &self,
        caller: &PlaneCaller,
        sha256: &str,
    ) -> Result<Option<ArtifactMetadata>> {
        let sha = parse_sha(sha256)?;
        match self.plane.head(caller, &sha).await {
            Ok(metadata) => Ok(Some(metadata)),
            Err(ArtifactPlaneError::NotFound) => Ok(None),
            Err(other) => Err(plane_err(other)),
        }
    }

    /// Resolve a neutral `artifact://{sha}` plane URI to bytes on the caller's
    /// behalf — the cross-server input path (P3).
    pub async fn resolve(&self, caller: &PlaneCaller, uri: &str) -> Result<ArtifactObject> {
        self.plane.resolve(caller, uri).await.map_err(plane_err)
    }
}

fn parse_sha(sha256: &str) -> Result<ArtifactSha256> {
    ArtifactSha256::new(sha256).map_err(|e| anyhow!("invalid artifact sha: {e}"))
}

fn plane_err(err: ArtifactPlaneError) -> anyhow::Error {
    anyhow!("artifact plane error: {err}")
}
