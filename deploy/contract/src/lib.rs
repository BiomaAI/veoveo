//! Typed repository deployment profiles and local registry declarations.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;

/// Canonical multi-source deployment profile.
pub const PROFILE_SCHEMA: &str = "veoveo.io/deployment/v2";
/// Canonical immutable multi-source deployment lock.
pub const DEPLOYMENT_LOCK_SCHEMA: &str = "veoveo.io/deployment-lock/v2";
/// Canonical local OCI registry declaration.
pub const REGISTRY_SCHEMA: &str = "veoveo.io/local-registry/v1";

/// A validated multi-source deployment profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentProfile {
    /// Profile schema identifier.
    pub schema_version: String,
    /// Stable profile name.
    pub name: String,
    /// Image publication destination.
    pub registry: RegistryReference,
    /// Independently resolved and published source repositories.
    pub sources: Vec<DeploymentSource>,
    /// Kubernetes destination.
    pub kubernetes: KubernetesTarget,
    /// Installation namespace.
    pub namespace: String,
    /// Resources applied before Helm.
    #[serde(default)]
    pub resources: ResourceSet,
    /// Typed first-party platform selection.
    pub platform: PlatformSelection,
    /// Gateway composition requirement documents checked against `platform`.
    #[serde(default)]
    pub gateway_requirements: Vec<PathBuf>,
    /// Additional deployments awaited after Helm.
    #[serde(default)]
    pub wait_for_deployments: Vec<String>,
}

/// An OCI registry selected by a deployment profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryReference {
    /// Registry host and optional port, without a URL scheme.
    pub address: String,
    /// Optional repository-local lifecycle declaration.
    pub local_config: Option<PathBuf>,
}

/// One independently versioned source and its owned build and chart surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentSource {
    /// Installation-local source identity.
    pub name: String,
    /// Artifact ownership boundary used for platform-image closure.
    pub role: DeploymentSourceRole,
    /// Source repository location.
    pub repository: SourceRepository,
    /// Independently resolved Git revision or ref.
    pub revision: String,
    /// Ordered Docker Bake publication phases owned by this source.
    #[serde(default)]
    pub image_groups: Vec<String>,
    /// Ordered Helm releases owned by this source.
    #[serde(default)]
    pub releases: Vec<ReleaseSpec>,
}

/// Ownership role for one independently versioned deployment source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentSourceRole {
    /// The sole Veoveo platform source in this deployment.
    Platform,
    /// An independently owned extension source.
    Extension,
}

/// Repository location for one deployment source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceRepository {
    /// Repository path relative to the deployment profile.
    Local { path: PathBuf },
    /// Git repository fetched into an isolated deployment cache.
    Git { url: String },
}

/// Repository-local OCI registry lifecycle settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalRegistrySpec {
    /// Registry schema identifier.
    pub schema_version: String,
    /// Stable registry name.
    pub name: String,
    /// Loopback host and port binding.
    pub host_port: String,
    /// Digest-pinned registry image.
    pub image: String,
    /// Persistent Docker volume.
    pub volume: String,
    /// Whether registry deletion is enabled.
    pub delete_enabled: bool,
}

/// Kubernetes destination selected by a deployment profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KubernetesTarget {
    /// Explicit kubeconfig context.
    pub context: String,
    /// Optional repository-managed k3d cluster.
    pub local_cluster: Option<LocalClusterSpec>,
}

/// Repository-managed local k3d cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalClusterSpec {
    /// Stable k3d cluster name.
    pub name: String,
    /// k3d configuration path.
    pub config: PathBuf,
    /// Manifests injected into the node at bootstrap.
    #[serde(default)]
    pub node_bootstrap_manifests: Vec<PathBuf>,
}

/// Kubernetes resources applied before Helm.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceSet {
    /// Raw manifest paths.
    #[serde(default)]
    pub manifests: Vec<PathBuf>,
    /// File-backed ConfigMaps.
    #[serde(default)]
    pub config_maps: Vec<ConfigMapSpec>,
    /// Environment-backed Secrets.
    #[serde(default)]
    pub secrets: Vec<SecretSpec>,
}

/// A file-backed ConfigMap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigMapSpec {
    /// Kubernetes object name.
    pub name: String,
    /// Data key to source path.
    pub files: BTreeMap<String, PathBuf>,
}

/// An environment-backed development Secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretSpec {
    /// Kubernetes object name.
    pub name: String,
    /// Secret data entries.
    pub data_from_env: Vec<SecretEnvironmentEntry>,
}

/// One Secret data entry loaded from an environment variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretEnvironmentEntry {
    /// Kubernetes Secret data key.
    pub key: String,
    /// Environment variable name.
    pub environment: String,
    /// Controlled value format.
    #[serde(default)]
    pub format: SecretFormat,
}

/// Controlled validation applied to a Secret value.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SecretFormat {
    /// Uninterpreted Secret text.
    #[default]
    Opaque,
    /// Canonical gateway internal trust JWKS.
    GatewayInternalTrustJwks,
}

/// A Helm release selected by a deployment profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseSpec {
    /// Helm release name.
    pub name: String,
    /// Chart root.
    pub chart: PathBuf,
    /// Ordered values files.
    #[serde(default)]
    pub values: Vec<PathBuf>,
    /// Typed values surface used for registry, revision, and platform injection.
    pub values_contract: ReleaseValuesContract,
    /// Whether Helm creates the namespace.
    #[serde(default)]
    pub create_namespace: bool,
    /// Helm operation timeout.
    pub timeout_seconds: u64,
}

/// Values surface implemented by one selected chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseValuesContract {
    /// Core Veoveo platform chart, including typed component selection.
    Platform,
    /// Veoveo-owned application chart using the global image source fields.
    VeoveoSource,
    /// External chart consuming the private extension Helm library contract.
    Extension,
}

/// A chart-level installation preset.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum InstallationPreset {
    /// Complete first-party platform surface.
    Full,
    /// Gateway, storage, Artifact, Frames, and Recording for extension installations.
    ExtensionFoundation,
    /// An explicit component and server selection.
    Custom,
}

/// Independently selectable platform infrastructure.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum PlatformComponent {
    Gateway,
    PlatformStore,
    ObjectStore,
    ArtifactService,
    /// Durable ingest, spool, and publication plane for recordings.
    RecordingDataPlane,
    /// Hardware-only renderer workload and its private pose/media services.
    GpuRenderer,
    /// Canonical simulation runtime compatibility artifacts and conformance gates.
    SimulationRuntimeSupport,
    Console,
    Telemetry,
    Ingress,
}

/// First-party hosted MCP servers selectable by an installation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum FirstPartyMcpServer {
    Artifact,
    Media,
    Timeseries,
    Optimization,
    Frames,
    Map,
    Time,
    View,
    Datasheet,
    Duckdb,
    Chart,
    Rerun,
    Recording,
    Stream,
    Reason,
    SimulationView,
}

