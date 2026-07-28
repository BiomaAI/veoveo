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
use sha2::{Digest, Sha256};
use url::Url;
use veoveo_deploy_contract::{
    ConfigMapSpec, DeploymentLock, DeploymentSource, FirstPartyMcpServer, LoadedProfile,
    LockedSource, PlannedImage, PlatformComponent, ReleaseSpec, ReleaseValuesContract,
    SecretFormat, SecretSpec, SourceRepository, load_local_registry,
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

#[derive(Debug, Deserialize)]
struct BakePrint {
    group: BTreeMap<String, BakeGroup>,
    target: BTreeMap<String, BakeImageTarget>,
}

#[derive(Debug, Deserialize)]
struct BakeGroup {
    targets: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BakeImageTarget {
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug)]
struct ResolvedSource {
    definition: DeploymentSource,
    repository: PathBuf,
    revision: String,
    image_digests: BTreeMap<String, String>,
    _checkout: tempfile::TempDir,
}

pub(crate) fn profile_validate(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let sources = resolve_sources(&profile)?;
    let selected_images = validate_bake_groups(&profile, &sources)?;
    profile.validate_image_plan(&selected_images)?;
    validate_helm_releases(&profile, &sources)?;
    let platform = profile.resolved_platform()?;
    println!(
        "Deployment profile {} is valid: {} sources, {} image groups, {} Helm releases, {} platform components, and {} MCP servers",
        profile.definition.name,
        sources.len(),
        sources
            .iter()
            .map(|source| source.definition.image_groups.len())
            .sum::<usize>(),
        sources
            .iter()
            .map(|source| source.definition.releases.len())
            .sum::<usize>(),
        platform.components.len(),
        platform.mcp_servers.len(),
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
            let arguments = local_cluster_create_arguments(&profile)?;
            let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
            status_checked("k3d", arguments, &[], None)?;
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

pub(crate) fn profile_up(path: &Path, lock_path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let lock = load_deployment_lock(lock_path)?;
    validate_locked_profile(&profile, &lock)?;
    let sources = resolve_locked_sources(&profile, &lock)?;
    let selected_images = validate_bake_groups(&profile, &sources)?;
    profile.validate_image_plan(&selected_images)?;
    validate_locked_images(&profile, &lock, &sources, &selected_images)?;
    validate_helm_releases(&profile, &sources)?;
    let platform = profile.resolved_platform()?;
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

    for source in &sources {
        for release in &source.definition.releases {
            helm_up(
                &profile,
                source,
                context,
                release,
                &platform.components,
                &platform.mcp_servers,
            )?;
        }
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
        "Deployment profile {} now runs {} digest-locked sources",
        profile.definition.name,
        sources.len()
    );
    Ok(())
}

pub(crate) fn profile_down(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let context = profile.definition.kubernetes.context.as_str();
    let releases = profile
        .definition
        .sources
        .iter()
        .flat_map(|source| source.releases.iter())
        .collect::<Vec<_>>();
    for release in releases.into_iter().rev() {
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

fn load_deployment_lock(path: &Path) -> Result<DeploymentLock> {
    let bytes =
        fs::read(path).with_context(|| format!("reading deployment lock {}", path.display()))?;
    let lock = serde_json::from_slice::<DeploymentLock>(&bytes)
        .with_context(|| format!("decoding deployment lock {}", path.display()))?;
    lock.validate()
        .with_context(|| format!("validating deployment lock {}", path.display()))?;
    Ok(lock)
}

fn resolve_sources(profile: &LoadedProfile) -> Result<Vec<ResolvedSource>> {
    let mut resolved = Vec::with_capacity(profile.definition.sources.len());
    for source in &profile.definition.sources {
        let origin = match &source.repository {
            SourceRepository::Local { .. } => profile.local_source_root(source)?,
            SourceRepository::Git { url } => PathBuf::from(url),
        };
        let checkout = tempfile::Builder::new()
            .prefix(&format!("veoveo-deployment-{}-", source.name))
            .tempdir()
            .with_context(|| format!("creating checkout for source {}", source.name))?;
        let destination = checkout.path();
        let clone_args = [
            "clone",
            "--quiet",
            "--no-checkout",
            path_str(&origin)?,
            path_str(destination)?,
        ];
        status_checked("git", clone_args, &[], None)
            .with_context(|| format!("cloning deployment source {}", source.name))?;
        let revision = resolve_revision(destination, &source.revision)?;
        status_checked(
            "git",
            ["checkout", "--quiet", "--detach", revision.as_str()],
            &[],
            Some(destination),
        )
        .with_context(|| {
            format!(
                "checking out deployment source {} at {revision}",
                source.name
            )
        })?;
        resolved.push(ResolvedSource {
            definition: source.clone(),
            repository: destination.to_path_buf(),
            revision,
            image_digests: BTreeMap::new(),
            _checkout: checkout,
        });
    }
    Ok(resolved)
}

fn validate_locked_profile(profile: &LoadedProfile, lock: &DeploymentLock) -> Result<()> {
    ensure!(
        lock.profile == profile.definition.name,
        "deployment lock is for profile {}, expected {}",
        lock.profile,
        profile.definition.name
    );
    ensure!(
        lock.registry == profile.definition.registry.address,
        "deployment lock registry {} does not match profile registry {}",
        lock.registry,
        profile.definition.registry.address
    );
    ensure!(
        lock.platform == profile.resolved_platform()?,
        "deployment lock platform selection does not match the profile"
    );
    ensure!(
        lock.sources.len() == profile.definition.sources.len(),
        "deployment lock contains {} sources, profile declares {}",
        lock.sources.len(),
        profile.definition.sources.len()
    );
    for source in &profile.definition.sources {
        let locked = lock
            .sources
            .iter()
            .find(|candidate| candidate.name == source.name)
            .with_context(|| format!("deployment lock omits profile source {}", source.name))?;
        ensure!(
            locked.role == source.role,
            "deployment lock role for source {} does not match the profile",
            source.name
        );
    }
    Ok(())
}

fn resolve_locked_sources(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
) -> Result<Vec<ResolvedSource>> {
    let mut resolved = Vec::with_capacity(profile.definition.sources.len());
    for source in &profile.definition.sources {
        let locked = lock
            .sources
            .iter()
            .find(|candidate| candidate.name == source.name)
            .with_context(|| format!("deployment lock omits source {}", source.name))?;
        let (clone_origin, source_origin) = match &source.repository {
            SourceRepository::Local { .. } => {
                let root = profile.local_source_root(source)?;
                let origin =
                    output_checked("git", ["config", "--get", "remote.origin.url"], Some(&root))
                        .with_context(|| {
                            format!("reading Git origin for deployment source {}", source.name)
                        })?;
                (
                    path_str(&root)?.to_owned(),
                    normalize_origin(String::from_utf8(origin)?.trim())?,
                )
            }
            SourceRepository::Git { url } => (url.clone(), normalize_origin(url)?),
        };
        ensure!(
            source_origin == locked.repository,
            "deployment lock repository for source {} is {}, profile resolves {}",
            source.name,
            locked.repository,
            source_origin
        );

        let checkout = tempfile::Builder::new()
            .prefix(&format!("veoveo-deployment-{}-", source.name))
            .tempdir()
            .with_context(|| format!("creating checkout for source {}", source.name))?;
        let destination = checkout.path();
        status_checked(
            "git",
            [
                "clone",
                "--quiet",
                "--no-checkout",
                clone_origin.as_str(),
                path_str(destination)?,
            ],
            &[],
            None,
        )
        .with_context(|| format!("cloning deployment source {}", source.name))?;
        let revision = resolve_revision(destination, &locked.revision)?;
        ensure!(
            revision == locked.revision,
            "deployment source {} resolved locked revision {} to {}",
            source.name,
            locked.revision,
            revision
        );
        status_checked(
            "git",
            ["checkout", "--quiet", "--detach", revision.as_str()],
            &[],
            Some(destination),
        )
        .with_context(|| {
            format!(
                "checking out deployment source {} at locked revision {revision}",
                source.name
            )
        })?;
        validate_locked_charts(source, locked, destination, &revision)?;
        resolved.push(ResolvedSource {
            definition: source.clone(),
            repository: destination.to_path_buf(),
            revision,
            image_digests: locked_image_digests(profile, locked)?,
            _checkout: checkout,
        });
    }
    Ok(resolved)
}

fn validate_locked_charts(
    source: &DeploymentSource,
    locked: &LockedSource,
    repository: &Path,
    revision: &str,
) -> Result<()> {
    ensure!(
        locked.charts.len() == source.releases.len(),
        "deployment lock source {} contains {} charts, profile declares {} releases",
        source.name,
        locked.charts.len(),
        source.releases.len()
    );
    for release in &source.releases {
        let chart = locked
            .charts
            .iter()
            .find(|candidate| candidate.release == release.name)
            .with_context(|| {
                format!(
                    "deployment lock source {} omits Helm release {}",
                    source.name, release.name
                )
            })?;
        let coordinate = format!(
            "source://{}/{}",
            source.name,
            release.chart.to_string_lossy()
        );
        ensure!(
            chart.coordinate == coordinate,
            "deployment lock chart coordinate for release {} is {}, expected {}",
            release.name,
            chart.coordinate,
            coordinate
        );
        let archive = Command::new("git")
            .args([
                "archive",
                "--format=tar",
                revision,
                path_str(&release.chart)?,
            ])
            .current_dir(repository)
            .output()
            .with_context(|| {
                format!(
                    "archiving locked chart {} from source {}",
                    release.chart.display(),
                    source.name
                )
            })?;
        ensure!(
            archive.status.success(),
            "git archive failed for locked chart {}:\n{}",
            release.chart.display(),
            String::from_utf8_lossy(&archive.stderr)
        );
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(&archive.stdout)));
        ensure!(
            chart.digest == digest,
            "deployment lock chart digest for release {} is {}, source produced {}",
            release.name,
            chart.digest,
            digest
        );
    }
    Ok(())
}

fn locked_image_digests(
    profile: &LoadedProfile,
    locked: &LockedSource,
) -> Result<BTreeMap<String, String>> {
    let prefix = format!("{}/", profile.definition.registry.address);
    locked
        .images
        .iter()
        .map(|image| {
            let repository = image.repository.strip_prefix(&prefix).with_context(|| {
                format!(
                    "locked image {} repository {} is outside profile registry {}",
                    image.name, image.repository, profile.definition.registry.address
                )
            })?;
            Ok((repository.to_owned(), image.digest.clone()))
        })
        .collect()
}

fn validate_locked_images(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
    sources: &[ResolvedSource],
    planned: &[PlannedImage],
) -> Result<()> {
    let locked_count = lock
        .sources
        .iter()
        .map(|source| source.images.len())
        .sum::<usize>();
    ensure!(
        locked_count == planned.len(),
        "deployment lock contains {locked_count} images, selected Bake groups resolve {}",
        planned.len()
    );
    let mut locked_images = BTreeMap::new();
    for source in &lock.sources {
        for image in &source.images {
            locked_images.insert(
                (source.name.as_str(), image.name.as_str()),
                image.repository.as_str(),
            );
        }
    }
    for image in planned {
        let source = sources
            .iter()
            .find(|candidate| candidate.definition.name == image.source)
            .with_context(|| format!("planned image references unknown source {}", image.source))?;
        let repository = image
            .reference
            .strip_suffix(&format!(":{}", source.revision))
            .with_context(|| {
                format!(
                    "planned image {} does not use locked source revision {}",
                    image.reference, source.revision
                )
            })?;
        let locked_repository = locked_images
            .get(&(image.source.as_str(), image.target.as_str()))
            .with_context(|| {
                format!(
                    "deployment lock omits selected image {}:{}",
                    image.source, image.target
                )
            })?;
        ensure!(
            repository == *locked_repository,
            "deployment lock repository for {}:{} is {}, Bake resolves {}",
            image.source,
            image.target,
            locked_repository,
            repository
        );
        ensure!(
            repository.starts_with(&format!("{}/", profile.definition.registry.address)),
            "locked image repository {repository} is outside profile registry {}",
            profile.definition.registry.address
        );
    }
    Ok(())
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

fn local_cluster_create_arguments(profile: &LoadedProfile) -> Result<Vec<String>> {
    let cluster = profile
        .definition
        .kubernetes
        .local_cluster
        .as_ref()
        .context("deployment profile does not manage a local k3d cluster")?;
    let config = profile.resolve(&cluster.config);
    let mut arguments = vec![
        "cluster".to_owned(),
        "create".to_owned(),
        "--config".to_owned(),
        path_str(&config)?.to_owned(),
    ];
    let mut destinations = std::collections::BTreeSet::new();
    for manifest in &cluster.node_bootstrap_manifests {
        let source = profile.resolve(manifest);
        let filename = source
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .with_context(|| {
                format!(
                    "node bootstrap manifest has no UTF-8 file name: {}",
                    source.display()
                )
            })?;
        ensure!(
            destinations.insert(filename.to_owned()),
            "node bootstrap manifests must have unique file names; duplicate `{filename}`"
        );
        arguments.push("--volume".to_owned());
        arguments.push(format!(
            "{}:/var/lib/rancher/k3s/server/manifests/{filename}@server:*",
            path_str(&source)?
        ));
    }
    Ok(arguments)
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

fn validate_bake_groups(
    profile: &LoadedProfile,
    sources: &[ResolvedSource],
) -> Result<Vec<PlannedImage>> {
    let mut selected_images = Vec::new();
    for source in sources {
        for group in &source.definition.image_groups {
            let output = Command::new("docker")
                .args(["buildx", "bake", group.as_str(), "--print"])
                .current_dir(&source.repository)
                .env("VEOVEO_REGISTRY", &profile.definition.registry.address)
                .env("VEOVEO_IMAGE_TAG", &source.revision)
                .output()
                .with_context(|| {
                    format!(
                        "running Docker Bake group {group} from source {} in profile {}",
                        source.definition.name, profile.definition.name
                    )
                })?;
            ensure!(
                output.status.success(),
                "validating Docker Bake group {group} from source {} in profile {} failed:\n{}",
                source.definition.name,
                profile.definition.name,
                String::from_utf8_lossy(&output.stderr)
            );
            let definition =
                serde_json::from_slice::<BakePrint>(&output.stdout).with_context(|| {
                    format!(
                        "decoding Docker Bake group {group} from source {}",
                        source.definition.name
                    )
                })?;
            let targets = definition
                .group
                .get(group)
                .with_context(|| format!("Docker Bake output omitted selected group {group}"))?;
            for target in &targets.targets {
                let image = definition.target.get(target).with_context(|| {
                    format!("Docker Bake group {group} references missing target {target}")
                })?;
                ensure!(
                    image.tags.len() == 1,
                    "image target {target} from source {} must resolve exactly one OCI reference",
                    source.definition.name
                );
                selected_images.push(PlannedImage {
                    source: source.definition.name.clone(),
                    target: target.clone(),
                    reference: image
                        .tags
                        .first()
                        .expect("one image tag was required")
                        .clone(),
                });
            }
        }
    }
    Ok(selected_images)
}

fn validate_helm_releases(profile: &LoadedProfile, sources: &[ResolvedSource]) -> Result<()> {
    let platform = profile.resolved_platform()?;
    for source in sources {
        for release in &source.definition.releases {
            let rendered = helm_render(
                profile,
                source,
                release,
                VALIDATION_REVISION,
                &platform.components,
                &platform.mcp_servers,
            )?;
            let images = rendered_container_images(&rendered)?;
            ensure!(
                !images.is_empty(),
                "Helm release {} rendered no container images",
                release.name
            );
            let registry_prefix = format!("{}/", profile.definition.registry.address);
            let owned = images
                .iter()
                .filter(|image| image.starts_with(&registry_prefix))
                .collect::<Vec<_>>();
            ensure!(
                !owned.is_empty(),
                "Helm release {} rendered no images from selected registry {}",
                release.name,
                profile.definition.registry.address
            );
            for image in owned {
                ensure!(
                    image.contains("@sha256:") || image.ends_with(VALIDATION_REVISION),
                    "Helm release {} rendered mutable container image {image}",
                    release.name
                );
            }
        }
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
    source: &ResolvedSource,
    context: &str,
    release: &ReleaseSpec,
    components: &BTreeSet<PlatformComponent>,
    mcp_servers: &BTreeSet<FirstPartyMcpServer>,
) -> Result<()> {
    let chart = source.repository.join(&release.chart);
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
        args.push(path_str(&source.repository.join(values))?.to_owned());
    }
    append_release_values(
        &mut args,
        profile,
        release,
        &source.revision,
        Some(&source.image_digests),
        components,
        mcp_servers,
    )?;
    args.extend([
        "--wait".to_owned(),
        "--timeout".to_owned(),
        format!("{}s", release.timeout_seconds),
    ]);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    status_checked("helm", refs, &[], None)
}

fn helm_render(
    profile: &LoadedProfile,
    source: &ResolvedSource,
    release: &ReleaseSpec,
    revision: &str,
    components: &BTreeSet<PlatformComponent>,
    mcp_servers: &BTreeSet<FirstPartyMcpServer>,
) -> Result<String> {
    let chart = source.repository.join(&release.chart);
    let mut args = vec![
        "template".to_owned(),
        release.name.clone(),
        path_str(&chart)?.to_owned(),
    ];
    for values in &release.values {
        args.push("--values".to_owned());
        args.push(path_str(&source.repository.join(values))?.to_owned());
    }
    append_release_values(
        &mut args,
        profile,
        release,
        revision,
        None,
        components,
        mcp_servers,
    )?;
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let rendered = output_checked("helm", refs, None)
        .with_context(|| format!("rendering Helm release {}", release.name))?;
    String::from_utf8(rendered).context("Helm output is not UTF-8")
}

fn append_release_values(
    args: &mut Vec<String>,
    profile: &LoadedProfile,
    release: &ReleaseSpec,
    revision: &str,
    image_digests: Option<&BTreeMap<String, String>>,
    components: &BTreeSet<PlatformComponent>,
    mcp_servers: &BTreeSet<FirstPartyMcpServer>,
) -> Result<()> {
    match release.values_contract {
        ReleaseValuesContract::Platform | ReleaseValuesContract::VeoveoSource => {
            args.extend([
                "--set-string".to_owned(),
                format!(
                    "global.veoveoRegistry={}",
                    profile.definition.registry.address
                ),
                "--set-string".to_owned(),
                format!("global.veoveoTag={revision}"),
            ]);
            if let Some(image_digests) = image_digests {
                ensure!(
                    !image_digests.is_empty(),
                    "locked release {} has no image digests",
                    release.name
                );
                args.extend([
                    "--set".to_owned(),
                    "global.production=true".to_owned(),
                    "--set-json".to_owned(),
                    format!(
                        "global.imageDigests={}",
                        serde_json::to_string(image_digests)?
                    ),
                ]);
            }
            if release.values_contract == ReleaseValuesContract::Platform {
                args.extend([
                    "--set-string".to_owned(),
                    format!("global.installationId={}", profile.definition.name),
                    "--set-string".to_owned(),
                    "installationPreset=custom".to_owned(),
                    "--set-json".to_owned(),
                    format!("components={}", serde_json::to_string(components)?),
                    "--set-json".to_owned(),
                    format!("mcpServers={}", serde_json::to_string(mcp_servers)?),
                    "--set-json".to_owned(),
                    format!(
                        "artifactService.allowedAudiences={}",
                        serde_json::to_string(&profile.resolved_platform()?.artifact_audiences)?
                    ),
                ]);
            }
        }
        ReleaseValuesContract::Extension => {
            args.extend([
                "--set-string".to_owned(),
                format!("veoveo.registry={}", profile.definition.registry.address),
                "--set-string".to_owned(),
                format!("veoveo.sourceTag={revision}"),
                "--set-string".to_owned(),
                format!("veoveo.installationId={}", profile.definition.name),
            ]);
            if let Some(image_digests) = image_digests {
                ensure!(
                    !image_digests.is_empty(),
                    "locked release {} has no image digests",
                    release.name
                );
                args.extend([
                    "--set".to_owned(),
                    "veoveo.production=true".to_owned(),
                    "--set-json".to_owned(),
                    format!(
                        "veoveo.imageDigests={}",
                        serde_json::to_string(image_digests)?
                    ),
                ]);
            }
        }
    }
    Ok(())
}

fn rendered_container_images(rendered: &str) -> Result<Vec<String>> {
    let mut images = Vec::new();
    for line in rendered.lines() {
        let trimmed = line.trim_start();
        let value = trimmed
            .strip_prefix("image:")
            .or_else(|| trimmed.strip_prefix("- image:"));
        let Some(value) = value else {
            continue;
        };
        let value = value.trim().trim_matches(['\'', '"']);
        ensure!(
            !value.is_empty(),
            "rendered Kubernetes image field is empty"
        );
        ensure!(
            !value.chars().any(char::is_whitespace),
            "rendered Kubernetes image field contains whitespace: {value}"
        );
        images.push(value.to_owned());
    }
    Ok(images)
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

fn resolve_revision(repository: &Path, candidate: &str) -> Result<String> {
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

fn normalize_origin(origin: &str) -> Result<String> {
    let origin = origin.trim().trim_end_matches('/');
    ensure!(!origin.is_empty(), "Git origin cannot be empty");
    let expanded = if !origin.contains("://") {
        if let Some((authority, path)) = origin.split_once(':') {
            if authority.contains('@') && !path.starts_with('/') {
                format!("ssh://{authority}/{}", path.trim_start_matches('/'))
            } else {
                origin.to_owned()
            }
        } else {
            origin.to_owned()
        }
    } else {
        origin.to_owned()
    };
    if let Ok(mut url) = Url::parse(&expanded) {
        url.set_query(None);
        url.set_fragment(None);
        let normalized_path = url
            .path()
            .trim_end_matches('/')
            .strip_suffix(".git")
            .unwrap_or_else(|| url.path().trim_end_matches('/'))
            .to_owned();
        url.set_path(&normalized_path);
        return Ok(url.to_string().trim_end_matches('/').to_owned());
    }
    let path =
        fs::canonicalize(&expanded).with_context(|| format!("normalizing Git origin {origin}"))?;
    Ok(format!("file://{}", path.display()))
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::normalize_origin;

    #[test]
    fn normalizes_scp_git_origins_for_lock_comparison() {
        assert_eq!(
            normalize_origin("git@github.com:BiomaAI/veoveo.git").expect("normalize origin"),
            "ssh://git@github.com/BiomaAI/veoveo"
        );
    }
}
