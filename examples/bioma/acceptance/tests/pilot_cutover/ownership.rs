//! Read-only Kubernetes gates for the installation's retained-resource transfer.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, de::DeserializeOwned};
use std::{collections::BTreeMap, fs::File, path::Path, process::Command};
use veoveo_bioma_acceptance::pilot_cutover::PilotAdoption;
use veoveo_mcp_contract::agent_management::RuntimeTemplate;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema: String,
    source_namespace: String,
    volumes: Vec<Volume>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Volume {
    agent: String,
    volume: String,
    source_path: String,
}
#[derive(Deserialize)]
struct Metadata {
    name: String,
    namespace: Option<String>,
    uid: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Resource<S> {
    spec: S,
}
#[derive(Deserialize)]
struct Release {
    #[serde(default)]
    suspend: bool,
}
#[derive(Deserialize)]
struct Deployment {
    replicas: u32,
}
#[derive(Deserialize)]
struct PodList {
    items: Vec<Pod>,
}
#[derive(Deserialize)]
struct Pod {
    spec: PodSpec,
}
#[derive(Deserialize)]
struct PodSpec {
    #[serde(default)]
    volumes: Vec<PodVolume>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PodVolume {
    persistent_volume_claim: Option<PodClaim>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PodClaim {
    claim_name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pv {
    persistent_volume_reclaim_policy: String,
    claim_ref: Metadata,
    local: LocalPath,
}
#[derive(Deserialize)]
struct LocalPath {
    path: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pvc {
    volume_name: String,
    access_modes: Vec<String>,
    storage_class_name: String,
    resources: Requests,
}
#[derive(Deserialize)]
struct Requests {
    requests: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Claim {
    metadata: Metadata,
    spec: Pvc,
    status: Phase,
}
#[derive(Deserialize)]
struct Phase {
    phase: String,
}
#[derive(Deserialize)]
struct Secret {
    metadata: Metadata,
    #[serde(default)]
    immutable: bool,
    #[serde(rename = "type")]
    kind: String,
    data: BTreeMap<String, String>,
}
fn get<T: DeserializeOwned>(namespace: &str, kind: &str, name: &str) -> Result<T> {
    let mut command = Command::new("kubectl");
    command.args(["--request-timeout=15s", "-n", namespace, "get", kind]);
    if !name.is_empty() {
        command.arg(name);
    }
    let output = command.args(["-o", "json"]).output()?;
    // Error bodies may echo Secret contents. Keep diagnostics to resource names.
    ensure!(
        output.status.success(),
        "cannot read {namespace}/{kind}/{name}"
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[derive(Clone, Copy)]
enum Stage {
    Transfer,
    Installed,
}
#[derive(Deserialize)]
struct DeploymentList {
    items: Vec<NamedObject>,
}
#[derive(Deserialize)]
struct NamedObject {
    metadata: Metadata,
}
#[derive(Deserialize)]
struct ConfigMap {
    data: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct ManagerConfig {
    templates: Vec<RuntimeTemplate>,
}

pub fn verify(entries: &[PilotAdoption], root: &Path) -> Result<()> {
    verify_resources(entries, root, Stage::Transfer)
}

pub fn verify_installed(entries: &[PilotAdoption], root: &Path) -> Result<RuntimeTemplate> {
    verify_resources(entries, root, Stage::Installed)?;
    let config: ConfigMap = get("veoveo-agents", "configmap", "veoveo-agent-manager")?;
    let manager: ManagerConfig = serde_json::from_str(
        config
            .data
            .get("manager.json")
            .context("manager configuration missing")?,
    )?;
    manager
        .templates
        .into_iter()
        .find(|template| template.id.as_str() == "uav-pilot")
        .context("approved pilot template missing")
}

fn verify_resources(entries: &[PilotAdoption], root: &Path, stage: Stage) -> Result<()> {
    let release: Resource<Release> = get("flux-system", "helmrelease", "uav-sim")?;
    match stage {
        Stage::Transfer => ensure!(
            release.spec.suspend,
            "old Helm reconciliation must remain suspended"
        ),
        Stage::Installed => ensure!(
            !release.spec.suspend,
            "installed Helm reconciliation must be active"
        ),
    }
    let deployments: DeploymentList = get("veoveo", "deployments", "")?;
    let manifest: Manifest =
        serde_json::from_reader(File::open(root.join("archive-manifest.json"))?)?;
    ensure!(
        manifest.schema == "bioma-pilot-cutover/v1"
            && manifest.source_namespace == "veoveo"
            && manifest.volumes.len() == 4,
        "wrong retained-volume export"
    );
    let pods: PodList = get("veoveo", "pods", "")?;
    let source: Secret = get("veoveo", "secret", "veoveo-uav-pilot-agents")?;
    for (index, (entry, volume)) in entries.iter().zip(&manifest.volumes).enumerate() {
        let resources = &entry.instance.resources;
        ensure!(
            volume.agent == entry.instance.key,
            "archive ordering differs from adoption plan"
        );
        match stage {
            Stage::Transfer => {
                let old: Resource<Deployment> = get("veoveo", "deployment", &volume.agent)?;
                ensure!(old.spec.replicas == 0, "old pilot has desired replicas");
            }
            Stage::Installed => ensure!(
                deployments
                    .items
                    .iter()
                    .all(|item| item.metadata.name != volume.agent),
                "superseded pilot Deployment still exists"
            ),
        }
        ensure!(
            pods.items
                .iter()
                .all(|pod| pod.spec.volumes.iter().all(|mount| mount
                    .persistent_volume_claim
                    .as_ref()
                    .is_none_or(|claim| claim.claim_name != volume.agent))),
            "old pilot memory is still mounted by a Pod"
        );
        let claim: Claim = get(&resources.namespace, "pvc", &resources.volume_claim)?;
        let pv: Resource<Pv> = get("veoveo", "pv", &volume.volume)?;
        ensure!(
            claim.status.phase == "Bound"
                && claim.spec.volume_name == volume.volume
                && claim.spec.access_modes == ["ReadWriteOnce"]
                && claim.spec.storage_class_name == "local-path"
                && claim
                    .spec
                    .resources
                    .requests
                    .get("storage")
                    .is_some_and(|size| size == "2Gi")
                && claim.metadata.labels.get("veoveo.ai/managed-agent")
                    == Some(&resources.workload)
                && pv.spec.persistent_volume_reclaim_policy == "Retain"
                && pv.spec.claim_ref.name == resources.volume_claim
                && pv.spec.claim_ref.namespace.as_deref() == Some(&resources.namespace)
                && pv.spec.claim_ref.uid == claim.metadata.uid
                && pv.spec.local.path == volume.source_path,
            "retained volume ownership differs for {}",
            volume.agent
        );
        let key: Secret = get(&resources.namespace, "secret", &resources.credential_secret)?;
        let original = source
            .data
            .get(&format!("uav-{}-private-key-der-b64", index + 1))
            .context("retained source key is absent")?;
        ensure!(
            key.immutable
                && key.kind == "Opaque"
                && key.metadata.labels.get("veoveo.ai/managed-agent") == Some(&resources.workload)
                && key.data.get("private-key-der-b64") == Some(original),
            "retained signing key differs for {}",
            volume.agent
        );
    }
    Ok(())
}