/// Platform capability names accepted from gateway composition requirements.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PlatformCapability {
    Artifact,
    Frames,
    Map,
    Media,
    Recording,
    Rrd,
    SimulationView,
}

/// NVIDIA GPU isolation selected for one independently scheduled workload.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GpuIsolation {
    /// One ordinary Kubernetes extended resource per requested physical GPU.
    Exclusive,
    /// An installation-defined NVIDIA time-slicing profile backed by measurements.
    NvidiaTimeSlicing,
    /// An installation-defined NVIDIA MIG profile backed by measurements.
    NvidiaMig,
}

/// One typed NVIDIA GPU workload placement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuWorkloadPlacement {
    /// Stable component or external workload identifier.
    pub workload: String,
    /// NVIDIA extended resources requested by the workload.
    pub devices: u16,
    /// Isolation mechanism selected by the installation.
    pub isolation: GpuIsolation,
    /// Digest of measured sharing evidence. Required for every shared placement.
    pub evidence_digest: Option<String>,
}

/// Installation GPU capacity used to reject impossible component combinations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuSchedulingProfile {
    /// Kubernetes RuntimeClass used by every selected NVIDIA workload.
    pub runtime_class_name: String,
    /// Physical NVIDIA devices schedulable by this installation profile.
    pub allocatable_devices: u16,
    /// Exact selected platform and external GPU workloads.
    pub workloads: Vec<GpuWorkloadPlacement>,
}

/// Typed first-party platform selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformSelection {
    /// Predefined or explicit selection mode.
    pub installation_preset: InstallationPreset,
    /// Explicit components; valid only for `custom`.
    #[serde(default)]
    pub components: BTreeSet<PlatformComponent>,
    /// Explicit hosted MCP servers; valid only for `custom`.
    #[serde(default)]
    pub mcp_servers: BTreeSet<FirstPartyMcpServer>,
    /// Installation-admitted artifact audiences.
    #[serde(default)]
    pub artifact_audiences: BTreeSet<String>,
    /// Independently owned extension workloads included in the installation lock.
    #[serde(default)]
    pub external_workloads: BTreeSet<String>,
    /// Optional explicit GPU placement. Production GPU selections provide this field.
    pub gpu_scheduling: Option<GpuSchedulingProfile>,
}

/// Fully expanded platform selection used by renderers and validators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedPlatformSelection {
    pub components: BTreeSet<PlatformComponent>,
    pub mcp_servers: BTreeSet<FirstPartyMcpServer>,
    pub artifact_audiences: BTreeSet<String>,
    #[serde(default)]
    pub external_workloads: BTreeSet<String>,
    pub gpu_scheduling: Option<GpuSchedulingProfile>,
}

/// Portable subset of a gateway composition requirements document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayDeploymentRequirements {
    pub platform_capabilities: BTreeSet<PlatformCapability>,
    pub artifact_audiences: BTreeSet<String>,
}

/// One immutable deployment lock spanning every selected source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentLock {
    pub schema_version: String,
    pub profile: String,
    pub registry: String,
    pub sources: Vec<LockedSource>,
    pub platform: ResolvedPlatformSelection,
}

/// Immutable resolution and artifact inventory for one source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockedSource {
    pub name: String,
    pub role: DeploymentSourceRole,
    pub repository: String,
    pub revision: String,
    pub images: Vec<LockedImage>,
    pub charts: Vec<LockedChart>,
}

/// One source-qualified OCI image reference resolved before publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedImage {
    pub source: String,
    pub target: String,
    pub reference: String,
}

/// One source-owned OCI image in a deployment lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockedImage {
    pub name: String,
    pub repository: String,
    /// Stable runnable platform-manifest digest consumed by Helm.
    pub digest: String,
    /// Attested OCI image-index digest emitted by this publication run.
    pub publication_digest: String,
}

/// One source-owned Helm chart in a deployment lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockedChart {
    pub release: String,
    pub coordinate: String,
    pub digest: String,
}

/// A deployment profile resolved against a repository and profile directory.
#[derive(Debug)]
pub struct LoadedProfile {
    /// Parsed profile definition.
    pub definition: DeploymentProfile,
    /// Canonical profile parent directory.
    pub directory: PathBuf,
    /// Canonical repository source root.
    pub repository: PathBuf,
}

