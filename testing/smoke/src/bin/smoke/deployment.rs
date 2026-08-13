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
    ConfigMapSpec, DeploymentLock, DeploymentSource, DeploymentSourceRole, FirstPartyMcpServer,
    LoadedProfile, LockedSource, PlannedImage, PlatformComponent, ReleaseSpec,
    ReleaseValuesContract, SecretFormat, SecretSpec, SourceRepository, load_local_registry,
};
use veoveo_mcp_contract::{GatewayControlPlane, GatewayInternalTrustBundle};

#[path = "deployment/gpu.rs"]
mod gpu;

use gpu::{apply_gpu_placement, ensure_gpu_allocator, prepare_gpu_placement, verify_gpu_placement};

const VALIDATION_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const GATEWAY_MOUNT_ROOT: &str = "/etc/veoveo/gateway/";
// Source resolution needs only the selected build paths. Keep unrelated LFS
// objects as pointers; a selected LFS input still fails in its owning build.
const GIT_SKIP_LFS_SMUDGE: &[(&str, &str)] = &[("GIT_LFS_SKIP_SMUDGE", "1")];

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
    deployment_image_digests: BTreeMap<String, String>,
    _checkout: tempfile::TempDir,
}

#[derive(Debug)]
struct PreparedGatewayActivation {
    config_map_name: String,
    revision: String,
    confidential_secret: String,
    required_secret_keys: BTreeSet<String>,
    data: BTreeMap<String, String>,
}

