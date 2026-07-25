use std::{
    collections::BTreeMap,
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
use veoveo_deploy_contract::{
    ConfigMapSpec, LoadedProfile, ReleaseSpec, SecretFormat, SecretSpec, load_local_registry,
};
use veoveo_mcp_contract::GatewayInternalTrustBundle;

const VALIDATION_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

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
    let profile = load_profile(path)?;
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
    let profile = load_profile(path)?;
    ensure_local_registry(&profile)
}

pub(crate) fn profile_cluster_up(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
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
    let profile = load_profile(path)?;
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
    let profile = load_profile(path)?;
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

pub(crate) fn profile_up(path: &Path, revision: Option<&str>) -> Result<()> {
    let profile = load_profile(path)?;
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
    let profile = load_profile(path)?;
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

fn load_profile(path: &Path) -> Result<LoadedProfile> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let repository = repository_root(base).or_else(|_| {
        let current = env::current_dir().context("reading current working directory")?;
        repository_root(&current).context(
            "deployment profile is outside a Git worktree and the command was not run from the Veoveo repository",
        )
    })?;
    LoadedProfile::load(path, &repository)
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

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}