impl LoadedProfile {
    /// Loads and validates a profile inside the supplied source root.
    pub fn load(path: &Path, repository: &Path) -> Result<Self> {
        let repository = fs::canonicalize(repository)
            .with_context(|| format!("resolving repository {}", repository.display()))?;
        let path = fs::canonicalize(path)
            .with_context(|| format!("resolving deployment profile {}", path.display()))?;
        ensure!(
            path.starts_with(&repository),
            "deployment profile {} is outside repository {}",
            path.display(),
            repository.display()
        );
        let directory = path
            .parent()
            .context("deployment profile path has no parent directory")?
            .to_path_buf();
        let definition = serde_json::from_slice::<DeploymentProfile>(
            &fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
        )
        .with_context(|| format!("decoding {}", path.display()))?;
        let profile = Self {
            definition,
            directory,
            repository,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Resolves a profile-owned path.
    #[must_use]
    pub fn resolve(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.directory.join(path)
        }
    }

    /// Resolves a local source repository selected by the profile.
    pub fn local_source_root(&self, source: &DeploymentSource) -> Result<PathBuf> {
        let SourceRepository::Local { path } = &source.repository else {
            anyhow::bail!("deployment source {} is a Git source", source.name);
        };
        let candidate = if path.is_absolute() {
            path.clone()
        } else {
            self.directory.join(path)
        };
        fs::canonicalize(&candidate).with_context(|| {
            format!(
                "resolving local repository for source {} at {}",
                source.name,
                candidate.display()
            )
        })
    }

    /// Loads and combines every gateway requirement document in profile order.
    pub fn gateway_requirements(&self) -> Result<GatewayDeploymentRequirements> {
        let mut requirements = GatewayDeploymentRequirements {
            platform_capabilities: BTreeSet::new(),
            artifact_audiences: BTreeSet::new(),
        };
        for path in &self.definition.gateway_requirements {
            let resolved = self.resolve(path);
            let document = serde_json::from_slice::<GatewayDeploymentRequirements>(
                &fs::read(&resolved).with_context(|| format!("reading {}", resolved.display()))?,
            )
            .with_context(|| format!("decoding {}", resolved.display()))?;
            requirements
                .platform_capabilities
                .extend(document.platform_capabilities);
            requirements
                .artifact_audiences
                .extend(document.artifact_audiences);
        }
        Ok(requirements)
    }

    /// Expands the platform preset and validates gateway requirements.
    pub fn resolved_platform(&self) -> Result<ResolvedPlatformSelection> {
        let resolved = self.definition.platform.resolve()?;
        resolved.satisfy(&self.gateway_requirements()?)?;
        Ok(resolved)
    }

    /// Resolves the Veoveo-owned OCI images required by the selected platform
    /// and its composed gateway requirements.
    pub fn required_platform_images(&self) -> Result<BTreeSet<String>> {
        let platform = self.resolved_platform()?;
        let requirements = self.gateway_requirements()?;
        let mut images = platform.required_images();
        images.extend(requirements.required_images());
        Ok(images)
    }

    /// Validates the complete source-qualified image plan before execution.
    pub fn validate_image_plan(&self, images: &[PlannedImage]) -> Result<()> {
        let required = self.required_platform_images()?;
        self.definition.validate_image_plan(images, &required)
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
        ensure!(!profile.sources.is_empty(), "sources cannot be empty");
        ensure_unique(
            "deployment source",
            profile.sources.iter().map(|item| &item.name),
        )?;
        let platform_sources = profile
            .sources
            .iter()
            .filter(|source| source.role == DeploymentSourceRole::Platform)
            .count();
        ensure!(
            platform_sources == 1,
            "deployment profile must contain exactly one platform source, found {platform_sources}"
        );
        let mut release_names = BTreeSet::new();
        for source in &profile.sources {
            validate_name("deployment source", &source.name)?;
            ensure!(
                !source.revision.trim().is_empty(),
                "deployment source {} revision cannot be empty",
                source.name
            );
            ensure!(
                !source.image_groups.is_empty() || !source.releases.is_empty(),
                "deployment source {} owns neither images nor Helm releases",
                source.name
            );
            ensure_unique("image group", source.image_groups.iter())?;
            for group in &source.image_groups {
                validate_name("image group", group)?;
            }
            match &source.repository {
                SourceRepository::Local { .. } => {
                    let root = self.local_source_root(source)?;
                    ensure!(
                        root.join(".git").exists()
                            || root.join("docker-bake.hcl").exists()
                            || root == self.repository,
                        "local source {} is not a repository root: {}",
                        source.name,
                        root.display()
                    );
                    validate_releases(source, &root, &mut release_names)?;
                }
                SourceRepository::Git { url } => {
                    validate_git_url(url)?;
                    validate_release_metadata(source, &mut release_names)?;
                }
            }
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
        for requirements in &profile.gateway_requirements {
            require_file(
                &self.resolve(requirements),
                "gateway composition requirements",
            )?;
        }
        let _ = self.resolved_platform()?;
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

impl DeploymentProfile {
    /// Returns the single source that owns Veoveo platform artifacts.
    pub fn platform_source(&self) -> Result<&DeploymentSource> {
        let mut sources = self
            .sources
            .iter()
            .filter(|source| source.role == DeploymentSourceRole::Platform);
        let source = sources
            .next()
            .context("deployment profile contains no platform source")?;
        ensure!(
            sources.next().is_none(),
            "deployment profile contains more than one platform source"
        );
        Ok(source)
    }

    /// Validates image ownership, collision freedom, and platform closure.
    pub fn validate_image_plan(
        &self,
        images: &[PlannedImage],
        required_platform_images: &BTreeSet<String>,
    ) -> Result<()> {
        let platform_source = self.platform_source()?;
        let source_names = self
            .sources
            .iter()
            .map(|source| source.name.as_str())
            .collect::<BTreeSet<_>>();
        let mut source_targets = BTreeSet::new();
        let mut references = BTreeMap::new();
        let mut platform_targets = BTreeSet::new();

        for image in images {
            ensure!(
                source_names.contains(image.source.as_str()),
                "image target {} references unknown deployment source {}",
                image.target,
                image.source
            );
            validate_name("image target", &image.target)?;
            ensure!(
                !image.reference.trim().is_empty()
                    && !image.reference.chars().any(char::is_whitespace),
                "image target {} from source {} has an invalid OCI reference",
                image.target,
                image.source
            );
            ensure!(
                source_targets.insert((image.source.clone(), image.target.clone())),
                "deployment source {} selects image target {} more than once",
                image.source,
                image.target
            );
            if let Some((owner_source, owner_target)) = references.insert(
                image.reference.clone(),
                (image.source.clone(), image.target.clone()),
            ) {
                anyhow::bail!(
                    "image reference {} collides between {}:{} and {}:{}",
                    image.reference,
                    owner_source,
                    owner_target,
                    image.source,
                    image.target
                );
            }
            if image.source == platform_source.name {
                platform_targets.insert(image.target.clone());
            }
        }

        let missing = required_platform_images
            .difference(&platform_targets)
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            missing.is_empty(),
            "deployment profile {} omits required Veoveo image targets from platform source {}: {}",
            self.name,
            platform_source.name,
            missing.join(", ")
        );
        Ok(())
    }
}

impl PlatformSelection {
    /// Expands one preset into an exact, dependency-checked selection.
    pub fn resolve(&self) -> Result<ResolvedPlatformSelection> {
        let (components, mcp_servers) = match self.installation_preset {
            InstallationPreset::Full => {
                ensure!(
                    self.components.is_empty() && self.mcp_servers.is_empty(),
                    "full preset does not accept explicit components or mcpServers"
                );
                (
                    PlatformComponent::all(),
                    FirstPartyMcpServer::all_supported(),
                )
            }
            InstallationPreset::ExtensionFoundation => {
                ensure!(
                    self.components.is_empty() && self.mcp_servers.is_empty(),
                    "extension-foundation preset does not accept explicit components or mcpServers"
                );
                (
                    BTreeSet::from([
                        PlatformComponent::Gateway,
                        PlatformComponent::PlatformStore,
                        PlatformComponent::ObjectStore,
                        PlatformComponent::ArtifactService,
                        PlatformComponent::RecordingDataPlane,
                    ]),
                    BTreeSet::from([
                        FirstPartyMcpServer::Artifact,
                        FirstPartyMcpServer::Frames,
                        FirstPartyMcpServer::Recording,
                    ]),
                )
            }
            InstallationPreset::Custom => {
                ensure!(
                    !self.components.is_empty(),
                    "custom components cannot be empty"
                );
                (self.components.clone(), self.mcp_servers.clone())
            }
        };
        let resolved = ResolvedPlatformSelection {
            components,
            mcp_servers,
            artifact_audiences: self.artifact_audiences.clone(),
            external_workloads: self.external_workloads.clone(),
            gpu_scheduling: self.gpu_scheduling.clone(),
        };
        resolved.validate_dependencies()?;
        Ok(resolved)
    }
}

impl PlatformComponent {
    fn all() -> BTreeSet<Self> {
        BTreeSet::from([
            Self::Gateway,
            Self::PlatformStore,
            Self::ObjectStore,
            Self::ArtifactService,
            Self::RecordingDataPlane,
            Self::GpuRenderer,
            Self::SimulationRuntimeSupport,
            Self::Console,
            Self::Telemetry,
            Self::Ingress,
        ])
    }
}

impl FirstPartyMcpServer {
    fn all_supported() -> BTreeSet<Self> {
        BTreeSet::from([
            Self::Artifact,
            Self::Media,
            Self::Timeseries,
            Self::Optimization,
            Self::Frames,
            Self::Map,
            Self::Time,
            Self::View,
            Self::Datasheet,
            Self::Duckdb,
            Self::Chart,
            Self::Rerun,
            Self::Recording,
            Self::Stream,
            Self::Reason,
            Self::SimulationView,
        ])
    }
}

impl ResolvedPlatformSelection {
    /// Returns the Veoveo-owned OCI image names required by this exact
    /// component and MCP-server selection.
    #[must_use]
    pub fn required_images(&self) -> BTreeSet<String> {
        let mut images = BTreeSet::new();
        for component in &self.components {
            images.extend(component.images().iter().map(|image| (*image).to_owned()));
        }
        for server in &self.mcp_servers {
            images.extend(server.images().iter().map(|image| (*image).to_owned()));
        }
        images
    }

    /// Validates component dependencies without reading Helm state.
    pub fn validate_dependencies(&self) -> Result<()> {
        for audience in &self.artifact_audiences {
            validate_name("artifact audience", audience)?;
        }
        for workload in &self.external_workloads {
            validate_name("external workload", workload)?;
        }
        if !self.mcp_servers.is_empty() {
            self.require_component(
                PlatformComponent::Gateway,
                "hosted MCP servers require gateway",
            )?;
        }
        if self
            .components
            .contains(&PlatformComponent::ArtifactService)
        {
            self.require_component(
                PlatformComponent::PlatformStore,
                "artifact service requires platform store",
            )?;
            self.require_component(
                PlatformComponent::ObjectStore,
                "artifact service requires object store",
            )?;
        }
        if self.components.contains(&PlatformComponent::Console)
            || self.components.contains(&PlatformComponent::Ingress)
        {
            self.require_component(
                PlatformComponent::Gateway,
                "console and ingress require gateway",
            )?;
        }
        if self
            .components
            .contains(&PlatformComponent::RecordingDataPlane)
        {
            self.require_component(
                PlatformComponent::PlatformStore,
                "recording data plane requires platform store",
            )?;
            self.require_component(
                PlatformComponent::ArtifactService,
                "recording data plane requires artifact service",
            )?;
        }
        if self.components.contains(&PlatformComponent::GpuRenderer) {
            self.require_component(
                PlatformComponent::SimulationRuntimeSupport,
                "GPU renderer requires simulation runtime support",
            )?;
            self.require_server(
                FirstPartyMcpServer::SimulationView,
                "GPU renderer requires Simulation View MCP",
            )?;
        }
        if self
            .mcp_servers
            .contains(&FirstPartyMcpServer::SimulationView)
        {
            self.require_component(
                PlatformComponent::ArtifactService,
                "Simulation View scene materialization requires the artifact service",
            )?;
            self.require_component(
                PlatformComponent::GpuRenderer,
                "Simulation View MCP requires the GPU renderer",
            )?;
            self.require_server(
                FirstPartyMcpServer::Frames,
                "Simulation View scene declarations require Frames MCP",
            )?;
            self.require_artifact_audience(
                "simulation-view",
                "Simulation View scene materialization requires its Artifact data-plane audience",
            )?;
        }

        let platform_store_servers = [
            FirstPartyMcpServer::Artifact,
            FirstPartyMcpServer::Media,
            FirstPartyMcpServer::Timeseries,
            FirstPartyMcpServer::Optimization,
            FirstPartyMcpServer::Frames,
            FirstPartyMcpServer::Map,
            FirstPartyMcpServer::Time,
            FirstPartyMcpServer::View,
            FirstPartyMcpServer::Datasheet,
            FirstPartyMcpServer::Duckdb,
            FirstPartyMcpServer::Recording,
            FirstPartyMcpServer::Stream,
            FirstPartyMcpServer::Reason,
        ];
        if platform_store_servers
            .iter()
            .any(|server| self.mcp_servers.contains(server))
        {
            self.require_component(
                PlatformComponent::PlatformStore,
                "selected MCP servers require platform store",
            )?;
        }

        let artifact_servers = [
            FirstPartyMcpServer::Artifact,
            FirstPartyMcpServer::Media,
            FirstPartyMcpServer::Timeseries,
            FirstPartyMcpServer::Optimization,
            FirstPartyMcpServer::Frames,
            FirstPartyMcpServer::Map,
            FirstPartyMcpServer::Datasheet,
            FirstPartyMcpServer::Duckdb,
            FirstPartyMcpServer::Recording,
            FirstPartyMcpServer::Stream,
            FirstPartyMcpServer::Reason,
        ];
        if artifact_servers
            .iter()
            .any(|server| self.mcp_servers.contains(server))
        {
            self.require_component(
                PlatformComponent::ArtifactService,
                "selected MCP servers require artifact service",
            )?;
            self.require_component(
                PlatformComponent::ObjectStore,
                "selected MCP servers require object store",
            )?;
        }
        if self.mcp_servers.contains(&FirstPartyMcpServer::Reason) {
            self.require_server(FirstPartyMcpServer::Recording, "reason requires recording")?;
        }
        if self.mcp_servers.contains(&FirstPartyMcpServer::Recording) {
            self.require_component(
                PlatformComponent::RecordingDataPlane,
                "Recording MCP requires the recording data plane",
            )?;
        }
        self.validate_gpu_scheduling()?;
        Ok(())
    }

    /// Checks composed extension requirements against the selected runtime.
    pub fn satisfy(&self, requirements: &GatewayDeploymentRequirements) -> Result<()> {
        for capability in &requirements.platform_capabilities {
            match capability {
                PlatformCapability::Artifact => {
                    self.require_server(
                        FirstPartyMcpServer::Artifact,
                        "artifact capability requires Artifact MCP",
                    )?;
                }
                PlatformCapability::Frames => {
                    self.require_server(
                        FirstPartyMcpServer::Frames,
                        "frames capability requires Frames MCP",
                    )?;
                }
                PlatformCapability::Map => {
                    self.require_server(
                        FirstPartyMcpServer::Map,
                        "map capability requires Map MCP",
                    )?;
                }
                PlatformCapability::Media => {
                    self.require_server(
                        FirstPartyMcpServer::Media,
                        "media capability requires Media MCP",
                    )?;
                }
                PlatformCapability::Recording | PlatformCapability::Rrd => {
                    self.require_server(
                        FirstPartyMcpServer::Recording,
                        "recording and rrd capabilities require Recording MCP and hub",
                    )?;
                }
                PlatformCapability::SimulationView => {
                    self.require_server(
                        FirstPartyMcpServer::SimulationView,
                        "simulation_view capability requires Simulation View MCP",
                    )?;
                }
            }
        }
        for audience in &requirements.artifact_audiences {
            ensure!(
                self.artifact_audiences.contains(audience),
                "gateway composition requires artifact audience {audience}, but the platform selection does not admit it"
            );
        }
        Ok(())
    }

    fn validate_gpu_scheduling(&self) -> Result<()> {
        let mut required = BTreeSet::new();
        if self.components.contains(&PlatformComponent::GpuRenderer) {
            required.insert("simulation-view-renderer");
        }
        if self.mcp_servers.contains(&FirstPartyMcpServer::View) {
            required.insert("view-renderer");
        }
        if self.mcp_servers.contains(&FirstPartyMcpServer::Stream) {
            required.insert("stream");
        }
        if self.mcp_servers.contains(&FirstPartyMcpServer::Reason) {
            required.insert("reason");
        }
        if self
            .mcp_servers
            .contains(&FirstPartyMcpServer::Optimization)
        {
            required.insert("cuopt-executor");
        }
        if required.is_empty() {
            ensure!(
                self.gpu_scheduling.is_none(),
                "gpuScheduling is present but no selected first-party workload requires a GPU"
            );
            return Ok(());
        }

        let scheduling = self
            .gpu_scheduling
            .as_ref()
            .context("selected GPU workloads require gpuScheduling")?;
        validate_name("GPU RuntimeClass", &scheduling.runtime_class_name)?;
        ensure!(
            scheduling.allocatable_devices > 0,
            "gpuScheduling allocatableDevices must be positive"
        );
        ensure!(
            !scheduling.workloads.is_empty(),
            "gpuScheduling workloads cannot be empty"
        );
        ensure_unique(
            "GPU workload",
            scheduling.workloads.iter().map(|item| &item.workload),
        )?;

        let mut exclusive_devices = 0_u32;
        let mut shared_devices = 0_u32;
        let mut declared = BTreeSet::new();
        for placement in &scheduling.workloads {
            validate_name("GPU workload", &placement.workload)?;
            ensure!(
                placement.devices > 0,
                "GPU workload {} must request at least one device",
                placement.workload
            );
            declared.insert(placement.workload.as_str());
            match placement.isolation {
                GpuIsolation::Exclusive => {
                    ensure!(
                        placement.evidence_digest.is_none(),
                        "exclusive GPU workload {} must not declare sharing evidence",
                        placement.workload
                    );
                    exclusive_devices += u32::from(placement.devices);
                }
                GpuIsolation::NvidiaTimeSlicing | GpuIsolation::NvidiaMig => {
                    let digest = placement.evidence_digest.as_deref().with_context(|| {
                        format!(
                            "shared GPU workload {} requires measured evidenceDigest",
                            placement.workload
                        )
                    })?;
                    validate_digest(digest)?;
                    shared_devices = shared_devices.max(u32::from(placement.devices));
                }
            }
        }
        for workload in required {
            ensure!(
                declared.contains(workload),
                "gpuScheduling is missing selected workload {workload}"
            );
        }
        ensure!(
            exclusive_devices + shared_devices <= u32::from(scheduling.allocatable_devices),
            "GPU workload selection requires {} physical devices but gpuScheduling exposes {}; two exclusive one-GPU workloads cannot share one ordinary GPU",
            exclusive_devices + shared_devices,
            scheduling.allocatable_devices
        );
        Ok(())
    }

    fn require_component(&self, component: PlatformComponent, reason: &str) -> Result<()> {
        ensure!(
            self.components.contains(&component),
            "{reason}; missing component {component:?}"
        );
        Ok(())
    }

    fn require_server(&self, server: FirstPartyMcpServer, reason: &str) -> Result<()> {
        ensure!(
            self.mcp_servers.contains(&server),
            "{reason}; missing mcpServer {server:?}"
        );
        Ok(())
    }

    fn require_artifact_audience(&self, audience: &str, reason: &str) -> Result<()> {
        ensure!(
            self.artifact_audiences.contains(audience),
            "{reason}; missing artifactAudience {audience}"
        );
        Ok(())
    }
}

impl PlatformComponent {
    fn images(self) -> &'static [&'static str] {
        match self {
            Self::Gateway => &["mcp-gateway"],
            Self::ArtifactService => &["artifact-service"],
            Self::RecordingDataPlane => &["recording-hub", "recording-forwarder"],
            Self::GpuRenderer => &["simulation-view-isaac", "simulation-view-pose"],
            Self::SimulationRuntimeSupport => &["simulation-runtime"],
            Self::Console => &["console-bff"],
            Self::PlatformStore | Self::ObjectStore | Self::Telemetry | Self::Ingress => &[],
        }
    }
}

impl FirstPartyMcpServer {
    fn images(self) -> &'static [&'static str] {
        match self {
            Self::Artifact => &["artifact-mcp"],
            Self::Media => &["media-mcp"],
            Self::Timeseries => &["timeseries-mcp"],
            Self::Optimization => &["cuopt-executor", "optimization-mcp"],
            Self::Frames => &["frames-mcp"],
            Self::Map => &["map-mcp"],
            Self::Time => &["time-mcp"],
            Self::View => &["view-mcp"],
            Self::Datasheet => &["datasheet-mcp"],
            Self::Duckdb => &["duckdb-mcp"],
            Self::Chart => &["chart-mcp"],
            Self::Rerun => &["mcp-stdio-bridge"],
            Self::Recording => &["recording-mcp"],
            Self::Stream => &["stream-mcp"],
            Self::Reason => &["reason-mcp"],
            Self::SimulationView => &["simulation-view-mcp"],
        }
    }
}

impl GatewayDeploymentRequirements {
    /// Returns Veoveo-owned producer-side images implied by composed
    /// capabilities. RRD transport uses the recording forwarder in the
    /// extension pod in addition to the platform's Recording MCP and hub.
    #[must_use]
    pub fn required_images(&self) -> BTreeSet<String> {
        if self
            .platform_capabilities
            .contains(&PlatformCapability::Rrd)
        {
            BTreeSet::from(["recording-forwarder".to_owned()])
        } else {
            BTreeSet::new()
        }
    }
}

impl DeploymentLock {
    /// Validates immutable source, image, and chart identities.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == DEPLOYMENT_LOCK_SCHEMA,
            "deployment lock schemaVersion must be {DEPLOYMENT_LOCK_SCHEMA}"
        );
        validate_name("profile", &self.profile)?;
        validate_registry_address(&self.registry)?;
        self.platform.validate_dependencies()?;
        ensure!(!self.sources.is_empty(), "locked sources cannot be empty");
        ensure_unique("locked source", self.sources.iter().map(|item| &item.name))?;
        let platform_sources = self
            .sources
            .iter()
            .filter(|source| source.role == DeploymentSourceRole::Platform)
            .count();
        ensure!(
            platform_sources == 1,
            "deployment lock must contain exactly one platform source, found {platform_sources}"
        );
        let mut image_repositories = BTreeMap::new();
        let mut release_names = BTreeMap::new();
        for source in &self.sources {
            validate_name("locked source", &source.name)?;
            ensure!(
                !source.repository.trim().is_empty()
                    && !source.repository.chars().any(char::is_whitespace),
                "locked source repository cannot be empty or contain whitespace"
            );
            validate_revision(&source.revision)?;
            ensure!(
                !source.images.is_empty() || !source.charts.is_empty(),
                "locked source {} contains no artifacts",
                source.name
            );
            ensure_unique("locked image", source.images.iter().map(|item| &item.name))?;
            ensure_unique(
                "locked Helm release",
                source.charts.iter().map(|item| &item.release),
            )?;
            for image in &source.images {
                validate_name("locked image", &image.name)?;
                ensure!(
                    !image.repository.contains('@') && !image.repository.ends_with(":latest"),
                    "locked image repository must not carry a mutable tag or digest"
                );
                validate_digest(&image.digest)?;
                validate_digest(&image.publication_digest)?;
                ensure!(
                    image.digest != image.publication_digest,
                    "locked image {} must distinguish its runnable manifest digest from its attested publication digest",
                    image.name
                );
                if let Some((owner_source, owner_image)) = image_repositories.insert(
                    image.repository.clone(),
                    (source.name.clone(), image.name.clone()),
                ) {
                    anyhow::bail!(
                        "locked image repository {} is owned by both {}:{} and {}:{}",
                        image.repository,
                        owner_source,
                        owner_image,
                        source.name,
                        image.name
                    );
                }
            }
            for chart in &source.charts {
                validate_name("locked Helm release", &chart.release)?;
                if let Some(owner_source) =
                    release_names.insert(chart.release.clone(), source.name.clone())
                {
                    anyhow::bail!(
                        "locked Helm release {} is owned by both {} and {}",
                        chart.release,
                        owner_source,
                        source.name
                    );
                }
                ensure!(
                    chart.coordinate.starts_with("oci://")
                        || chart.coordinate.starts_with("source://"),
                    "locked chart coordinate must use oci:// or source://"
                );
                ensure!(
                    !chart.coordinate.ends_with(":latest"),
                    "locked chart coordinate must not use latest"
                );
                validate_digest(&chart.digest)?;
            }
        }
        Ok(())
    }
}

