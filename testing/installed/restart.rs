//! Installed harness lifecycle. Domain owners choose the workload and mutations.
//! No installation credentials enter profiles, command arguments or reports.
use anyhow::{Context, Result, ensure};
use rmcp::{
    Peer, RoleClient, ServiceError,
    model::{
        ClientRequest, ReadResourceRequest, ReadResourceRequestParams, ResourceContents,
        ServerResult,
    },
};
use serde::Deserialize;
use std::{collections::BTreeMap, time::Duration};
use tokio::process::Command;
use veoveo_deploy_contract::InstallationTarget;
use veoveo_types::ResourceUri;

pub struct DeploymentRestart {
    context: String,
    namespace: String,
    deployment: String,
    component: String,
    caller: Peer<RoleClient>,
    readiness_uri: ResourceUri,
}

impl DeploymentRestart {
    pub fn new(
        target: &InstallationTarget,
        deployment: &str,
        component: &str,
        caller: Peer<RoleClient>,
        readiness_uri: ResourceUri,
    ) -> Result<Self> {
        target.validate()?;
        ensure!(
            target
                .expected_deployments
                .iter()
                .any(|name| name == deployment),
            "restart workload is not declared by the installation target"
        );
        Ok(Self {
            context: target.kubernetes.context.clone(),
            namespace: target.kubernetes.namespace.clone(),
            deployment: deployment.to_owned(),
            component: component.to_owned(),
            caller,
            readiness_uri,
        })
    }

    /// One rollout mutation, followed by Kubernetes' native watches. Old Pods must
    /// disappear and the same Deployment must report all replacement replicas ready.
    /// An ambiguous command failure never dispatches a second restart.
    pub async fn restart(&self) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(75), async {
            self.rollout().await?;
            self.require_public_route().await
        })
        .await
        .context("installed source restart exceeded 75 seconds")?
    }

    async fn require_public_route(&self) -> Result<()> {
        // Pod readiness precedes Service routing convergence. Admit the public
        // MCP route before K07 checks retained state or dispatches its next mutation.
        tokio::time::timeout(Duration::from_secs(10), async {
            for attempt in 1..=40 {
                // Use an explicit request: the SDK resource cache must not supply
                // a pre-restart contract, including on this driver's later restart.
                let request = ClientRequest::ReadResourceRequest(ReadResourceRequest::new(
                    ReadResourceRequestParams::new(self.readiness_uri.as_str()),
                ));
                match self.caller.send_request(request).await {
                    Ok(ServerResult::ReadResourceResult(result)) => {
                        ensure!(
                            matches!(result.contents.as_slice(),
                            [ResourceContents::TextResourceContents { uri, text, .. }]
                                if uri == self.readiness_uri.as_str() && !text.is_empty()),
                            "source readiness read did not return its declared contract"
                        );
                        eprintln!(
                            "source {} public route ready after {attempt} read attempts",
                            self.component
                        );
                        return Ok(());
                    }
                    Err(ServiceError::McpError(error))
                        if error.code == rmcp::model::ErrorCode::INTERNAL_ERROR => {}
                    Err(ServiceError::TransportSend(_) | ServiceError::TransportClosed) => {}
                    Err(error) => return Err(error.into()),
                    Ok(_) => {
                        anyhow::bail!("source readiness read returned an unexpected MCP result")
                    }
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            anyhow::bail!("source public route did not become readable in forty attempts")
        })
        .await
        .context("source public route did not become readable within ten seconds")?
    }

    async fn rollout(&self) -> Result<()> {
        let before = self.deployment().await?;
        before.require_ready()?;
        ensure!(
            before
                .spec
                .selector
                .match_labels
                .get("app.kubernetes.io/component")
                == Some(&self.component),
            "restart deployment does not select the source owner's component"
        );
        let selector = before
            .spec
            .selector
            .match_labels
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(",");
        ensure!(
            !selector.is_empty(),
            "deployment restart requires a label selector"
        );
        let pods: PodList = serde_json::from_slice(
            &self
                .command(&["get", "pods", "--selector", &selector, "-o", "json"])
                .await?,
        )?;
        ensure!(
            !pods.items.is_empty(),
            "restart deployment has no running pod fixture"
        );
        let resource = format!("deployment/{}", self.deployment);
        self.command(&["rollout", "restart", &resource]).await?;
        self.command(&["rollout", "status", &resource, "--timeout=55s"])
            .await?;
        let mut args = vec![
            "wait".to_owned(),
            "--for=delete".into(),
            "--timeout=15s".into(),
        ];
        args.extend(
            pods.items
                .iter()
                .map(|pod| format!("pod/{}", pod.metadata.name)),
        );
        let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
        self.command(&borrowed).await?;
        let after = self.deployment().await?;
        after.require_ready()?;
        ensure!(
            after.metadata.uid == before.metadata.uid
                && after.metadata.generation > before.metadata.generation
                && after.spec.replicas == before.spec.replicas,
            "restart did not preserve workload identity and replace its replicas"
        );
        Ok(())
    }

    async fn deployment(&self) -> Result<Deployment> {
        serde_json::from_slice(
            &self
                .command(&["get", "deployment", &self.deployment, "-o", "json"])
                .await?,
        )
        .context("decode installed deployment state")
    }

    async fn command(&self, arguments: &[&str]) -> Result<Vec<u8>> {
        let result = Command::new("kubectl")
            .args(["--context", &self.context, "--namespace", &self.namespace])
            .args(arguments)
            .kill_on_drop(true)
            .output()
            .await
            .context("run kubectl")?;
        // Kubernetes exec credential plugins can include private values in their
        // diagnostics. Propagate status without arbitrary process output.
        ensure!(
            result.status.success(),
            "kubectl {} failed with {}; inspect the selected workload",
            arguments[0],
            result.status
        );
        Ok(result.stdout)
    }
}

#[derive(Deserialize)]
struct Deployment {
    metadata: DeploymentMetadata,
    spec: DeploymentSpec,
    status: DeploymentStatus,
}
#[derive(Deserialize)]
struct DeploymentMetadata {
    uid: String,
    generation: u64,
}
#[derive(Deserialize)]
struct DeploymentSpec {
    replicas: u32,
    selector: Selector,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selector {
    match_labels: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentStatus {
    observed_generation: Option<u64>,
    updated_replicas: Option<u32>,
    available_replicas: Option<u32>,
    unavailable_replicas: Option<u32>,
}
impl Deployment {
    fn require_ready(&self) -> Result<()> {
        ensure!(
            self.spec.replicas > 0
                && self
                    .status
                    .observed_generation
                    .is_some_and(|g| g >= self.metadata.generation)
                && self.status.updated_replicas == Some(self.spec.replicas)
                && self.status.available_replicas == Some(self.spec.replicas)
                && self.status.unavailable_replicas.unwrap_or(0) == 0,
            "source deployment does not have every requested replica ready"
        );
        Ok(())
    }
}
#[derive(Deserialize)]
struct PodList {
    items: Vec<Pod>,
}
#[derive(Deserialize)]
struct Pod {
    metadata: PodMetadata,
}
#[derive(Deserialize)]
struct PodMetadata {
    name: String,
}