pub(crate) fn profile_validate(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let _gateway_activation = prepare_gateway_activation(&profile)?;
    let _gpu_placement = prepare_gpu_placement(&profile)?;
    let sources = resolve_sources(&profile)?;
    let selected_images = validate_bake_selections(&profile, &sources)?;
    profile.validate_image_plan(&selected_images)?;
    validate_helm_releases(&profile, &sources)?;
    let platform = profile.resolved_platform()?;
    println!(
        "Deployment profile {} is valid: {} sources, {} image publication phases, {} Helm releases, {} platform components, and {} MCP servers",
        profile.definition.name,
        sources.len(),
        sources
            .iter()
            .map(|source| match source.definition.role {
                DeploymentSourceRole::Platform => 1,
                DeploymentSourceRole::Extension | DeploymentSourceRole::Workload => {
                    source.definition.image_groups.len()
                }
            })
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
    if profile.resolved_platform()?.gpu_scheduling.is_some() {
        wait_for_cluster_nodes(
            &profile.definition.kubernetes.context,
            Duration::from_secs(120),
        )?;
    } else {
        wait_for_cluster_gpu(
            &profile.definition.kubernetes.context,
            Duration::from_secs(120),
        )?;
    }
    println!(
        "Deployment profile {} cluster is ready",
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
    let gateway_activation = prepare_gateway_activation(&profile)?;
    let lock = load_deployment_lock(lock_path)?;
    validate_locked_profile(&profile, &lock)?;
    let sources = resolve_locked_sources(&profile, &lock)?;
    let selected_images = validate_bake_selections(&profile, &sources)?;
    profile.validate_image_plan(&selected_images)?;
    validate_locked_images(&profile, &lock, &sources, &selected_images)?;
    validate_helm_releases(&profile, &sources)?;
    let platform = profile.resolved_platform()?;
    let context = profile.definition.kubernetes.context.as_str();
    apply_local_cluster_bootstrap(&profile)?;
    if platform.gpu_scheduling.is_some() {
        wait_for_cluster_nodes(context, Duration::from_secs(120))?;
    } else {
        wait_for_cluster_gpu(context, Duration::from_secs(120))?;
    }

    kubectl_apply_value(
        context,
        &serde_json::json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {"name": profile.definition.namespace}
        }),
    )?;

    if let Some(placement) = prepare_gpu_placement(&profile)? {
        let scheduling = platform
            .gpu_scheduling
            .as_ref()
            .context("prepared GPU placement has no resolved scheduling profile")?;
        ensure_gpu_allocator(context, &profile.definition.namespace, scheduling)?;
        apply_gpu_placement(
            context,
            &profile.definition.namespace,
            scheduling,
            &placement,
        )?;
    }

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
    if let Some(activation) = &gateway_activation {
        validate_gateway_secret(
            context,
            &profile.definition.namespace,
            &activation.confidential_secret,
            &activation.required_secret_keys,
        )?;
        apply_gateway_activation(context, &profile.definition.namespace, activation)?;
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
    if let Some(scheduling) = &platform.gpu_scheduling {
        verify_gpu_placement(context, &profile.definition.namespace, scheduling)?;
    }
    println!(
        "Deployment profile {} now runs {} digest-locked sources",
        profile.definition.name,
        sources.len()
    );
    Ok(())
}

pub(crate) fn profile_gpu_verify(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let platform = profile.resolved_platform()?;
    let scheduling = platform
        .gpu_scheduling
        .as_ref()
        .context("deployment profile does not declare managed GPU scheduling")?;
    verify_gpu_placement(
        profile.definition.kubernetes.context.as_str(),
        profile.definition.namespace.as_str(),
        scheduling,
    )?;
    println!(
        "Deployment profile {} GPU placement is healthy",
        profile.definition.name
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
        status_checked("git", clone_args, GIT_SKIP_LFS_SMUDGE, None)
            .with_context(|| format!("cloning deployment source {}", source.name))?;
        let revision = resolve_revision(destination, &source.revision)?;
        status_checked(
            "git",
            ["checkout", "--quiet", "--detach", revision.as_str()],
            GIT_SKIP_LFS_SMUDGE,
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
            deployment_image_digests: BTreeMap::new(),
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
        lock.registry == profile.definition.registry.locked(),
        "deployment lock registry endpoints do not match the profile"
    );
    let profile_revision = resolve_revision(&profile.repository, "HEAD")?;
    ensure!(
        lock.profile_revision == profile_revision,
        "deployment lock installation revision {} does not match checked-out installation revision {}",
        lock.profile_revision,
        profile_revision
    );
    validate_installation_inputs(profile)?;
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

fn validate_installation_inputs(profile: &LoadedProfile) -> Result<()> {
    for path in profile.installation_inputs()? {
        let relative = path.strip_prefix(&profile.repository).with_context(|| {
            format!(
                "installation input {} is outside installation repository {}",
                path.display(),
                profile.repository.display()
            )
        })?;
        let tracked = Command::new("git")
            .args(["ls-files", "--error-unmatch", "--"])
            .arg(relative)
            .current_dir(&profile.repository)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .with_context(|| format!("checking tracked installation input {}", path.display()))?;
        ensure!(
            tracked.success(),
            "installation input {} is not tracked at profile revision",
            path.display()
        );
        let unchanged = Command::new("git")
            .args(["diff", "--quiet", "HEAD", "--"])
            .arg(relative)
            .current_dir(&profile.repository)
            .status()
            .with_context(|| format!("checking installation input {}", path.display()))?;
        ensure!(
            unchanged.success(),
            "installation input {} differs from locked profile revision",
            path.display()
        );
    }
    Ok(())
}

fn resolve_locked_sources(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
) -> Result<Vec<ResolvedSource>> {
    let mut resolved = Vec::with_capacity(profile.definition.sources.len());
    let deployment_image_digests = locked_image_digests(profile, &lock.sources)?;
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
            GIT_SKIP_LFS_SMUDGE,
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
            GIT_SKIP_LFS_SMUDGE,
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
            image_digests: locked_image_digests(profile, std::slice::from_ref(locked))?,
            deployment_image_digests: deployment_image_digests.clone(),
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
    sources: &[LockedSource],
) -> Result<BTreeMap<String, String>> {
    locked_image_digests_for_registry(&profile.definition.registry.pull_address, sources)
}

fn locked_image_digests_for_registry(
    registry: &str,
    sources: &[LockedSource],
) -> Result<BTreeMap<String, String>> {
    let prefix = format!("{registry}/");
    let mut image_digests = BTreeMap::new();
    for source in sources {
        for image in &source.images {
            let repository = image.repository.strip_prefix(&prefix).with_context(|| {
                format!(
                    "locked image {} repository {} is outside profile registry {}",
                    image.name, image.repository, registry
                )
            })?;
            ensure!(
                image_digests
                    .insert(repository.to_owned(), image.digest.clone())
                    .is_none(),
                "locked image repository {} is owned by more than one deployment source",
                image.repository
            );
        }
    }
    Ok(image_digests)
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
        "deployment lock contains {locked_count} images, selected Bake targets resolve {}",
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
            repository.starts_with(&format!("{}/", profile.definition.registry.pull_address)),
            "locked image repository {repository} is outside profile registry {}",
            profile.definition.registry.pull_address
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

fn wait_for_cluster_nodes(context: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let output = output_checked(
            "kubectl",
            ["--context", context, "get", "nodes", "-o", "json"],
            None,
        )?;
        let inventory =
            serde_json::from_slice::<Value>(&output).context("decoding node inventory")?;
        let ready = inventory["items"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|node| {
                node.pointer("/status/conditions")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|condition| {
                        condition.get("type").and_then(Value::as_str) == Some("Ready")
                            && condition.get("status").and_then(Value::as_str) == Some("True")
                    })
            });
        if ready {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "Kubernetes context {context} has no Ready node after {} seconds",
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

fn validate_bake_selections(
    profile: &LoadedProfile,
    sources: &[ResolvedSource],
) -> Result<Vec<PlannedImage>> {
    let mut selected_images = Vec::new();
    let platform_targets = profile.required_platform_images()?;
    for source in sources {
        let selections = match source.definition.role {
            DeploymentSourceRole::Platform => {
                vec![(
                    "exact platform selection".to_owned(),
                    platform_targets.iter().cloned().collect::<Vec<_>>(),
                )]
            }
            DeploymentSourceRole::Extension | DeploymentSourceRole::Workload => source
                .definition
                .image_groups
                .iter()
                .map(|group| (format!("group {group}"), vec![group.clone()]))
                .collect(),
        };
        for (selection_name, bake_patterns) in selections {
            let mut command = Command::new("docker");
            command.args(["buildx", "bake"]);
            command.args(&bake_patterns);
            let output = command
                .arg("--print")
                .current_dir(&source.repository)
                .env("VEOVEO_REGISTRY", &profile.definition.registry.pull_address)
                .env("VEOVEO_IMAGE_TAG", &source.revision)
                .output()
                .with_context(|| {
                    format!(
                        "running Docker Bake {selection_name} from source {} in profile {}",
                        source.definition.name, profile.definition.name
                    )
                })?;
            ensure!(
                output.status.success(),
                "validating Docker Bake {selection_name} from source {} in profile {} failed:\n{}",
                source.definition.name,
                profile.definition.name,
                String::from_utf8_lossy(&output.stderr)
            );
            let definition =
                serde_json::from_slice::<BakePrint>(&output.stdout).with_context(|| {
                    format!(
                        "decoding Docker Bake {selection_name} from source {}",
                        source.definition.name
                    )
                })?;
            let selected_targets = if source.definition.role == DeploymentSourceRole::Platform {
                platform_targets.iter().cloned().collect::<Vec<_>>()
            } else {
                let group = bake_patterns
                    .first()
                    .expect("extension and workload selections contain one group");
                definition
                    .group
                    .get(group)
                    .with_context(|| format!("Docker Bake output omitted selected group {group}"))?
                    .targets
                    .clone()
            };
            for target in selected_targets {
                let image = definition.target.get(&target).with_context(|| {
                    format!("Docker Bake {selection_name} references missing target {target}")
                })?;
                ensure!(
                    image.tags.len() == 1,
                    "image target {target} from source {} must resolve exactly one OCI reference",
                    source.definition.name
                );
                selected_images.push(PlannedImage {
                    source: source.definition.name.clone(),
                    target,
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
            let registry_prefix = format!("{}/", profile.definition.registry.pull_address);
            let owned = images
                .iter()
                .filter(|image| image.starts_with(&registry_prefix))
                .collect::<Vec<_>>();
            ensure!(
                !owned.is_empty(),
                "Helm release {} rendered no images from selected registry {}",
                release.name,
                profile.definition.registry.pull_address
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

fn prepare_gateway_activation(
    profile: &LoadedProfile,
) -> Result<Option<PreparedGatewayActivation>> {
    let Some(activation) = &profile.definition.gateway_activation else {
        return Ok(None);
    };
    let control_plane_path = profile.resolve(&activation.control_plane);
    let control_plane_text = fs::read_to_string(&control_plane_path).with_context(|| {
        format!(
            "reading gateway activation control plane {}",
            control_plane_path.display()
        )
    })?;
    let control_plane: GatewayControlPlane = serde_json::from_str(&control_plane_text)
        .with_context(|| {
            format!(
                "decoding gateway activation control plane {}",
                control_plane_path.display()
            )
        })?;
    control_plane
        .validate()
        .context("validating gateway activation control plane")?;

    let jwks_keys = control_plane
        .jwks_file_paths()
        .into_iter()
        .map(gateway_mount_key)
        .collect::<Result<BTreeSet<_>>>()?;
    let ca_keys = control_plane
        .certificate_authority_file_paths()
        .into_iter()
        .map(gateway_mount_key)
        .collect::<Result<BTreeSet<_>>>()?;
    let referenced = jwks_keys.union(&ca_keys).cloned().collect::<BTreeSet<_>>();
    let configured = activation
        .public_files
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        referenced == configured,
        "gateway activation publicFiles differ from control-plane file references; referenced={referenced:?}, configured={configured:?}"
    );

    let mut data = BTreeMap::from([(activation.control_plane_key.clone(), control_plane_text)]);
    for (key, path) in &activation.public_files {
        let resolved = profile.resolve(path);
        let text = fs::read_to_string(&resolved).with_context(|| {
            format!(
                "reading gateway activation public file {}",
                resolved.display()
            )
        })?;
        validate_gateway_public_file(key, &text, jwks_keys.contains(key), ca_keys.contains(key))?;
        data.insert(key.clone(), text);
    }

    let mut hasher = Sha256::new();
    hasher.update(b"veoveo.io/gateway-activation/v1\0");
    for (key, value) in &data {
        hasher.update(
            u64::try_from(key.len())
                .expect("ConfigMap key length fits u64")
                .to_be_bytes(),
        );
        hasher.update(key.as_bytes());
        hasher.update(
            u64::try_from(value.len())
                .expect("ConfigMap value length fits u64")
                .to_be_bytes(),
        );
        hasher.update(value.as_bytes());
    }
    let digest = hex::encode(hasher.finalize());
    Ok(Some(PreparedGatewayActivation {
        config_map_name: format!("{}-{}", activation.config_map_name_prefix, &digest[..12]),
        revision: digest,
        confidential_secret: activation.confidential_secret.clone(),
        required_secret_keys: activation.required_secret_keys.clone(),
        data,
    }))
}

fn gateway_mount_key(path: &str) -> Result<String> {
    let key = path.strip_prefix(GATEWAY_MOUNT_ROOT).with_context(|| {
        format!("gateway public file path {path:?} must be beneath {GATEWAY_MOUNT_ROOT}")
    })?;
    ensure!(
        !key.is_empty() && !key.contains('/'),
        "gateway public file path {path:?} must resolve to one ConfigMap data key"
    );
    Ok(key.to_owned())
}

fn validate_gateway_public_file(key: &str, text: &str, jwks: bool, ca: bool) -> Result<()> {
    if jwks {
        let jwks: jsonwebtoken::jwk::JwkSet = serde_json::from_str(text)
            .with_context(|| format!("decoding gateway JWKS file {key}"))?;
        ensure!(!jwks.keys.is_empty(), "gateway JWKS file {key} has no keys");
    }
    if ca {
        let certificates = reqwest::Certificate::from_pem_bundle(text.as_bytes())
            .with_context(|| format!("parsing gateway CA bundle {key}"))?;
        ensure!(
            !certificates.is_empty(),
            "gateway CA bundle {key} has no certificates"
        );
    }
    Ok(())
}

fn validate_gateway_secret(
    context: &str,
    namespace: &str,
    name: &str,
    required_keys: &BTreeSet<String>,
) -> Result<()> {
    let bytes = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--namespace",
            namespace,
            "get",
            "secret",
            name,
            "--output=json",
        ],
        None,
    )
    .with_context(|| format!("reading installation-owned gateway Secret {name}"))?;
    let secret: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("decoding installation-owned gateway Secret {name}"))?;
    let data = secret
        .get("data")
        .and_then(Value::as_object)
        .with_context(|| format!("installation-owned gateway Secret {name} has no data"))?;
    for key in required_keys {
        ensure!(
            data.get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "installation-owned gateway Secret {name} is missing required key {key}"
        );
    }
    Ok(())
}

fn apply_gateway_activation(
    context: &str,
    namespace: &str,
    activation: &PreparedGatewayActivation,
) -> Result<()> {
    kubectl_apply_value(
        context,
        &serde_json::json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {
                "name": activation.config_map_name,
                "namespace": namespace,
                "labels": {
                    "app.kubernetes.io/managed-by": "veoveo-profile",
                    "veoveo.ai/gateway-activation": "true"
                },
                "annotations": {
                    "veoveo.ai/gateway-activation-revision": activation.revision
                }
            },
            "immutable": true,
            "data": activation.data
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
    for values in ordered_release_values(&source.repository, &profile.directory, release) {
        args.push("--values".to_owned());
        args.push(path_str(&values)?.to_owned());
    }
    let image_digests = release_image_digests(
        release.values_contract,
        &source.image_digests,
        &source.deployment_image_digests,
    );
    append_release_values(
        &mut args,
        profile,
        release,
        &source.revision,
        Some(image_digests),
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

fn release_image_digests<'a>(
    values_contract: ReleaseValuesContract,
    source: &'a BTreeMap<String, String>,
    deployment: &'a BTreeMap<String, String>,
) -> &'a BTreeMap<String, String> {
    match values_contract {
        ReleaseValuesContract::Extension => deployment,
        ReleaseValuesContract::Platform | ReleaseValuesContract::VeoveoSource => source,
    }
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
    for values in ordered_release_values(&source.repository, &profile.directory, release) {
        args.push("--values".to_owned());
        args.push(path_str(&values)?.to_owned());
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

fn ordered_release_values(
    source_repository: &Path,
    installation_directory: &Path,
    release: &ReleaseSpec,
) -> Vec<PathBuf> {
    release
        .source_values
        .iter()
        .map(|path| source_repository.join(path))
        .chain(
            release
                .installation_values
                .iter()
                .map(|path| installation_directory.join(path)),
        )
        .collect()
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
                    profile.definition.registry.pull_address
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
                let platform = profile.resolved_platform()?;
                args.extend([
                    "--set-string".to_owned(),
                    format!("global.installationId={}", profile.definition.name),
                    "--set-string".to_owned(),
                    "installationPreset=custom".to_owned(),
                    "--set-json".to_owned(),
                    format!("components={}", serde_json::to_string(components)?),
                    "--set-json".to_owned(),
                    format!("mcpServers={}", serde_json::to_string(mcp_servers)?),
                ]);
                if !platform.artifact_audiences.is_empty() {
                    args.extend([
                        "--set-json".to_owned(),
                        format!(
                            "artifactService.allowedAudiences={}",
                            serde_json::to_string(&platform.artifact_audiences)?
                        ),
                    ]);
                }
                if let Some(placement) = prepare_gpu_placement(profile)? {
                    args.extend([
                        "--set-json".to_owned(),
                        format!(
                            "global.gpuPlacement={}",
                            serde_json::to_string(&serde_json::json!({
                                "enabled": true,
                                "claimName": placement.claim_name,
                                "runtimeClassName": placement.runtime_class_name,
                                "evidenceDigest": placement.evidence_digest,
                                "workloadRequests": placement.workload_requests,
                                "workloadReplicas": placement.workload_replicas
                            }))?
                        ),
                    ]);
                }
                if let Some(activation) = prepare_gateway_activation(profile)? {
                    args.extend([
                        "--set-string".to_owned(),
                        format!(
                            "gateway.existingControlPlaneConfigMap={}",
                            activation.config_map_name
                        ),
                        "--set-string".to_owned(),
                        format!("gateway.controlPlaneRevision={}", activation.revision),
                        "--set-string".to_owned(),
                        format!("global.existingSecret={}", activation.confidential_secret),
                    ]);
                }
            }
        }
        ReleaseValuesContract::Extension => {
            args.extend([
                "--set-string".to_owned(),
                format!(
                    "veoveo.registry={}",
                    profile.definition.registry.pull_address
                ),
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
            if let Some(placement) = prepare_gpu_placement(profile)? {
                args.extend([
                    "--set-json".to_owned(),
                    format!(
                        "veoveo.gpuPlacement={}",
                        serde_json::to_string(&serde_json::json!({
                            "enabled": true,
                            "claimName": placement.claim_name,
                            "runtimeClassName": placement.runtime_class_name,
                            "evidenceDigest": placement.evidence_digest,
                            "workloadRequests": placement.workload_requests,
                            "workloadReplicas": placement.workload_replicas
                        }))?
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
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };

    use veoveo_deploy_contract::{
        DeploymentSourceRole, LoadedProfile, LockedImage, LockedSource, ReleaseSpec,
        ReleaseValuesContract,
    };

    use super::{
        gateway_mount_key, locked_image_digests_for_registry, normalize_origin,
        ordered_release_values, prepare_gateway_activation, release_image_digests,
        validate_gateway_public_file,
    };

    const DIGEST_A: &str =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DIGEST_B: &str =
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn deployment_image_closure_spans_sources() {
        let sources = [
            LockedSource {
                name: "platform".to_owned(),
                role: DeploymentSourceRole::Platform,
                repository: "https://example.invalid/platform".to_owned(),
                revision: "a".repeat(40),
                images: vec![LockedImage {
                    name: "agent-kernel".to_owned(),
                    repository: "registry.example/veoveo/agent-kernel".to_owned(),
                    digest: DIGEST_A.to_owned(),
                    publication_digest: DIGEST_B.to_owned(),
                }],
                charts: vec![],
            },
            LockedSource {
                name: "extension".to_owned(),
                role: DeploymentSourceRole::Extension,
                repository: "https://example.invalid/extension".to_owned(),
                revision: "b".repeat(40),
                images: vec![LockedImage {
                    name: "runtime".to_owned(),
                    repository: "registry.example/extension/runtime".to_owned(),
                    digest: DIGEST_B.to_owned(),
                    publication_digest: DIGEST_A.to_owned(),
                }],
                charts: vec![],
            },
        ];

        assert_eq!(
            locked_image_digests_for_registry("registry.example", &sources).unwrap(),
            [
                ("extension/runtime".to_owned(), DIGEST_B.to_owned()),
                ("veoveo/agent-kernel".to_owned(), DIGEST_A.to_owned()),
            ]
            .into_iter()
            .collect()
        );
    }

    #[test]
    fn external_values_receive_platform_images_without_polluting_platform_values() {
        let source = [("veoveo/gateway".to_owned(), DIGEST_A.to_owned())]
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let deployment = [
            ("extension/runtime".to_owned(), DIGEST_B.to_owned()),
            ("veoveo/gateway".to_owned(), DIGEST_A.to_owned()),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>();

        assert_eq!(
            release_image_digests(ReleaseValuesContract::Platform, &source, &deployment),
            &source
        );
        assert_eq!(
            release_image_digests(ReleaseValuesContract::VeoveoSource, &source, &deployment),
            &source
        );
        assert_eq!(
            release_image_digests(ReleaseValuesContract::Extension, &source, &deployment),
            &deployment
        );
    }

    #[test]
    fn normalizes_scp_git_origins_for_lock_comparison() {
        assert_eq!(
            normalize_origin("git@github.com:BiomaAI/veoveo.git").expect("normalize origin"),
            "ssh://git@github.com/BiomaAI/veoveo"
        );
    }

    #[test]
    fn installation_values_override_source_values_in_helm_order() {
        let release = ReleaseSpec {
            name: "example".to_owned(),
            chart: PathBuf::from("chart"),
            source_values: vec![PathBuf::from("defaults.yaml")],
            installation_values: vec![
                PathBuf::from("environment.yaml"),
                PathBuf::from("site.yaml"),
            ],
            values_contract: ReleaseValuesContract::Platform,
            create_namespace: false,
            timeout_seconds: 60,
        };

        assert_eq!(
            ordered_release_values(Path::new("/source"), Path::new("/installation"), &release),
            [
                PathBuf::from("/source/defaults.yaml"),
                PathBuf::from("/installation/environment.yaml"),
                PathBuf::from("/installation/site.yaml"),
            ]
        );
    }

    #[test]
    fn checked_in_profile_preflights_revisioned_gateway_activation() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile_path = repository.join("showcase/sumo/deploy/deployment.json");
        let profile = LoadedProfile::load(&profile_path, &repository).unwrap();
        let activation = prepare_gateway_activation(&profile).unwrap().unwrap();

        assert!(activation.config_map_name.starts_with("veoveo-gateway-"));
        assert_eq!(activation.revision.len(), 64);
        assert!(
            activation
                .revision
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        assert_eq!(
            activation.data.keys().cloned().collect::<Vec<_>>(),
            ["gateway.json", "jwks.json"]
        );
        assert!(
            activation
                .required_secret_keys
                .contains("oidc-client-secret")
        );
    }

    #[test]
    fn gateway_public_file_validation_rejects_invalid_trust_material() {
        let invalid_jwks = validate_gateway_public_file("jwks.json", "not-json", true, false)
            .expect_err("invalid JWKS must fail");
        assert!(invalid_jwks.to_string().contains("decoding gateway JWKS"));

        let empty_jwks = validate_gateway_public_file("jwks.json", r#"{"keys":[]}"#, true, false)
            .expect_err("empty JWKS must fail");
        assert!(empty_jwks.to_string().contains("has no keys"));

        let invalid_ca = validate_gateway_public_file("ca.pem", "not-pem", false, true)
            .expect_err("invalid CA must fail");
        assert!(invalid_ca.to_string().contains("gateway CA bundle"));
    }

    #[test]
    fn gateway_public_files_must_be_direct_mount_keys() {
        assert_eq!(
            gateway_mount_key("/etc/veoveo/gateway/ca.pem").unwrap(),
            "ca.pem"
        );
        assert!(gateway_mount_key("/tmp/ca.pem").is_err());
        assert!(gateway_mount_key("/etc/veoveo/gateway/trust/ca.pem").is_err());
    }
}