impl LocalRegistrySpec {
    /// Returns the registry address visible from k3d nodes.
    pub fn address(&self) -> Result<String> {
        let (_, port) = self
            .host_port
            .rsplit_once(':')
            .context("local registry hostPort must be HOST:PORT")?;
        ensure!(
            port.parse::<u16>().is_ok(),
            "local registry hostPort is invalid"
        );
        Ok(format!("k3d-{}:{port}", self.name))
    }

    /// Returns the Docker container name.
    #[must_use]
    pub fn container_name(&self) -> String {
        format!("k3d-{}", self.name)
    }
}

/// Loads and validates a local registry declaration.
pub fn load_local_registry(path: &Path) -> Result<LocalRegistrySpec> {
    let registry = serde_json::from_slice::<LocalRegistrySpec>(
        &fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("decoding {}", path.display()))?;
    ensure!(
        registry.schema_version == REGISTRY_SCHEMA,
        "local registry schemaVersion must be {REGISTRY_SCHEMA}"
    );
    for segment in registry.name.split('.') {
        validate_name("local registry segment", segment)?;
    }
    ensure!(
        !registry.image.trim().is_empty(),
        "local registry image is empty"
    );
    ensure!(
        registry.image.contains("@sha256:"),
        "local registry image must use an immutable digest"
    );
    ensure!(
        registry.volume.ends_with(":/var/lib/registry"),
        "local registry volume must mount /var/lib/registry"
    );
    let _ = registry.address()?;
    Ok(registry)
}

