use super::*;
use veoveo_mcp_contract::{
    ArtifactReadAuthority, ArtifactReadCapabilityId, ArtifactReadCapabilityScope, ArtifactTaskId,
    IssueArtifactReadCapabilityRequest, IssuedArtifactReadCapability,
};

impl HttpArtifactPlane {
    pub async fn issue_read_capability(
        &self,
        caller: &PlaneCaller,
        request: &IssueArtifactReadCapabilityRequest,
    ) -> Result<IssuedArtifactReadCapability, ArtifactPlaneError> {
        let response = self
            .http
            .post(self.url("/artifact-read-capabilities"))
            .bearer_auth(&caller.bearer_token)
            .json(request)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
        }
    }

    pub async fn read_capability_scope(
        &self,
        capability: &IssuedArtifactReadCapability,
        task_id: ArtifactTaskId,
    ) -> Result<ArtifactReadCapabilityScope, ArtifactPlaneError> {
        if task_id != capability.task_id {
            return Err(ArtifactPlaneError::Unauthenticated);
        }
        let response = self
            .http
            .get(self.url(&format!(
                "/artifact-read-capabilities/{}",
                capability.capability_id
            )))
            .query(&[("task_id", task_id.to_string())])
            .bearer_auth(capability.secret.expose_secret())
            .send()
            .await
            .map_err(transport)?;
        if !response.status().is_success() {
            return response_error(response).await;
        }
        let scope: ArtifactReadCapabilityScope = response.json().await.map_err(transport)?;
        if scope.task_id != task_id {
            return Err(ArtifactPlaneError::Unauthenticated);
        }
        Ok(scope)
    }

    pub async fn revoke_read_capability(
        &self,
        caller: &PlaneCaller,
        capability_id: ArtifactReadCapabilityId,
    ) -> Result<(), ArtifactPlaneError> {
        let response = self
            .http
            .delete(self.url(&format!("/artifact-read-capabilities/{capability_id}")))
            .bearer_auth(&caller.bearer_token)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            Ok(())
        } else {
            response_error(response).await
        }
    }

    fn read_url(
        &self,
        capability: &IssuedArtifactReadCapability,
        task_id: ArtifactTaskId,
        artifact: ArtifactId,
        operation: &str,
    ) -> Result<reqwest::Url, ArtifactPlaneError> {
        if task_id != capability.task_id {
            return Err(ArtifactPlaneError::Unauthenticated);
        }
        reqwest::Url::parse_with_params(
            &self.url(&format!(
                "/artifact-read-capabilities/{}/artifacts/{artifact}/{operation}",
                capability.capability_id
            )),
            &[("task_id", task_id.to_string())],
        )
        .map_err(transport)
    }

    pub async fn read_metadata(
        &self,
        authority: ArtifactReadAuthority<'_>,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let metadata = match authority {
            ArtifactReadAuthority::Caller(caller) => self.head(caller, &artifact_id).await?,
            ArtifactReadAuthority::Task {
                capability,
                task_id,
            } => {
                let response = self
                    .http
                    .get(self.read_url(capability, task_id, artifact_id, "meta")?)
                    .bearer_auth(capability.secret.expose_secret())
                    .send()
                    .await
                    .map_err(transport)?;
                if response.status().is_success() {
                    response.json().await.map_err(transport)?
                } else {
                    return response_error(response).await;
                }
            }
        };
        if metadata.artifact_id != artifact_id || metadata.artifact_uri != artifact_id.plane_uri() {
            return Err(ArtifactPlaneError::Transport(
                "artifact metadata identity does not match the requested occurrence".into(),
            ));
        }
        Ok(metadata)
    }

    pub async fn download_with_authority(
        &self,
        authority: ArtifactReadAuthority<'_>,
        artifact_id: ArtifactId,
    ) -> Result<AuthorizedArtifactDownload, ArtifactPlaneError> {
        match authority {
            ArtifactReadAuthority::Caller(caller) => {
                self.download(caller, &artifact_id.plane_uri()).await
            }
            ArtifactReadAuthority::Task {
                capability,
                task_id,
            } => {
                let metadata = self.read_metadata(authority, artifact_id).await?;
                let response = self
                    .http
                    .get(self.read_url(capability, task_id, artifact_id, "download")?)
                    .bearer_auth(capability.secret.expose_secret())
                    .send()
                    .await
                    .map_err(transport)?;
                if response.status().is_success() {
                    Ok(AuthorizedArtifactDownload { metadata, response })
                } else {
                    response_error(response).await
                }
            }
        }
    }
}
