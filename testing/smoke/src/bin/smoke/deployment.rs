use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::Value;
use tempfile::TempDir;
use veoveo_mcp_contract::GatewayInternalTrustBundle;

const PROFILE_SCHEMA: &str = "veoveo.io/deployment/v1";
const REGISTRY_SCHEMA: &str = "veoveo.io/local-registry/v1";
const VALIDATION_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeploymentProfile {
    schema_version: String,
    name: String,
    registry: RegistryReference,
    image_groups: Vec<String>,
    kubernetes: KubernetesTarget,
    namespace: String,
    #[serde(default)]
    resources: ResourceSet,
    releases: Vec<ReleaseSpec>,
    #[serde(default)]
    wait_for_deployments: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryReference {
    address: String,
    local_config: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalRegistrySpec {
    schema_version: String,
    name: String,
    host_port: String,
    image: String,
    volume: String,
    delete_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KubernetesTarget {
    context: String,
    local_cluster: Option<LocalClusterSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalClusterSpec {
    name: String,
    config: PathBuf,
    #[serde(default)]
    node_bootstrap_manifests: Vec<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResourceSet {
    #[serde(default)]
    manifests: Vec<PathBuf>,
    #[serde(default)]
    config_maps: Vec<ConfigMapSpec>,
    #[serde(default)]
    secrets: Vec<SecretSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigMapSpec {
    name: String,
    files: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretSpec {
    name: String,
    data_from_env: Vec<SecretEnvironmentEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretEnvironmentEntry {
    key: String,
    environment: String,
    #[serde(default)]
    format: SecretFormat,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SecretFormat {
    #[default]
    Opaque,
    GatewayInternalTrustJwks,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleaseSpec {
    name: String,
    chart: PathBuf,
    #[serde(default)]
    values: Vec<PathBuf>,
    #[serde(default)]
    create_namespace: bool,
    timeout_seconds: u64,
}

#[derive(Debug)]
struct LoadedProfile {
    definition: DeploymentProfile,
    directory: PathBuf,
    repository: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct K3dClusterSummary {
    name: String,
    servers_running: u64,
    servers_count: u64,
    agents_running: u64,
    agents_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct K3dRegistryState {
    #[serde(rename = "Running")]
    running: bool,
}

#[derive(Debug, Deserialize)]
struct K3dRegistrySummary {
    name: String,
    #[serde(rename = "State")]
    state: K3dRegistryState,
}

pub(crate) fn profile_validate(path: &Path) -> Result<()> {
    let profile = LoadedProfile::load(path)?;
    validate_bake_groups(&profile)?;
    validate_helm_releases(&profile)?;
    println!(
        "Deployment profile {} is valid: {} image groups and {} Helm releases",
        profile.definition.name,
        profile.definition.image_groups.len(),
        profile.definition.releases.len()
    );
    Ok(())
}

pub(crate) fn profile_registry_up(path: &Path) -> Result<()> {
    let profile = LoadedProfile::load(path)?;
    ensure_local_registry(&profile)
}

pub(crate) fn profile_cluster_up(path: &Path) -> Result<()> {
    let profile = LoadedProfile::load(path)?;
    ensure_local_registry(&profile)?;
    let cluster = profile
        .definition
        .kubernetes
        .local_cluster
        .as_ref()
        .context("deployment profile does not manage a local k3d cluster")?;
    let clusters = k3d_clusters()?;
    match clusters
        .iter()
        .find(|candidate| candidate.name == cluster.name)
    {
        Some(existing)
            if existing.servers_running == existing.servers_count
                && existing.agents_running == existing.agents_count =>
        {
            println!("k3d cluster {} is already running", cluster.name);
        }
        Some(_) => {
            status_checked(
                "k3d",
                ["cluster", "start", cluster.name.as_str()],
                &[],
                None,
            )?;
        }
        None => {
            let config = profile.resolve(&cluster.config);
            status_checked(
                "k3d",
                ["cluster", "create", "--config", path_str(&config)?],
                &[],
                None,
            )?;
        }
    }
    apply_local_cluster_bootstrap(&profile)?;
    wait_for_cluster_gpu(
        &profile.definition.kubernetes.context,
        Duration::from_secs(120),
    )?;
    println!(
        "Deployment profile {} cluster is ready with NVIDIA GPU capacity",
        profile.definition.name
    );
    Ok(())
}

pub(crate) fn profile_cluster_stop(path: &Path) -> Result<()> {
    let profile = LoadedProfile::load(path)?;
    let cluster = profile
        .definition
        .kubernetes
        .local_cluster
        .as_ref()
        .context("deployment profile does not manage a local k3d cluster")?;
    if k3d_clusters()?.iter().any(|item| item.name == cluster.name) {
        status_checked("k3d", ["cluster", "stop", cluster.name.as_str()], &[], None)?;
    }
    Ok(())
}

pub(crate) fn profile_cluster_delete(path: &Path) -> Result<()> {
    let profile = LoadedProfile::load(path)?;
    let cluster = profile
        .definition
        .kubernetes
        .local_cluster
        .as_ref()
        .context("deployment profile does not manage a local k3d cluster")?;
    if k3d_clusters()?.iter().any(|item| item.name == cluster.name) {
        status_checked(
            "k3d",
            ["cluster", "delete", cluster.name.as_str()],
            &[],
            None,
        )?;
    }
    Ok(())
}

pub(crate) fn profile_publish(path: &Path, revision: Option<&str>) -> Result<()> {
    let profile = LoadedProfile::load(path)?;
    if profile.definition.registry.local_config.is_some() {
        ensure_local_registry(&profile)?;
    }
    let revision = resolve_revision(&profile.repository, revision)?;
    let temporary = TempDir::new().context("creating the publication worktree directory")?;
    let source = temporary.path().join("source");
    status_checked(
        "git",
        [
            "worktree",
            "add",
            "--detach",
            path_str(&source)?,
            revision.as_str(),
        ],
        &[],
        Some(&profile.repository),
    )?;
    let mut worktree = PublicationWorktree {
        repository: profile.repository.clone(),
        source,
        active: true,
    };

    let mut args = vec!["buildx".to_owned(), "bake".to_owned()];
    args.extend(profile.definition.image_groups.iter().cloned());
    args.push("--push".to_owned());
    let envs = [
        (
            "VEOVEO_REGISTRY",
            profile.definition.registry.address.as_str(),
        ),
        ("VEOVEO_IMAGE_TAG", revision.as_str()),
    ];
    let argument_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    status_checked("docker", argument_refs, &envs, Some(&worktree.source))?;
    worktree.remove()?;
    println!(
        "Published {} image groups for immutable revision {} to {}",
        profile.definition.image_groups.len(),
        revision,
        profile.definition.registry.address
    );
    Ok(())
}

pub(crate) fn profile_up(path: &Path, revision: Option<&str>) -> Result<()> {
    let profile = LoadedProfile::load(path)?;
    let revision = resolve_revision(&profile.repository, revision)?;
    let context = profile.definition.kubernetes.context.as_str();
    apply_local_cluster_bootstrap(&profile)?;
    wait_for_cluster_gpu(context, Duration::from_secs(120))?;

    kubectl_apply_value(
        context,
        &serde_json::json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {"name": profile.definition.namespace}
        }),
    )?;

    for manifest in &profile.definition.resources.manifests {
        let manifest = profile.resolve(manifest);
        status_checked(
            "kubectl",
            [
                "--context",
                context,
                "--namespace",
                profile.definition.namespace.as_str(),
                "apply",
                "-f",
                path_str(&manifest)?,
            ],
            &[],
            None,
        )?;
    }
    for config_map in &profile.definition.resources.config_maps {
        apply_config_map(&profile, context, config_map)?;
    }
    for secret in &profile.definition.resources.secrets {
        apply_secret(&profile, context, secret)?;
    }

    for release in &profile.definition.releases {
        helm_up(&profile, context, release, &revision)?;
    }
    for deployment in &profile.definition.wait_for_deployments {
        let target = format!("deployment/{deployment}");
        status_checked(
            "kubectl",
            [
                "--context",
                context,
                "--namespace",
                profile.definition.namespace.as_str(),
                "rollout",
                "status",
                target.as_str(),
                "--timeout=10m",
            ],
            &[],
            None,
        )?;
    }
    println!(
        "Deployment profile {} now runs immutable revision {}",
        profile.definition.name, revision
    );
    Ok(())
}

pub(crate) fn profile_down(path: &Path) -> Result<()> {
    let profile = LoadedProfile::load(path)?;
    let context = profile.definition.kubernetes.context.as_str();
    for release in profile.definition.releases.iter().rev() {
        let output = Command::new("helm")
            .args([
                "--kube-context",
                context,
                "status",
                release.name.as_str(),
                "--namespace",
                profile.definition.namespace.as_str(),
            ])
            .output()
            .context("checking Helm release state")?;
        if output.status.success() {
            status_checked(
                "helm",
                [
                    "--kube-context",
                    context,
                    "uninstall",
                    release.name.as_str(),
                    "--namespace",
                    profile.definition.namespace.as_str(),
                ],
                &[],
                None,
            )?;
        }
    }
    Ok(())
}

impl LoadedProfile {
    fn load(path: &Path) -> Result<Self> {
        let path = fs::canonicalize(path)
            .with_context(|| format!("resolving deployment profile {}", path.display()))?;
        let directory = path
            .parent()
            .context("deployment profile path has no parent directory")?
            .to_path_buf();
        let definition = serde_json::from_slice::<DeploymentProfile>(
            &fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
        )
        .with_context(|| format!("decoding {}", path.display()))?;
        let repository = repository_root(&directory).or_else(|_| {
            let current = env::current_dir().context("reading current working directory")?;
            repository_root(&current).context(
                "deployment profile is outside a Git worktree and the command was not run from the Veoveo repository",
            )
        })?;
        let profile = Self {
            definition,
            directory,
            repository,
        };
        profile.validate()?;
        Ok(profile)
    }

    fn resolve(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.directory.join(path)
        }
    }

    fn validate(&self) -> Result<()> {
        let profile = &self.definition;
        ensure!(
            profile.schema_version == PROFILE_SCHEMA,
            "schemaVersion must be {PROFILE_SCHEMA}"
        );
        validate_name("profile", &profile.name)?;
        validate_name("namespace", &profile.namespace)?;
        validate_registry_address(&profile.registry.address)?;
        ensure!(
            !profile.image_groups.is_empty(),
            "imageGroups cannot be empty"
        );
        ensure!(!profile.releases.is_empty(), "releases cannot be empty");
        ensure_unique("image group", profile.image_groups.iter())?;
        for group in &profile.image_groups {
            validate_name("image group", group)?;
        }
        ensure!(
            !profile.kubernetes.context.trim().is_empty(),
            "Kubernetes context cannot be empty"
        );
        if let Some(cluster) = &profile.kubernetes.local_cluster {
            validate_name("cluster", &cluster.name)?;
            require_file(&self.resolve(&cluster.config), "k3d cluster config")?;
            let cluster_config = fs::read_to_string(self.resolve(&cluster.config))?;
            ensure!(
                cluster_config.contains(&profile.registry.address),
                "k3d cluster config must use registry {}",
                profile.registry.address
            );
            for manifest in &cluster.node_bootstrap_manifests {
                require_file(
                    &self.resolve(manifest),
                    "local cluster node bootstrap manifest",
                )?;
            }
        }
        if let Some(config) = &profile.registry.local_config {
            let registry = load_local_registry(&self.resolve(config))?;
            ensure!(
                registry.address()? == profile.registry.address,
                "local registry config resolves to {}, profile uses {}",
                registry.address()?,
                profile.registry.address
            );
        }
        for manifest in &profile.resources.manifests {
            require_file(&self.resolve(manifest), "Kubernetes manifest")?;
        }
        ensure_unique(
            "ConfigMap",
            profile.resources.config_maps.iter().map(|item| &item.name),
        )?;
        for config_map in &profile.resources.config_maps {
            validate_name("ConfigMap", &config_map.name)?;
            ensure!(
                !config_map.files.is_empty(),
                "ConfigMap {} has no files",
                config_map.name
            );
            for (key, path) in &config_map.files {
                validate_data_key(key)?;
                require_file(&self.resolve(path), "ConfigMap source")?;
            }
        }
        ensure_unique(
            "Secret",
            profile.resources.secrets.iter().map(|item| &item.name),
        )?;
        for secret in &profile.resources.secrets {
            validate_name("Secret", &secret.name)?;
            ensure!(
                !secret.data_from_env.is_empty(),
                "Secret {} has no data",
                secret.name
            );
            ensure_unique(
                "Secret data key",
                secret.data_from_env.iter().map(|item| &item.key),
            )?;
            for item in &secret.data_from_env {
                validate_data_key(&item.key)?;
                ensure!(
                    !item.environment.trim().is_empty(),
                    "Secret environment name cannot be empty"
                );
            }
        }
        ensure_unique(
            "Helm release",
            profile.releases.iter().map(|item| &item.name),
        )?;
        for release in &profile.releases {
            validate_name("Helm release", &release.name)?;
            ensure!(release.timeout_seconds > 0, "Helm timeout must be positive");
            require_directory(&self.resolve(&release.chart), "Helm chart")?;
            for values in &release.values {
                require_file(&self.resolve(values), "Helm values")?;
            }
        }
        ensure_unique(
            "deployment wait target",
            profile.wait_for_deployments.iter(),
        )?;
        for deployment in &profile.wait_for_deployments {
            validate_name("deployment wait target", deployment)?;
        }
        Ok(())
    }
}

impl LocalRegistrySpec {
    fn address(&self) -> Result<String> {
        let (_, port) = self
            .host_port
            .rsplit_once(':')
            .context("local registry hostPort must be HOST:PORT")?;
        ensure!(
            port.parse::<u16>().is_ok(),
            "local registry port is invalid"
        );
        Ok(format!("k3d-{}:{port}", self.name))
    }

    fn container_name(&self) -> String {
        format!("k3d-{}", self.name)
    }
}

struct PublicationWorktree {
    repository: PathBuf,
    source: PathBuf,
    active: bool,
}

impl PublicationWorktree {
    fn remove(&mut self) -> Result<()> {
        if self.active {
            status_checked(
                "git",
                ["worktree", "remove", "--force", path_str(&self.source)?],
                &[],
                Some(&self.repository),
            )?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for PublicationWorktree {
    fn drop(&mut self) {
        if self.active {
            let _ = Command::new("git")
                .current_dir(&self.repository)
                .args(["worktree", "remove", "--force"])
                .arg(&self.source)
                .status();
        }
    }
}

fn ensure_local_registry(profile: &LoadedProfile) -> Result<()> {
    let config = profile
        .definition
        .registry
        .local_config
        .as_ref()
        .context("deployment profile does not manage a local registry")?;
    let registry = load_local_registry(&profile.resolve(config))?;
    let expected_name = registry.container_name();
    let registries = k3d_registries()?;
    if let Some(existing) = registries.iter().find(|item| item.name == expected_name) {
        ensure!(
            existing.state.running,
            "local registry {expected_name} is not running"
        );
        println!("Local registry {expected_name} is already running");
        return Ok(());
    }

    let mut args = vec![
        "registry".to_owned(),
        "create".to_owned(),
        registry.name.clone(),
        "--port".to_owned(),
        registry.host_port.clone(),
        "--image".to_owned(),
        registry.image.clone(),
        "--volume".to_owned(),
        registry.volume.clone(),
    ];
    if registry.delete_enabled {
        args.push("--delete-enabled".to_owned());
    }
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    status_checked("k3d", refs, &[], None)?;
    Ok(())
}

fn load_local_registry(path: &Path) -> Result<LocalRegistrySpec> {
    let config = serde_json::from_slice::<LocalRegistrySpec>(
        &fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("decoding {}", path.display()))?;
    ensure!(
        config.schema_version == REGISTRY_SCHEMA,
        "local registry schemaVersion must be {REGISTRY_SCHEMA}"
    );
    for segment in config.name.split('.') {
        validate_name("local registry segment", segment)?;
    }
    ensure!(
        !config.image.trim().is_empty(),
        "local registry image is empty"
    );
    ensure!(
        config.image.contains("@sha256:"),
        "local registry image must use an immutable digest"
    );
    ensure!(
        config.volume.ends_with(":/var/lib/registry"),
        "local registry volume must mount /var/lib/registry"
    );
    let _ = config.address()?;
    Ok(config)
}

fn k3d_clusters() -> Result<Vec<K3dClusterSummary>> {
    let output = output_checked("k3d", ["cluster", "list", "-o", "json"], None)?;
    serde_json::from_slice(&output).context("decoding k3d cluster inventory")
}

fn k3d_registries() -> Result<Vec<K3dRegistrySummary>> {
    let output = output_checked("k3d", ["registry", "list", "-o", "json"], None)?;
    serde_json::from_slice(&output).context("decoding k3d registry inventory")
}

fn apply_local_cluster_bootstrap(profile: &LoadedProfile) -> Result<()> {
    let Some(cluster) = &profile.definition.kubernetes.local_cluster else {
        return Ok(());
    };
    for manifest in &cluster.node_bootstrap_manifests {
        let manifest = profile.resolve(manifest);
        status_checked(
            "kubectl",
            [
                "--context",
                profile.definition.kubernetes.context.as_str(),
                "apply",
                "-f",
                path_str(&manifest)?,
            ],
            &[],
            None,
        )?;
    }
    Ok(())
}

fn wait_for_cluster_gpu(context: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if cluster_gpu_capacity(context)? > 0 {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "Kubernetes context {context} exposes no allocatable NVIDIA GPU after {} seconds",
            timeout.as_secs()
        );
        thread::sleep(Duration::from_secs(1));
    }
}

fn cluster_gpu_capacity(context: &str) -> Result<u64> {
    let output = output_checked(
        "kubectl",
        ["--context", context, "get", "nodes", "-o", "json"],
        None,
    )?;
    let inventory = serde_json::from_slice::<Value>(&output).context("decoding node inventory")?;
    let gpu_capacity = inventory["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            node.pointer("/status/allocatable/nvidia.com~1gpu")?
                .as_str()
        })
        .filter_map(|capacity| capacity.parse::<u64>().ok())
        .sum::<u64>();
    Ok(gpu_capacity)
}

fn validate_bake_groups(profile: &LoadedProfile) -> Result<()> {
    for group in &profile.definition.image_groups {
        output_checked(
            "docker",
            ["buildx", "bake", group.as_str(), "--print"],
            Some(&profile.repository),
        )
        .with_context(|| format!("validating Docker Bake group {group}"))?;
    }
    Ok(())
}

fn validate_helm_releases(profile: &LoadedProfile) -> Result<()> {
    for release in &profile.definition.releases {
        let chart = profile.resolve(&release.chart);
        let mut args = vec![
            "template".to_owned(),
            release.name.clone(),
            path_str(&chart)?.to_owned(),
        ];
        for values in &release.values {
            args.push("--values".to_owned());
            args.push(path_str(&profile.resolve(values))?.to_owned());
        }
        args.extend([
            "--set-string".to_owned(),
            format!(
                "global.veoveoRegistry={}",
                profile.definition.registry.address
            ),
            "--set-string".to_owned(),
            format!("global.veoveoTag={VALIDATION_REVISION}"),
        ]);
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let rendered = output_checked("helm", refs, None)
            .with_context(|| format!("rendering Helm release {}", release.name))?;
        let rendered = String::from_utf8(rendered)?;
        ensure!(
            rendered.contains(&format!("{}/veoveo/", profile.definition.registry.address))
                && rendered.contains(VALIDATION_REVISION),
            "Helm release {} did not render immutable Veoveo image references",
            release.name
        );
    }
    Ok(())
}

fn apply_config_map(
    profile: &LoadedProfile,
    context: &str,
    config_map: &ConfigMapSpec,
) -> Result<()> {
    let mut data = BTreeMap::new();
    for (key, path) in &config_map.files {
        data.insert(
            key.clone(),
            fs::read_to_string(profile.resolve(path))
                .with_context(|| format!("reading ConfigMap source {}", path.display()))?,
        );
    }
    kubectl_apply_value(
        context,
        &serde_json::json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {
                "name": config_map.name,
                "namespace": profile.definition.namespace
            },
            "data": data
        }),
    )
}

fn apply_secret(profile: &LoadedProfile, context: &str, secret: &SecretSpec) -> Result<()> {
    let mut data = BTreeMap::new();
    for entry in &secret.data_from_env {
        let value = required_environment(&entry.environment)?;
        if matches!(entry.format, SecretFormat::GatewayInternalTrustJwks) {
            GatewayInternalTrustBundle::from_json(&value).with_context(|| {
                format!(
                    "{} must contain canonical gateway trust JSON",
                    entry.environment
                )
            })?;
        }
        data.insert(entry.key.clone(), value);
    }
    kubectl_apply_value(
        context,
        &serde_json::json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": {
                "name": secret.name,
                "namespace": profile.definition.namespace
            },
            "type": "Opaque",
            "stringData": data
        }),
    )
}

fn helm_up(
    profile: &LoadedProfile,
    context: &str,
    release: &ReleaseSpec,
    revision: &str,
) -> Result<()> {
    let chart = profile.resolve(&release.chart);
    let mut args = vec![
        "--kube-context".to_owned(),
        context.to_owned(),
        "upgrade".to_owned(),
        "--install".to_owned(),
        release.name.clone(),
        path_str(&chart)?.to_owned(),
        "--namespace".to_owned(),
        profile.definition.namespace.clone(),
    ];
    if release.create_namespace {
        args.push("--create-namespace".to_owned());
    }
    for values in &release.values {
        args.push("--values".to_owned());
        args.push(path_str(&profile.resolve(values))?.to_owned());
    }
    args.extend([
        "--set-string".to_owned(),
        format!(
            "global.veoveoRegistry={}",
            profile.definition.registry.address
        ),
        "--set-string".to_owned(),
        format!("global.veoveoTag={revision}"),
        "--wait".to_owned(),
        "--timeout".to_owned(),
        format!("{}s", release.timeout_seconds),
    ]);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    status_checked("helm", refs, &[], None)
}

fn kubectl_apply_value(context: &str, value: &Value) -> Result<()> {
    let mut child = Command::new("kubectl")
        .args(["--context", context, "apply", "-f", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .context("spawning kubectl apply")?;
    serde_json::to_writer(
        child
            .stdin
            .as_mut()
            .context("kubectl stdin is unavailable")?,
        value,
    )?;
    child
        .stdin
        .take()
        .context("kubectl stdin is unavailable")?
        .flush()?;
    let status = child.wait().context("waiting for kubectl apply")?;
    ensure!(status.success(), "kubectl apply failed with {status}");
    Ok(())
}

fn resolve_revision(repository: &Path, revision: Option<&str>) -> Result<String> {
    let candidate = revision.unwrap_or("HEAD");
    let expression = format!("{candidate}^{{commit}}");
    let output = output_checked(
        "git",
        ["rev-parse", "--verify", expression.as_str()],
        Some(repository),
    )?;
    let revision = String::from_utf8(output)?.trim().to_owned();
    ensure!(
        revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git revision did not resolve to a full commit SHA"
    );
    Ok(revision)
}

fn repository_root(directory: &Path) -> Result<PathBuf> {
    let output = output_checked("git", ["rev-parse", "--show-toplevel"], Some(directory))?;
    Ok(PathBuf::from(String::from_utf8(output)?.trim()))
}

fn required_environment(name: &str) -> Result<String> {
    let value = match env::var(name) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => environment_from_main_worktree(name)?.with_context(|| {
            format!(
                "required environment variable {name} is absent from the process and main worktree .env"
            )
        })?,
        Err(error) => return Err(error).with_context(|| format!("reading {name}")),
    };
    ensure!(
        !value.trim().is_empty(),
        "required environment variable {name} is empty"
    );
    Ok(value)
}

fn environment_from_main_worktree(name: &str) -> Result<Option<String>> {
    let output = output_checked("git", ["worktree", "list", "--porcelain"], None)?;
    let listing = String::from_utf8(output)?;
    let Some(main_worktree) = listing
        .lines()
        .find_map(|line| line.strip_prefix("worktree "))
    else {
        return Ok(None);
    };
    let environment_file = Path::new(main_worktree).join(".env");
    if !environment_file.is_file() {
        return Ok(None);
    }
    for item in dotenvy::from_path_iter(&environment_file)
        .with_context(|| format!("reading {}", environment_file.display()))?
    {
        let (key, value) = item.context("decoding main worktree .env")?;
        if key == name {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn output_checked<'a>(
    program: &str,
    args: impl IntoIterator<Item = &'a str>,
    directory: Option<&Path>,
) -> Result<Vec<u8>> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let output = command
        .output()
        .with_context(|| format!("running {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}

fn status_checked<'a>(
    program: &str,
    args: impl IntoIterator<Item = &'a str>,
    environment: &[(&str, &str)],
    directory: Option<&Path>,
) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args).envs(environment.iter().copied());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let status = command
        .status()
        .with_context(|| format!("running {program}"))?;
    ensure!(status.success(), "{program} failed with {status}");
    Ok(())
}

fn validate_registry_address(address: &str) -> Result<()> {
    ensure!(
        !address.trim().is_empty(),
        "registry address cannot be empty"
    );
    ensure!(
        !address.contains("://"),
        "registry address must not include a URL scheme"
    );
    ensure!(
        !address.ends_with('/'),
        "registry address must not end in /"
    );
    ensure!(
        !address.chars().any(char::is_whitespace),
        "registry address contains whitespace"
    );
    Ok(())
}

fn validate_name(kind: &str, name: &str) -> Result<()> {
    ensure!(!name.is_empty(), "{kind} name cannot be empty");
    ensure!(name.len() <= 63, "{kind} name exceeds 63 characters");
    ensure!(
        name.bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name.as_bytes()[name.len() - 1].is_ascii_alphanumeric(),
        "{kind} name {name} must be a lowercase DNS label"
    );
    Ok(())
}

fn validate_data_key(key: &str) -> Result<()> {
    ensure!(!key.is_empty(), "Kubernetes data key cannot be empty");
    ensure!(
        key.bytes()
            .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') }),
        "invalid Kubernetes data key {key}"
    );
    Ok(())
}

fn ensure_unique<'a>(kind: &str, values: impl IntoIterator<Item = &'a String>) -> Result<()> {
    let mut unique = BTreeSet::new();
    for value in values {
        ensure!(unique.insert(value), "duplicate {kind}: {value}");
    }
    Ok(())
}

fn require_file(path: &Path, kind: &str) -> Result<()> {
    ensure!(path.is_file(), "{kind} does not exist: {}", path.display());
    Ok(())
}

fn require_directory(path: &Path, kind: &str) -> Result<()> {
    ensure!(path.is_dir(), "{kind} does not exist: {}", path.display());
    Ok(())
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}