/// Generates the canonical deployment profile schema.
#[must_use]
pub fn deployment_profile_schema() -> schemars::Schema {
    schemars::schema_for!(DeploymentProfile)
}

/// Generates the canonical immutable deployment lock schema.
#[must_use]
pub fn deployment_lock_schema() -> schemars::Schema {
    schemars::schema_for!(DeploymentLock)
}

fn validate_releases(
    source: &DeploymentSource,
    root: &Path,
    release_names: &mut BTreeSet<String>,
) -> Result<()> {
    validate_release_metadata(source, release_names)?;
    for release in &source.releases {
        require_directory(&root.join(&release.chart), "Helm chart")?;
        for values in &release.values {
            require_file(&root.join(values), "Helm values")?;
        }
    }
    Ok(())
}

fn validate_release_metadata(
    source: &DeploymentSource,
    release_names: &mut BTreeSet<String>,
) -> Result<()> {
    for release in &source.releases {
        match source.role {
            DeploymentSourceRole::Platform => ensure!(
                release.values_contract != ReleaseValuesContract::Extension,
                "platform source {} cannot own extension Helm release {}",
                source.name,
                release.name
            ),
            DeploymentSourceRole::Extension => ensure!(
                release.values_contract == ReleaseValuesContract::Extension,
                "extension source {} must use the extension values contract for Helm release {}",
                source.name,
                release.name
            ),
        }
        validate_name("Helm release", &release.name)?;
        ensure!(
            release_names.insert(release.name.clone()),
            "duplicate Helm release: {}",
            release.name
        );
        ensure!(release.timeout_seconds > 0, "Helm timeout must be positive");
        ensure!(
            !release.chart.as_os_str().is_empty() && !release.chart.is_absolute(),
            "Helm chart path for {} must be source-relative",
            release.name
        );
        for values in &release.values {
            ensure!(
                !values.as_os_str().is_empty() && !values.is_absolute(),
                "Helm values path for {} must be source-relative",
                release.name
            );
        }
    }
    Ok(())
}

