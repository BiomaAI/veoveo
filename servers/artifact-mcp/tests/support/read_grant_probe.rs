//! Artifact-owned grant fixture reused by metadata and derived Reason findings.
//! A selected disposable artifact must have no pre-existing grant for this subject.
use crate::restart::DeploymentRestart;
use anyhow::{Context, Result, ensure};
use rmcp::{
    Peer, RoleClient,
    model::{CallToolRequestParams, ReadResourceRequestParams, ResourceContents},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use veoveo_artifact_contract::ArtifactId;
use veoveo_artifact_mcp::contract::{
    ArtifactGrantsOutput, ArtifactResource, GrantArtifactRequest, RevokeArtifactGrantRequest,
};
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_conformance::knowledge_probes::{KnowledgeChangeDriver, KnowledgeProbeFuture};
use veoveo_types::{AccessLevel, AccessSubject};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Administrator {
    endpoint: veoveo_types::HttpsUrl,
    token_file: std::path::PathBuf,
}

impl Administrator {
    pub async fn connect(
        &self,
        target: &veoveo_deploy_contract::InstallationTarget,
    ) -> Result<rmcp::service::RunningService<RoleClient, rmcp::model::ClientConfig>> {
        crate::installed::require_origin(&self.endpoint, target)?;
        crate::installed::connect(&self.endpoint, &self.token_file).await
    }
}

pub struct GrantProbe {
    admin: Peer<RoleClient>,
    artifact: ArtifactId,
    grantee: AccessSubject,
    restart: DeploymentRestart,
    changes: AtomicUsize,
}

impl GrantProbe {
    pub async fn new(
        admin: Peer<RoleClient>,
        artifact: ArtifactId,
        grantee: AccessSubject,
        restart: DeploymentRestart,
    ) -> Result<Self> {
        let probe = Self {
            admin,
            artifact,
            grantee,
            restart,
            changes: AtomicUsize::new(0),
        };
        ensure!(
            !probe.granted().await?,
            "fixture subject already has a grant; preserve it and choose an absent subject"
        );
        Ok(probe)
    }

    async fn granted(&self) -> Result<bool> {
        let uri = ArtifactResource::Grants(self.artifact).to_uri();
        let result = self
            .admin
            .read_resource(ReadResourceRequestParams::new(uri.as_str()))
            .await?;
        let [
            ResourceContents::TextResourceContents {
                uri: actual, text, ..
            },
        ] = result.contents.as_slice()
        else {
            anyhow::bail!("Artifact grant read requires one JSON member");
        };
        ensure!(
            actual == uri.as_str(),
            "grant response belongs to another resource"
        );
        let grants: ArtifactGrantsOutput = serde_json::from_str(text)?;
        ensure!(
            grants.artifact_id == self.artifact,
            "grant response belongs to another Artifact"
        );
        Ok(grants.grants.iter().any(|g| g.subject == self.grantee))
    }

    async fn set_grant(&self, grant: bool) -> Result<()> {
        let result: ArtifactGrantsOutput = if grant {
            call(
                &self.admin,
                "grant_access",
                &GrantArtifactRequest {
                    artifact_id: self.artifact,
                    subject: self.grantee.clone(),
                    level: AccessLevel::Read,
                },
            )
            .await?
        } else {
            call(
                &self.admin,
                "revoke_access",
                &RevokeArtifactGrantRequest {
                    artifact_id: self.artifact,
                    subject: self.grantee.clone(),
                },
            )
            .await?
        };
        let mut subject_grants = result.grants.iter().filter(|g| g.subject == self.grantee);
        let matches = if grant {
            subject_grants
                .next()
                .is_some_and(|g| g.level == AccessLevel::Read)
                && subject_grants.next().is_none()
        } else {
            subject_grants.next().is_none()
        };
        ensure!(
            result.artifact_id == self.artifact && matches,
            "Artifact grant receipt disagrees with the requested fixture mutation"
        );
        Ok(())
    }

    /// Reconcile the owned subject even after an ambiguous mutation response.
    /// This never removes an existing subject: construction checked its absence.
    pub async fn cleanup(&self) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(30), async {
            if self.granted().await? {
                self.set_grant(false).await?;
            }
            ensure!(
                !self.granted().await?,
                "temporary Artifact grant remains after cleanup"
            );
            Ok(())
        })
        .await
        .context("temporary Artifact grant cleanup exceeded thirty seconds")?
    }
}

impl KnowledgeChangeDriver for GrantProbe {
    fn mutate(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(async move {
            let count = self.changes.fetch_add(1, Ordering::SeqCst);
            self.set_grant(count.is_multiple_of(2)).await
        })
    }
    fn restart(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(self.restart.restart())
    }
}

async fn call<T: Serialize, O: DeserializeOwned>(
    peer: &Peer<RoleClient>,
    name: &str,
    input: &T,
) -> Result<O> {
    let name = GatewayToolName::from_parts(&"artifact".parse()?, &name.parse()?)?;
    let arguments = serde_json::to_value(input)?
        .as_object()
        .cloned()
        .context("Artifact request must be an object")?;
    let result = peer
        .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(arguments))
        .await?;
    ensure!(
        result.is_error != Some(true),
        "Artifact fixture mutation returned a tool error"
    );
    serde_json::from_value(
        result
            .structured_content
            .context("Artifact fixture mutation omitted structured output")?,
    )
    .context("decode Artifact grant receipt")
}
