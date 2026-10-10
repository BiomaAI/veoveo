//! Closed installed inputs and read-only controller/template admission.
use super::*;
use std::collections::BTreeMap;
use veoveo_agent_runtime::contract::authoring::{
    RuntimeTemplate, runtime_config_revision, runtime_template_revision,
};
use veoveo_modules::{CredentialRevision, InstallationGeneration, ModulePlanDocument};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Input {
    pub schema: String,
    pub caller_token_file: PathBuf,
    pub work_context: WorkContextId,
    pub owner: Uuid,
    pub controller: Controller,
    pub module_plan: ModulePlan,
    pub template: RuntimeTemplate,
    pub model: wire::ModelReference,
    pub definition: wire::AgentDefinitionId,
    pub instance: wire::AgentManagedInstanceId,
    pub timeout_seconds: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Controller {
    pub namespace: String,
    pub deployment: String,
    pub uid: Uuid,
    pub image: String,
    pub config_map: String,
    pub config_sha256: Sha256Digest,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ModulePlan {
    pub gateway_deployment: String,
    pub gateway_uid: Uuid,
    pub config_map: String,
    pub sha256: Sha256Digest,
    pub generation: InstallationGeneration,
    pub credential_revision: CredentialRevision,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagerConfig {
    namespace: String,
    gateway_url: String,
    database_credential_revision: CredentialRevision,
    templates: Vec<RuntimeTemplate>,
    models: Vec<wire::ModelConnection>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IdleManifest {
    agent: serde::de::IgnoredAny,
    model: serde::de::IgnoredAny,
    gateway: serde::de::IgnoredAny,
    episode: IdleEpisode,
    schedule: IdleSchedule,
    #[serde(default)]
    resource_subscriptions: Vec<ResourceUri>,
    preamble: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IdleEpisode {
    #[serde(default)]
    max_turns: Option<u32>,
    #[serde(default)]
    request_timeout_s: Option<u32>,
    #[serde(default)]
    task_deadline_s: Option<u32>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IdleSchedule {
    heartbeat_interval_s: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Object {
    metadata: Metadata,
    #[serde(default)]
    data: BTreeMap<String, String>,
    spec: Option<Spec>,
    #[serde(default)]
    immutable: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    uid: Uuid,
    #[serde(default)]
    deletion_timestamp: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    replicas: Option<u32>,
    template: PodTemplate,
}
#[derive(Deserialize)]
struct PodTemplate {
    metadata: PodMetadata,
    spec: PodSpec,
}
#[derive(Deserialize)]
struct PodMetadata {
    labels: BTreeMap<String, String>,
    annotations: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct PodSpec {
    containers: Vec<Container>,
    volumes: Vec<Volume>,
}
#[derive(Deserialize)]
struct Container {
    name: String,
    image: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Volume {
    config_map: Option<ConfigMapVolume>,
}
#[derive(Deserialize)]
struct ConfigMapVolume {
    name: String,
}

pub(super) fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 63
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && value.as_bytes()[0].is_ascii_alphanumeric()
            && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric(),
        "invalid selected Kubernetes name"
    );
    Ok(())
}
impl Input {
    pub fn validate(&self, target: &InstallationTarget) -> Result<()> {
        ensure!(
            self.schema == "veoveo.ai/agent-manager-journey-input/v1",
            "unsupported Agent Manager fixture"
        );
        let admin = target
            .administrator
            .as_ref()
            .context("installation requires administrator")?;
        ensure!(
            admin.profile == "admin" && admin.work_context == self.work_context.as_str(),
            "fixture requires installed admin profile and Work Context"
        );
        ensure!(
            (180..=240).contains(&self.timeout_seconds),
            "journey deadline must be 180..240 seconds including cleanup"
        );
        for selected in [
            &self.controller.namespace,
            &self.controller.deployment,
            &self.controller.config_map,
            &self.module_plan.config_map,
            &self.module_plan.gateway_deployment,
            &self.template.workload.namespace,
            &self.template.workload.config_map,
        ] {
            name(selected)?;
        }
        ensure!(
            self.controller.namespace == self.template.workload.namespace
                && self.controller.namespace != target.kubernetes.namespace,
            "controller/template namespace differs or is not isolated from gateway"
        );
        ensure!(
            self.template.work_contexts.contains(&self.work_context)
                && self.template.models.contains(&self.model.id)
                && self.template.tools.is_empty()
                && self.template.resource_subscriptions.is_empty()
                && self.template.parameters.is_empty(),
            "idle template must admit context/model and forbid tools, subscriptions and parameters"
        );
        for image in [&self.controller.image, &self.template.workload.image] {
            let (_, digest) = image
                .rsplit_once('@')
                .context("image requires pinned digest")?;
            Sha256Digest::parse(digest).map_err(|_| anyhow!("invalid selected image digest"))?;
        }
        Ok(())
    }
    pub fn template_revision(&self) -> Sha256Digest {
        runtime_template_revision(&self.template)
    }
    pub async fn admit(&self, target: &InstallationTarget) -> Result<()> {
        self.validate(target)?;
        let deployment: Object = get(
            target,
            &self.controller.namespace,
            "deployment",
            &self.controller.deployment,
        )
        .await?;
        ensure!(
            deployment.metadata.uid == self.controller.uid
                && deployment.metadata.deletion_timestamp.is_none(),
            "controller identity changed"
        );
        let spec = deployment
            .spec
            .context("controller Deployment spec missing")?;
        ensure!(
            spec.replicas == Some(1)
                && spec
                    .template
                    .metadata
                    .labels
                    .get("app.kubernetes.io/component")
                    .map(String::as_str)
                    == Some("agent-manager")
                && spec.template.spec.containers.len() == 1
                && spec.template.spec.containers[0].name == "manager"
                && spec.template.spec.containers[0].image == self.controller.image,
            "requires one admitted existing Manager controller"
        );
        ensure!(
            spec.template.spec.volumes.iter().any(|v| v
                .config_map
                .as_ref()
                .is_some_and(|c| c.name == self.controller.config_map)),
            "Manager configuration mount differs"
        );
        ensure!(
            spec.template
                .metadata
                .annotations
                .get("veoveo.ai/database-credential-revision")
                .map(String::as_str)
                == Some(self.module_plan.credential_revision.as_str()),
            "controller credential revision differs"
        );
        let cm: Object = get(
            target,
            &self.controller.namespace,
            "configmap",
            &self.controller.config_map,
        )
        .await?;
        let config = cm
            .data
            .get("manager.json")
            .context("Manager config missing")?;
        ensure!(
            digest(config.as_bytes()) == self.controller.config_sha256,
            "Manager config digest differs"
        );
        let config: ManagerConfig =
            serde_json::from_str(config).map_err(|_| anyhow!("invalid Manager configuration"))?;
        ensure!(
            config.namespace == self.controller.namespace
                && url::Url::parse(&config.gateway_url).ok().as_ref()
                    == Some(&target.public_base_url)
                && config.database_credential_revision == self.module_plan.credential_revision
                && config.templates.iter().any(|t| t == &self.template)
                && config
                    .models
                    .iter()
                    .any(|m| m.id == self.model.id && m.revision() == self.model.revision),
            "Manager installation/template/model revision differs"
        );
        ensure!(
            target
                .expected_deployments
                .contains(&self.module_plan.gateway_deployment),
            "Gateway selection is not declared by installation"
        );
        let gateway: Object = get(
            target,
            &target.kubernetes.namespace,
            "deployment",
            &self.module_plan.gateway_deployment,
        )
        .await?;
        ensure!(
            gateway.metadata.uid == self.module_plan.gateway_uid
                && gateway.metadata.deletion_timestamp.is_none(),
            "Gateway identity changed"
        );
        let gateway = gateway.spec.context("Gateway spec missing")?.template;
        ensure!(
            gateway
                .metadata
                .labels
                .get("app.kubernetes.io/component")
                .map(String::as_str)
                == Some("gateway")
                && gateway.spec.volumes.iter().any(|v| v
                    .config_map
                    .as_ref()
                    .is_some_and(|c| c.name == self.module_plan.config_map))
                && gateway
                    .metadata
                    .annotations
                    .get("checksum/module-plan")
                    .map(String::as_str)
                    == self.module_plan.sha256.as_str().strip_prefix("sha256:"),
            "current Gateway module plan mount/revision differs"
        );
        let plan: Object = get(
            target,
            &target.kubernetes.namespace,
            "configmap",
            &self.module_plan.config_map,
        )
        .await?;
        let bytes = plan.data.get("plan.json").context("module plan missing")?;
        ensure!(
            digest(bytes.as_bytes()) == self.module_plan.sha256,
            "module plan digest differs"
        );
        let plan: ModulePlanDocument =
            serde_json::from_str(bytes).map_err(|_| anyhow!("invalid current module plan"))?;
        ensure!(
            plan.enabled().iter().any(|m| m.as_str() == "agents"),
            "current plan does not enable Agents lane"
        );
        ensure!(
            plan.generation() == self.module_plan.generation
                && plan.credential_revision() == &self.module_plan.credential_revision,
            "module plan identity differs"
        );
        let cm: Object = get(
            target,
            &self.template.workload.namespace,
            "configmap",
            &self.template.workload.config_map,
        )
        .await?;
        ensure!(
            cm.immutable
                && runtime_config_revision(&cm.data) == self.template.workload.config_digest,
            "idle ConfigMap digest differs"
        );
        admit_idle(&cm.data, self.timeout_seconds)
    }
}
pub(super) fn admit_idle(data: &BTreeMap<String, String>, cap: u64) -> Result<()> {
    ensure!(
        data.len() == 1,
        "idle template permits only manifest.json, no migration/wake producers"
    );
    let manifest: IdleManifest = serde_json::from_str(
        data.get("manifest.json").context("idle manifest missing")?,
    )
    .map_err(|_| anyhow!("idle manifest profile refuses unsupported fields/prompt/producers"))?;
    ensure!(
        manifest.schedule.heartbeat_interval_s > cap && manifest.resource_subscriptions.is_empty(),
        "idle heartbeat must exceed entire operation and cleanup cap; subscriptions forbidden"
    );
    let _ = (
        manifest.agent,
        manifest.model,
        manifest.gateway,
        manifest.episode.max_turns,
        manifest.episode.request_timeout_s,
        manifest.episode.task_deadline_s,
        manifest.preamble,
    );
    Ok(())
}
async fn get<T: serde::de::DeserializeOwned>(
    target: &InstallationTarget,
    namespace: &str,
    kind: &str,
    name: &str,
) -> Result<T> {
    let mut command = tokio::process::Command::new("kubectl");
    command.args([
        "--context",
        &target.kubernetes.context,
        "--namespace",
        namespace,
        "get",
        kind,
        name,
        "--output=json",
        "--request-timeout=10s",
    ]);
    let output = veoveo_testing_support::output_async(command, Duration::from_secs(15))
        .await
        .map_err(|_| anyhow!("selected Kubernetes admission failed"))?;
    ensure!(
        output.status.success() && output.stdout.len() <= 1024 * 1024,
        "selected Kubernetes admission refused or exceeded byte cap"
    );
    serde_json::from_slice(&output.stdout)
        .map_err(|_| anyhow!("invalid selected Kubernetes observation"))
}