fn validate_git_url(value: &str) -> Result<()> {
    let url = Url::parse(value).with_context(|| format!("invalid Git source URL {value}"))?;
    ensure!(
        matches!(url.scheme(), "https" | "ssh"),
        "Git source URL must use https or ssh"
    );
    ensure!(
        url.host_str().is_some() && url.password().is_none(),
        "Git source URL must have a host and contain no password"
    );
    ensure!(
        url.query().is_none() && url.fragment().is_none(),
        "Git source URL must not contain a query or fragment"
    );
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

fn validate_revision(revision: &str) -> Result<()> {
    ensure!(
        matches!(revision.len(), 40 | 64)
            && revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "source revision must be a full lowercase SHA-1 or SHA-256 object id"
    );
    Ok(())
}

fn validate_digest(digest: &str) -> Result<()> {
    let Some(value) = digest.strip_prefix("sha256:") else {
        anyhow::bail!("artifact digest must start with sha256:");
    };
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "artifact digest must contain 64 lowercase hexadecimal digits"
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, path::Path};

    use jsonschema::Validator;

    use super::{
        DeploymentLock, DeploymentSourceRole, FirstPartyMcpServer, GatewayDeploymentRequirements,
        GpuIsolation, GpuSchedulingProfile, GpuWorkloadPlacement, InstallationPreset,
        LoadedProfile, PlannedImage, PlatformCapability, PlatformComponent, PlatformSelection,
        deployment_lock_schema, deployment_profile_schema,
    };

    #[test]
    fn loads_repository_profile() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile = repository.join("showcase/sumo/deploy/deployment.json");
        let loaded = LoadedProfile::load(&profile, &repository).expect("load SUMO profile");
        assert_eq!(loaded.definition.name, "sumo");
        assert_eq!(loaded.definition.sources.len(), 1);
        assert_eq!(loaded.definition.sources[0].image_groups.len(), 3);
        loaded.resolved_platform().expect("resolve platform");
    }

    #[test]
    fn loads_anonymous_external_extension_installation() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile =
            repository.join("testing/fixtures/external-extension-installation/deployment.json");
        let loaded =
            LoadedProfile::load(&profile, &repository).expect("load external extension profile");
        assert_eq!(
            loaded
                .required_platform_images()
                .expect("resolve image closure"),
            BTreeSet::from([
                "artifact-mcp".to_owned(),
                "artifact-service".to_owned(),
                "frames-mcp".to_owned(),
                "map-mcp".to_owned(),
                "mcp-gateway".to_owned(),
                "media-mcp".to_owned(),
                "recording-forwarder".to_owned(),
                "recording-hub".to_owned(),
                "recording-mcp".to_owned(),
            ])
        );
    }

    #[test]
    fn loads_checked_multi_source_deployment_lock() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = repository
            .join("testing/fixtures/external-simulation-installation/deployment.lock.json");
        let bytes = fs::read(&path).expect("read checked deployment lock");
        let lock = serde_json::from_slice::<DeploymentLock>(&bytes)
            .expect("decode checked deployment lock");

        lock.validate().expect("validate checked deployment lock");
        assert!(
            lock.sources
                .iter()
                .flat_map(|source| &source.images)
                .all(|image| image.digest != image.publication_digest)
        );
    }

    #[test]
    fn extension_targets_cannot_satisfy_platform_image_closure() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile =
            repository.join("testing/fixtures/external-extension-installation/deployment.json");
        let loaded =
            LoadedProfile::load(&profile, &repository).expect("load external extension profile");
        let required = loaded
            .required_platform_images()
            .expect("resolve platform image closure");
        let mut definition = loaded.definition.clone();
        let mut extension = definition.sources[0].clone();
        extension.name = "anonymous-extension".to_owned();
        extension.role = DeploymentSourceRole::Extension;
        definition.sources.push(extension);
        let images = required
            .iter()
            .map(|target| PlannedImage {
                source: if target == "frames-mcp" {
                    "anonymous-extension"
                } else {
                    "veoveo"
                }
                .to_owned(),
                target: target.clone(),
                reference: format!("registry.example.internal/{target}:revision"),
            })
            .collect::<Vec<_>>();

        let error = definition
            .validate_image_plan(&images, &required)
            .expect_err("extension target cannot satisfy platform closure");

        assert!(error.to_string().contains("frames-mcp"));
    }

    #[test]
    fn image_plan_rejects_duplicate_targets_and_references() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile =
            repository.join("testing/fixtures/external-extension-installation/deployment.json");
        let loaded =
            LoadedProfile::load(&profile, &repository).expect("load external extension profile");
        let required = loaded
            .required_platform_images()
            .expect("resolve platform image closure");
        let mut images = required
            .iter()
            .map(|target| PlannedImage {
                source: "veoveo".to_owned(),
                target: target.clone(),
                reference: format!("registry.example.internal/veoveo/{target}:revision"),
            })
            .collect::<Vec<_>>();
        images.push(images[0].clone());
        assert!(
            loaded
                .validate_image_plan(&images)
                .expect_err("duplicate target must fail")
                .to_string()
                .contains("selects image target")
        );

        images.pop();
        let collision = images[0].reference.clone();
        images.push(PlannedImage {
            source: "veoveo".to_owned(),
            target: "different-target".to_owned(),
            reference: collision,
        });
        assert!(
            loaded
                .validate_image_plan(&images)
                .expect_err("duplicate reference must fail")
                .to_string()
                .contains("collides between")
        );
    }

    #[test]
    fn requirements_fail_closed_when_runtime_servers_are_absent() {
        let selection = PlatformSelection {
            installation_preset: InstallationPreset::Custom,
            components: BTreeSet::from([
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::ObjectStore,
                PlatformComponent::ArtifactService,
                PlatformComponent::RecordingDataPlane,
            ]),
            mcp_servers: BTreeSet::from([
                FirstPartyMcpServer::Artifact,
                FirstPartyMcpServer::Frames,
                FirstPartyMcpServer::Recording,
            ]),
            artifact_audiences: BTreeSet::from(["anonymous".to_owned()]),
            external_workloads: BTreeSet::new(),
            gpu_scheduling: None,
        }
        .resolve()
        .expect("valid minimal selection");
        let requirements = GatewayDeploymentRequirements {
            platform_capabilities: BTreeSet::from([
                PlatformCapability::Frames,
                PlatformCapability::Map,
                PlatformCapability::Media,
                PlatformCapability::Rrd,
            ]),
            artifact_audiences: BTreeSet::from(["anonymous".to_owned()]),
        };
        let error = selection
            .satisfy(&requirements)
            .expect_err("Map and Media must be selected");
        assert!(error.to_string().contains("Map MCP"));
    }

    #[test]
    fn requirements_accept_all_declared_runtime_servers() {
        let selection = PlatformSelection {
            installation_preset: InstallationPreset::Custom,
            components: BTreeSet::from([
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::ObjectStore,
                PlatformComponent::ArtifactService,
                PlatformComponent::RecordingDataPlane,
            ]),
            mcp_servers: BTreeSet::from([
                FirstPartyMcpServer::Artifact,
                FirstPartyMcpServer::Frames,
                FirstPartyMcpServer::Map,
                FirstPartyMcpServer::Media,
                FirstPartyMcpServer::Recording,
            ]),
            artifact_audiences: BTreeSet::from(["anonymous".to_owned()]),
            external_workloads: BTreeSet::new(),
            gpu_scheduling: None,
        }
        .resolve()
        .expect("valid selection");
        selection
            .satisfy(&GatewayDeploymentRequirements {
                platform_capabilities: BTreeSet::from([
                    PlatformCapability::Artifact,
                    PlatformCapability::Frames,
                    PlatformCapability::Map,
                    PlatformCapability::Media,
                    PlatformCapability::Recording,
                    PlatformCapability::Rrd,
                ]),
                artifact_audiences: BTreeSet::from(["anonymous".to_owned()]),
            })
            .expect("all runtime requirements selected");
    }

    #[test]
    fn platform_image_closure_includes_service_and_rrd_transport_images() {
        let selection = PlatformSelection {
            installation_preset: InstallationPreset::Custom,
            components: BTreeSet::from([
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::ObjectStore,
                PlatformComponent::ArtifactService,
                PlatformComponent::RecordingDataPlane,
            ]),
            mcp_servers: BTreeSet::from([
                FirstPartyMcpServer::Artifact,
                FirstPartyMcpServer::Frames,
                FirstPartyMcpServer::Map,
                FirstPartyMcpServer::Media,
                FirstPartyMcpServer::Recording,
            ]),
            artifact_audiences: BTreeSet::from(["anonymous".to_owned()]),
            external_workloads: BTreeSet::new(),
            gpu_scheduling: None,
        }
        .resolve()
        .expect("valid extension platform");
        let requirements = GatewayDeploymentRequirements {
            platform_capabilities: BTreeSet::from([
                PlatformCapability::Artifact,
                PlatformCapability::Frames,
                PlatformCapability::Map,
                PlatformCapability::Media,
                PlatformCapability::Recording,
                PlatformCapability::Rrd,
            ]),
            artifact_audiences: BTreeSet::from(["anonymous".to_owned()]),
        };
        let mut images = selection.required_images();
        images.extend(requirements.required_images());
        assert_eq!(
            images,
            BTreeSet::from([
                "artifact-mcp".to_owned(),
                "artifact-service".to_owned(),
                "frames-mcp".to_owned(),
                "map-mcp".to_owned(),
                "mcp-gateway".to_owned(),
                "media-mcp".to_owned(),
                "recording-forwarder".to_owned(),
                "recording-hub".to_owned(),
                "recording-mcp".to_owned(),
            ])
        );
    }

    #[test]
    fn optimization_image_closure_includes_gpu_executor() {
        let selection = PlatformSelection {
            installation_preset: InstallationPreset::Custom,
            components: BTreeSet::from([
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::ObjectStore,
                PlatformComponent::ArtifactService,
            ]),
            mcp_servers: BTreeSet::from([FirstPartyMcpServer::Optimization]),
            artifact_audiences: BTreeSet::from(["optimization".to_owned()]),
            external_workloads: BTreeSet::new(),
            gpu_scheduling: Some(GpuSchedulingProfile {
                runtime_class_name: "nvidia".to_owned(),
                allocatable_devices: 1,
                workloads: vec![GpuWorkloadPlacement {
                    workload: "cuopt-executor".to_owned(),
                    devices: 1,
                    isolation: GpuIsolation::Exclusive,
                    evidence_digest: None,
                }],
            }),
        }
        .resolve()
        .expect("valid Optimization selection");
        assert_eq!(
            selection.required_images(),
            BTreeSet::from([
                "artifact-service".to_owned(),
                "cuopt-executor".to_owned(),
                "mcp-gateway".to_owned(),
                "optimization-mcp".to_owned(),
            ])
        );
    }

    #[test]
    fn simulation_view_image_closure_is_complete() {
        let selection = PlatformSelection {
            installation_preset: InstallationPreset::Custom,
            components: BTreeSet::from([
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::ObjectStore,
                PlatformComponent::ArtifactService,
                PlatformComponent::GpuRenderer,
                PlatformComponent::SimulationRuntimeSupport,
            ]),
            mcp_servers: BTreeSet::from([
                FirstPartyMcpServer::Frames,
                FirstPartyMcpServer::SimulationView,
            ]),
            artifact_audiences: BTreeSet::from(["simulation-view".to_owned()]),
            external_workloads: BTreeSet::new(),
            gpu_scheduling: Some(GpuSchedulingProfile {
                runtime_class_name: "nvidia".to_owned(),
                allocatable_devices: 1,
                workloads: vec![GpuWorkloadPlacement {
                    workload: "simulation-view-renderer".to_owned(),
                    devices: 1,
                    isolation: GpuIsolation::Exclusive,
                    evidence_digest: None,
                }],
            }),
        }
        .resolve()
        .expect("valid Simulation View selection");
        assert_eq!(
            selection.required_images(),
            BTreeSet::from([
                "artifact-service".to_owned(),
                "frames-mcp".to_owned(),
                "mcp-gateway".to_owned(),
                "simulation-runtime".to_owned(),
                "simulation-view-isaac".to_owned(),
                "simulation-view-mcp".to_owned(),
                "simulation-view-pose".to_owned(),
            ])
        );
        let mut missing_artifact_audience = selection;
        missing_artifact_audience.artifact_audiences.clear();
        let error = missing_artifact_audience
            .validate_dependencies()
            .expect_err("Simulation View cannot materialize scenes without its Artifact audience");
        assert!(
            error
                .to_string()
                .contains("missing artifactAudience simulation-view")
        );
    }

    #[test]
    fn exclusive_simulation_gpu_workloads_fail_on_one_device() {
        let error = PlatformSelection {
            installation_preset: InstallationPreset::Custom,
            components: BTreeSet::from([
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::ObjectStore,
                PlatformComponent::ArtifactService,
                PlatformComponent::GpuRenderer,
                PlatformComponent::SimulationRuntimeSupport,
            ]),
            mcp_servers: BTreeSet::from([
                FirstPartyMcpServer::Frames,
                FirstPartyMcpServer::SimulationView,
            ]),
            artifact_audiences: BTreeSet::from(["simulation-view".to_owned()]),
            external_workloads: BTreeSet::from(["external-simulator".to_owned()]),
            gpu_scheduling: Some(GpuSchedulingProfile {
                runtime_class_name: "nvidia".to_owned(),
                allocatable_devices: 1,
                workloads: vec![
                    GpuWorkloadPlacement {
                        workload: "simulation-view-renderer".to_owned(),
                        devices: 1,
                        isolation: GpuIsolation::Exclusive,
                        evidence_digest: None,
                    },
                    GpuWorkloadPlacement {
                        workload: "external-simulator".to_owned(),
                        devices: 1,
                        isolation: GpuIsolation::Exclusive,
                        evidence_digest: None,
                    },
                ],
            }),
        }
        .resolve()
        .expect_err("two exclusive workloads cannot fit one GPU");
        assert!(error.to_string().contains("cannot share one ordinary GPU"));
    }

    #[test]
    fn generated_schemas_are_closed_and_compile() {
        for schema in [deployment_profile_schema(), deployment_lock_schema()] {
            let value = serde_json::to_value(schema).expect("serialize schema");
            Validator::new(&value).expect("compile schema");
        }
    }
}
