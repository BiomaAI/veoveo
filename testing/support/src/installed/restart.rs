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

mod crash;
mod drain;
mod group;
mod handoff;
pub use crash::{CrashIdentity, CrashReceipt, CrashTarget, CrashWatch};
pub use drain::{DrainProfile, DrainReceipt, SelectedDrainIdentity, SelectedDrainTarget};
pub use group::{
    ContainerExit, DrainGroupProgress, DrainGroupReceipt, DrainGroupState,
    ReplacementContainerIdentity, SelectedDrainGroup,
};
pub use handoff::{
    HandoffFixture, HandoffObjectKind, HandoffObservation, HandoffObserver, HandoffPhase,
    HandoffReceipt, HandoffResourceVersion,
};

#[derive(Clone)]
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

    /// Admit one selected NVIDIA server Pod before any restart mutation.
    pub async fn select_drain_target(
        &self,
        pod_name: &str,
        profile: DrainProfile,
    ) -> Result<SelectedDrainTarget> {
        self.select_drain_snapshot(pod_name, profile)
            .await
            .map(|(selected, _)| selected)
    }

    async fn select_drain_snapshot(
        &self,
        pod_name: &str,
        profile: DrainProfile,
    ) -> Result<(SelectedDrainTarget, drain::Pod)> {
        drain::name(pod_name)?;
        let before = self.deployment().await?;
        before.require_ready()?;
        ensure!(
            before.spec.replicas == 1,
            "selected process drain requires a single declared replica"
        );
        ensure!(
            before
                .spec
                .selector
                .match_labels
                .get("app.kubernetes.io/component")
                == Some(&self.component),
            "selected Deployment belongs to another component"
        );
        drain::version(&before.metadata.resource_version)?;
        let namespace: drain::Namespace = serde_json::from_slice(
            &self
                .command(&["get", "namespace", &self.namespace, "-o", "json"])
                .await?,
        )
        .map_err(|_| anyhow::anyhow!("invalid namespace admission response"))?;
        ensure!(
            namespace.metadata.name == self.namespace
                && namespace.metadata.deletion_timestamp.is_none(),
            "selected namespace is replaced or deleting"
        );
        let pod: drain::Pod = serde_json::from_slice(
            &self
                .command(&["get", "pod", pod_name, "-o", "json"])
                .await?,
        )
        .map_err(|_| anyhow::anyhow!("invalid selected Pod admission response"))?;
        let grace = pod.admit(&self.namespace, &profile)?;
        let status = pod.selected_status(&profile)?;
        let container_id = status.container_id.clone();
        let restart_count = status.restart_count;
        let owner = pod
            .metadata
            .owner_references
            .iter()
            .find(|owner| owner.kind == "ReplicaSet" && owner.controller == Some(true))
            .context("selected Pod has no controlling ReplicaSet")?;
        drain::name(&owner.name)?;
        let replica_set: drain::ReplicaSet = serde_json::from_slice(
            &self
                .command(&["get", "replicaset", &owner.name, "-o", "json"])
                .await?,
        )
        .map_err(|_| anyhow::anyhow!("invalid ReplicaSet admission response"))?;
        ensure!(!namespace.metadata.uid.is_nil(), "namespace UID is absent");
        let deployment_uid = uuid::Uuid::parse_str(&before.metadata.uid)
            .map_err(|_| anyhow::anyhow!("invalid Deployment UID"))?;
        ensure!(
            !deployment_uid.is_nil() && !owner.uid.is_nil(),
            "workload owner UID is absent"
        );
        ensure!(
            replica_set.metadata.uid == owner.uid
                && replica_set.metadata.namespace.as_deref() == Some(&self.namespace)
                && replica_set
                    .metadata
                    .owner_references
                    .iter()
                    .any(|owner| owner.kind == "Deployment"
                        && owner.controller == Some(true)
                        && owner.uid == deployment_uid
                        && owner.name == self.deployment),
            "selected Pod is not owned by the admitted Deployment"
        );
        let selected = SelectedDrainTarget {
            namespace: self.namespace.clone(),
            namespace_uid: namespace.metadata.uid,
            deployment: self.deployment.clone(),
            deployment_uid,
            deployment_version: before.metadata.resource_version,
            generation: before.metadata.generation,
            pod: pod.metadata.name.clone(),
            pod_uid: pod.metadata.uid,
            pod_version: pod.metadata.resource_version.clone(),
            container_id,
            restart_count,
            profile,
            grace,
            annotations: before.spec.template.metadata.annotations,
        };
        Ok((selected, pod))
    }

    /// One UID/resourceVersion-fenced mutation. Actual container exit is required.
    pub async fn restart_with_drain(&self, selected: &SelectedDrainTarget) -> Result<DrainReceipt> {
        ensure!(
            selected.namespace == self.namespace && selected.deployment == self.deployment,
            "drain target belongs to another workload"
        );
        tokio::time::timeout(selected.profile.deadline + Duration::from_secs(75), async {
            let namespace: drain::Namespace = serde_json::from_slice(
                &self
                    .command(&["get", "namespace", &self.namespace, "-o", "json"])
                    .await?,
            )
            .map_err(|_| anyhow::anyhow!("invalid namespace fence response"))?;
            ensure!(
                namespace.metadata.uid == selected.namespace_uid
                    && namespace.metadata.deletion_timestamp.is_none(),
                "selected namespace changed before restart"
            );
            let mut watch = drain::PodWatch::start(&self.context, &self.namespace, &selected.pod)?;
            let mut observation = drain::DrainObservation::new(selected.clone());
            let initial = tokio::time::timeout(Duration::from_secs(10), watch.next())
                .await
                .context("selected old Pod watch admission exceeded ten seconds")??;
            observation.initial(initial)?;
            // The native watch has delivered its initial selected object before effects.
            let deadline = tokio::time::Instant::now() + selected.profile.deadline;
            tokio::time::timeout_at(deadline, async {
                observation.dispatched(chrono::SubsecRound::trunc_subsecs(chrono::Utc::now(), 0));
                self.fenced_patch(selected).await?;
                loop {
                    if observation.observe(watch.next().await?)? {
                        break;
                    }
                }
                Ok::<_, anyhow::Error>(())
            })
            .await
            .context("selected old container exit was not observed within the drain deadline")??;
            drop(watch); // Owned process-group teardown also covers every earlier error/cancellation.
            let resource = format!("deployment/{}", self.deployment);
            self.command(&["rollout", "status", &resource, "--timeout=55s"])
                .await?;
            let after = self.deployment().await?;
            after.require_ready()?;
            ensure!(
                after.metadata.uid == selected.deployment_uid.to_string()
                    && after.metadata.generation > selected.generation
                    && after.spec.replicas == 1,
                "replacement Deployment identity or readiness differs"
            );
            self.require_public_route().await?;
            observation.receipt(after.metadata.generation)
        })
        .await
        .context("installed selected-container restart exceeded its lifecycle deadline")?
    }

    async fn fenced_patch(&self, selected: &SelectedDrainTarget) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        let mut annotations = selected.annotations.clone();
        annotations.insert(
            "kubectl.kubernetes.io/restartedAt".into(),
            chrono::Utc::now().to_rfc3339(),
        );
        let patch = serde_json::to_vec(&serde_json::json!([
            {"op":"test","path":"/metadata/uid","value":selected.deployment_uid},
            {"op":"test","path":"/metadata/resourceVersion","value":selected.deployment_version},
            {"op":"add","path":"/spec/template/metadata/annotations","value":annotations}
        ]))?;
        ensure!(
            patch.len() <= 65536,
            "selected Deployment annotation patch exceeds 64 KiB"
        );
        let mut command = Command::new("kubectl");
        command
            .args([
                "--context",
                &self.context,
                "--namespace",
                &self.namespace,
                "--request-timeout=10s",
                "patch",
                "deployment",
                &self.deployment,
                "--type=json",
                "--patch-file=/dev/stdin",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = crate::spawn_async(command)?;
        let mut input = child
            .stdin
            .take()
            .context("native restart patch input unavailable")?;
        input.write_all(&patch).await?;
        input.shutdown().await?;
        drop(input);
        let result = child.wait_with_output().await?;
        ensure!(
            result.status.success(),
            "fenced Deployment patch failed with {}; restart outcome is unqualified and must not be redispatched",
            result.status
        );
        Ok(())
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
        let mut command = Command::new("kubectl");
        command
            .args(["--context", &self.context, "--namespace", &self.namespace])
            .args(arguments)
            .kill_on_drop(true);
        let result = crate::output_async(command, Duration::from_secs(75))
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
    #[serde(rename = "resourceVersion")]
    resource_version: String,
}
#[derive(Deserialize)]
struct DeploymentSpec {
    replicas: u32,
    selector: Selector,
    template: PodTemplate,
}
#[derive(Deserialize)]
struct PodTemplate {
    metadata: TemplateMetadata,
}
#[derive(Deserialize)]
struct TemplateMetadata {
    #[serde(default)]
    annotations: BTreeMap<String, String>,
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
