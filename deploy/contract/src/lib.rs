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
pub const PROFILE_SCHEMA: &str = "veoveo.io/deployment/v6";
/// Canonical immutable multi-source deployment lock.
pub const DEPLOYMENT_LOCK_SCHEMA: &str = "veoveo.io/deployment-lock/v6";
/// Canonical non-release image closure used by development GitOps deployments.
pub const DEVELOPMENT_IMAGE_LOCK_SCHEMA: &str = "veoveo.io/development-image-lock/v1";
/// Canonical local OCI registry declaration.
pub const REGISTRY_SCHEMA: &str = "veoveo.io/local-registry/v1";

/// Qualified NVIDIA DRA driver name selected by the supported GPU adapter.
pub const NVIDIA_DRA_DRIVER_NAME: &str = "gpu.nvidia.com";
/// Canonical chart coordinate for the supported NVIDIA DRA driver release.
pub const NVIDIA_DRA_CHART_COORDINATE: &str =
    "oci://registry.k8s.io/dra-driver-nvidia/charts/dra-driver-nvidia-gpu";
/// Latest stable NVIDIA DRA driver release qualified by this contract.
pub const NVIDIA_DRA_VERSION: &str = "0.4.1";
/// OCI manifest digest for the qualified NVIDIA DRA Helm chart.
pub const NVIDIA_DRA_CHART_DIGEST: &str =
    "sha256:7a00373fdef1025f27ebb1d353719446bbbe6ec4697e9a503c5ffd7e4f1525dd";
/// Digest of the exact qualified Helm chart archive.
pub const NVIDIA_DRA_CHART_CONTENT_DIGEST: &str =
    "sha256:c1c316f6bdcfe5fed3ff649cff1b43be50d27d0cb1aaf9d29e7bdca1eaa331ce";
/// Canonical container repository used by the qualified NVIDIA DRA chart.
pub const NVIDIA_DRA_IMAGE_REPOSITORY: &str =
    "registry.k8s.io/dra-driver-nvidia/dra-driver-nvidia-gpu";
/// Multi-platform OCI index digest for the qualified NVIDIA DRA image.
pub const NVIDIA_DRA_IMAGE_DIGEST: &str =
    "sha256:eefe67396dedea4df74f68a94d5883f33204888b83979babd42b91501a2de1d8";
/// Linux AMD64 manifest digest within the qualified NVIDIA DRA image index.
pub const NVIDIA_DRA_IMAGE_AMD64_DIGEST: &str =
    "sha256:ad86983849542f6ef22f02e963ecbf545706e037455e0c265889ace137863556";
/// Linux ARM64 manifest digest within the qualified NVIDIA DRA image index.
pub const NVIDIA_DRA_IMAGE_ARM64_DIGEST: &str =
    "sha256:b51290bbc1ee6745adf8ffff040d2b917d3e07dbd5cd36fd444b0e371ccc9166";
/// Exact Kubernetes release qualified for the managed GPU allocator closure.
pub const NVIDIA_DRA_KUBERNETES_VERSION: &str = "1.36.2";
/// Exact Helm release qualified for the managed GPU allocator closure.
pub const NVIDIA_DRA_HELM_VERSION: &str = "4.2.3";
/// Exact host NVIDIA driver retained by the qualified GPU allocator closure.
pub const NVIDIA_DRA_HOST_DRIVER_VERSION: &str = "610.43.02";
/// Exact NVIDIA Container Toolkit package in the qualified repository-managed node image.
pub const NVIDIA_DRA_CONTAINER_TOOLKIT_PACKAGE_VERSION: &str = "1.19.1-1";

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
    /// Optional revisioned gateway document and public trust activation.
    pub gateway_activation: Option<GatewayActivationSpec>,
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
    /// Registry host and optional port reachable by the publication host.
    pub push_address: String,
    /// Registry host and optional port used by Kubernetes image pulls.
    pub pull_address: String,
    /// Transport and trust mode used by OCI and BuildKit clients.
    pub transport: RegistryTransport,
    /// Optional repository-local lifecycle declaration.
    pub local_config: Option<PathBuf>,
}

/// Immutable registry endpoints selected by a deployment lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockedRegistry {
    /// Registry host and optional port reachable by the publication host.
    pub push_address: String,
    /// Registry host and optional port used by Kubernetes image pulls.
    pub pull_address: String,
    /// Transport and trust mode shared by publication and pull clients.
    pub transport: RegistryTransport,
}

impl RegistryReference {
    /// Returns the immutable registry endpoints recorded in deployment evidence.
    #[must_use]
    pub fn locked(&self) -> LockedRegistry {
        LockedRegistry {
            push_address: self.push_address.clone(),
            pull_address: self.pull_address.clone(),
            transport: self.transport,
        }
    }

    fn validate(&self) -> Result<()> {
        validate_registry_address(&self.push_address)?;
        validate_registry_address(&self.pull_address)?;
        Ok(())
    }
}

impl LockedRegistry {
    fn validate(&self) -> Result<()> {
        validate_registry_address(&self.push_address)?;
        validate_registry_address(&self.pull_address)?;
        Ok(())
    }
}

/// Registry transport and trust profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RegistryTransport {
    /// HTTPS using trust roots already installed for the OCI client and BuildKit daemon.
    Tls,
    /// Explicitly admitted plaintext HTTP for a private development registry.
    InsecureHttp,
}

impl RegistryTransport {
    #[must_use]
    pub const fn is_insecure(self) -> bool {
        matches!(self, Self::InsecureHttp)
    }
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
    /// A separately selected Veoveo-owned showcase or application source.
    Workload,
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

/// Installation-owned gateway document, public trust, and confidential Secret binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayActivationSpec {
    /// Prefix for the immutable digest-qualified ConfigMap created by `profile-up`.
    pub config_map_name_prefix: String,
    /// ConfigMap key and mounted filename of the composed control-plane document.
    pub control_plane_key: String,
    /// Installation-repository path to the composed control-plane document.
    pub control_plane: PathBuf,
    /// Additional public mounted filename to installation-repository source path.
    #[serde(default)]
    pub public_files: BTreeMap<String, PathBuf>,
    /// Pre-existing Secret containing confidential gateway and identity material.
    pub confidential_secret: String,
    /// Exact Secret data keys that must exist before activation.
    pub required_secret_keys: BTreeSet<String>,
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
    /// Ordered chart-source-owned values files, resolved from the selected source revision.
    #[serde(default)]
    pub source_values: Vec<PathBuf>,
    /// Ordered installation-owned values files, resolved from the deployment profile repository.
    #[serde(default)]
    pub installation_values: Vec<PathBuf>,
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
    /// Canonical simulation runtime compatibility artifacts and conformance gates.
    SimulationRuntimeSupport,
    /// Continuously scheduled agent kernel artifact for external agent workloads.
    AgentRuntimeSupport,
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
    Optimization,
    Recording,
    Rrd,
}

/// Provider-neutral isolation selected for one physical-device group.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GpuIsolation {
    /// One workload replica owns the whole physical device.
    Exclusive,
    /// Multiple consumers share one physical device under a measured time-slice policy.
    MeasuredTimeSlicing,
    /// Consumers use hardware partitions of one physical device.
    Mig,
}

/// NVIDIA DRA time-slice interval compiled from a provider-neutral placement group.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GpuTimeSliceInterval {
    Short,
    Default,
    Long,
}

/// One workload consuming the physical device selected for its group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuWorkloadPlacement {
    /// Stable component or external workload identifier.
    pub workload: String,
    /// Kubernetes Deployment that owns this workload.
    pub deployment: String,
    /// Container receiving the DRA request.
    pub container: String,
    /// Kubernetes workload replicas. One replica is the normal platform contract.
    #[serde(default = "one_replica")]
    pub replicas: u16,
}

const fn one_replica() -> u16 {
    1
}

/// One named set of workloads that must resolve to the same physical device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuSamePhysicalDeviceGroup {
    /// Stable group and DRA request name.
    pub name: String,
    /// Workloads bound to this request.
    pub workloads: Vec<GpuWorkloadPlacement>,
    /// Isolation mechanism selected by the installation.
    pub isolation: GpuIsolation,
    /// Maximum replicas allowed to consume this physical device.
    pub maximum_consumers: u16,
    /// Digest of measured sharing evidence. Required for measured time slicing.
    pub evidence_digest: Option<String>,
    /// NVIDIA MIG profile name. Present only for MIG groups.
    pub mig_profile: Option<String>,
    /// Measured scheduling interval. Present only for time-sliced groups.
    pub time_slice_interval: Option<GpuTimeSliceInterval>,
}

/// Named groups that must resolve to different physical devices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuDifferentPhysicalDeviceConstraint {
    pub groups: BTreeSet<String>,
}

/// Exact OCI Helm chart selected for a managed cluster dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedOciChart {
    /// Tag-free OCI chart coordinate.
    pub coordinate: String,
    /// Exact chart version passed to the OCI registry.
    pub version: String,
    /// OCI chart-manifest digest reported by the registry.
    pub digest: String,
    /// SHA-256 digest of the downloaded chart archive installed by Helm.
    pub content_digest: String,
}

/// Exact multi-platform OCI image selected for a managed cluster dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedOciImage {
    /// Tag-free container repository.
    pub repository: String,
    /// Human-readable immutable release tag retained beside the digest.
    pub tag: String,
    /// Multi-platform OCI index digest used in rendered Pod specifications.
    pub digest: String,
    /// Exact platform manifests admitted beneath the image index.
    pub platform_digests: BTreeMap<String, String>,
}

/// Explicit acceptance of the upstream maturity of NVIDIA GPU allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum GpuAllocatorMaturityAcceptance {
    /// NVIDIA v0.4.1 marks GPU allocation as a technology-preview feature.
    TechnologyPreview,
}

/// Authorized removal of a conflicting NVIDIA device plugin from DRA-owned nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ConflictingGpuDevicePluginRemoval {
    /// The installation guarantees that no device plugin runs on selected nodes.
    RequireAbsent,
    /// Remove a conflicting installation-owned DaemonSet before DRA claims are created.
    DeleteDaemonSet { namespace: String, name: String },
    /// Uninstall a conflicting installation-owned Helm release after checking its chart version.
    UninstallHelmRelease {
        namespace: String,
        release_name: String,
        expected_chart_version: String,
    },
}

/// Complete managed installation of the qualified NVIDIA GPU DRA driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedGpuAllocatorInstallation {
    /// Cluster-wide Helm release identity.
    pub release_name: String,
    /// Dedicated namespace for the DRA kubelet plugin and chart-owned RBAC.
    pub namespace: String,
    /// Exact upstream chart artifact.
    pub chart: ManagedOciChart,
    /// Exact upstream driver image.
    pub image: ManagedOciImage,
    /// Host driver root mounted into the kubelet plugin.
    pub nvidia_driver_root: String,
    /// Existing installation-owned labels selecting the nodes VeoVeo may manage.
    pub eligible_node_selector: BTreeMap<String, String>,
    /// Authorized removal of an existing device plugin that conflicts with DRA ownership.
    pub conflicting_device_plugin_removal: ConflictingGpuDevicePluginRemoval,
    /// Required acknowledgment of the upstream GPU allocation maturity.
    pub maturity_acceptance: GpuAllocatorMaturityAcceptance,
    /// Bounded atomic Helm operation timeout.
    pub timeout_seconds: u64,
}

/// Kubernetes DRA implementation selected by the installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuDynamicResourceAllocator {
    /// Stable ResourceClaim name shared across pod replacement and Helm upgrades.
    pub claim_name: String,
    /// DRA DeviceClass for complete physical devices.
    pub full_device_class_name: String,
    /// DRA DeviceClass for MIG devices.
    pub mig_device_class_name: String,
    /// DRA driver that accepts opaque sharing configuration.
    pub driver_name: String,
    /// Driver configuration API compiled into opaque DRA parameters.
    pub configuration_api_version: String,
    /// Managed, digest-pinned installation of the selected allocator.
    pub installation: ManagedGpuAllocatorInstallation,
}

/// Installation GPU topology compiled to one durable DRA allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuSchedulingProfile {
    /// Kubernetes RuntimeClass used by every selected NVIDIA workload.
    pub runtime_class_name: String,
    /// Physical NVIDIA devices schedulable by this installation profile.
    pub allocatable_devices: u16,
    /// Digest of installation evidence proving this topology on the target hardware.
    pub evidence_digest: String,
    /// Supported physical-device allocation mechanism.
    pub allocator: GpuDynamicResourceAllocator,
    /// Exact same-physical-device groups.
    pub same_physical_device_groups: Vec<GpuSamePhysicalDeviceGroup>,
    /// Constraints whose group members must use different physical devices.
    #[serde(default)]
    pub different_physical_device_groups: Vec<GpuDifferentPhysicalDeviceConstraint>,
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
    /// Exact installation-repository revision that owns the profile and installation values.
    pub profile_revision: String,
    pub registry: LockedRegistry,
    pub sources: Vec<LockedSource>,
    pub platform: ResolvedPlatformSelection,
}

/// Digest-locked development image closure derived from one qualified deployment lock.
///
/// This contract deliberately cannot be consumed as a [`DeploymentLock`]. Staged images
/// retain runnable identity but do not carry the release attestations required by the
/// qualified deployment path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentImageLock {
    pub schema_version: String,
    pub release_eligible: bool,
    pub base_deployment_lock_digest: String,
    pub registry: LockedRegistry,
    pub images: Vec<DevelopmentLockedImage>,
}

/// One runnable image identity in a development closure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentLockedImage {
    pub source: String,
    pub target: String,
    pub repository: String,
    pub source_revision: String,
    pub runtime_digest: String,
    pub origin: DevelopmentImageOrigin,
}

/// Evidence lineage for one development image identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum DevelopmentImageOrigin {
    /// The runnable digest came unchanged from the qualified base closure.
    Qualified { publication_digest: String },
    /// The runnable digest came from a runtime-only staging publication.
    Staged {
        staging_index_digest: String,
        stage_evidence_digest: String,
    },
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
    /// Canonical profile document path.
    pub path: PathBuf,
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
            path,
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

    /// Returns every installation-repository file whose bytes affect profile
    /// validation, resource application, or Helm values.
    pub fn installation_inputs(&self) -> Result<BTreeSet<PathBuf>> {
        let mut paths = BTreeSet::from([self.path.clone()]);
        if let Some(path) = &self.definition.registry.local_config {
            paths.insert(self.resolve(path));
        }
        if let Some(cluster) = &self.definition.kubernetes.local_cluster {
            paths.insert(self.resolve(&cluster.config));
            paths.extend(
                cluster
                    .node_bootstrap_manifests
                    .iter()
                    .map(|path| self.resolve(path)),
            );
        }
        paths.extend(
            self.definition
                .resources
                .manifests
                .iter()
                .map(|path| self.resolve(path)),
        );
        if let Some(activation) = &self.definition.gateway_activation {
            paths.insert(self.resolve(&activation.control_plane));
            paths.extend(
                activation
                    .public_files
                    .values()
                    .map(|path| self.resolve(path)),
            );
        }
        paths.extend(
            self.definition
                .resources
                .config_maps
                .iter()
                .flat_map(|config_map| config_map.files.values())
                .map(|path| self.resolve(path)),
        );
        paths.extend(
            self.definition
                .gateway_requirements
                .iter()
                .map(|path| self.resolve(path)),
        );
        paths.extend(
            self.definition
                .sources
                .iter()
                .flat_map(|source| &source.releases)
                .flat_map(|release| &release.installation_values)
                .map(|path| self.resolve(path)),
        );
        paths
            .into_iter()
            .map(|path| {
                fs::canonicalize(&path)
                    .with_context(|| format!("resolving installation input {}", path.display()))
            })
            .collect()
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
        profile.registry.validate()?;
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
            if source.role == DeploymentSourceRole::Platform {
                ensure!(
                    source.image_groups.is_empty(),
                    "platform source {} must not declare imageGroups; its targets are derived from the exact platform selection",
                    source.name
                );
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
            for release in &source.releases {
                for values in &release.installation_values {
                    require_file(&self.resolve(values), "installation-owned Helm values")?;
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
                cluster_config.contains(&profile.registry.pull_address),
                "k3d cluster config must use registry {}",
                profile.registry.pull_address
            );
            for manifest in &cluster.node_bootstrap_manifests {
                require_file(
                    &self.resolve(manifest),
                    "local cluster node bootstrap manifest",
                )?;
            }
        }
        if let Some(config) = &profile.registry.local_config {
            ensure!(
                profile.registry.transport == RegistryTransport::InsecureHttp,
                "repository-managed local registries require registry transport insecure-http"
            );
            let registry = load_local_registry(&self.resolve(config))?;
            ensure!(
                registry.push_address()? == profile.registry.push_address,
                "local registry config host endpoint resolves to {}, profile uses {}",
                registry.push_address()?,
                profile.registry.push_address
            );
            ensure!(
                registry.pull_address()? == profile.registry.pull_address,
                "local registry config cluster endpoint resolves to {}, profile uses {}",
                registry.pull_address()?,
                profile.registry.pull_address
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
        if let Some(activation) = &profile.gateway_activation {
            validate_name(
                "gateway activation ConfigMap prefix",
                &activation.config_map_name_prefix,
            )?;
            ensure!(
                activation.config_map_name_prefix.len() <= 45,
                "gateway activation ConfigMap prefix must leave room for its digest suffix"
            );
            validate_data_key(&activation.control_plane_key)?;
            require_file(
                &self.resolve(&activation.control_plane),
                "gateway activation control plane",
            )?;
            ensure!(
                !activation
                    .public_files
                    .contains_key(&activation.control_plane_key),
                "gateway activation publicFiles must not replace controlPlaneKey {}",
                activation.control_plane_key
            );
            for (key, path) in &activation.public_files {
                validate_data_key(key)?;
                require_file(&self.resolve(path), "gateway activation public file")?;
            }
            validate_name(
                "gateway activation confidential Secret",
                &activation.confidential_secret,
            )?;
            ensure!(
                !activation.required_secret_keys.is_empty(),
                "gateway activation requiredSecretKeys cannot be empty"
            );
            for key in &activation.required_secret_keys {
                validate_data_key(key)?;
            }
            ensure!(
                !profile
                    .resources
                    .secrets
                    .iter()
                    .any(|secret| secret.name == activation.confidential_secret),
                "gateway activation confidential Secret {} must be installation-managed and must not be rewritten by profile resources",
                activation.confidential_secret
            );
        }
        for requirements in &profile.gateway_requirements {
            require_file(
                &self.resolve(requirements),
                "gateway composition requirements",
            )?;
        }
        for path in self.installation_inputs()? {
            ensure!(
                path.starts_with(&self.repository),
                "installation input {} is outside installation repository {}",
                path.display(),
                self.repository.display()
            );
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
        let unexpected = platform_targets
            .difference(required_platform_images)
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            unexpected.is_empty(),
            "deployment profile {} selects unnecessary Veoveo image targets from platform source {}: {}",
            self.name,
            platform_source.name,
            unexpected.join(", ")
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
            Self::SimulationRuntimeSupport,
            Self::AgentRuntimeSupport,
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
            .contains(&PlatformComponent::AgentRuntimeSupport)
        {
            self.require_component(
                PlatformComponent::Gateway,
                "agent runtime support requires gateway",
            )?;
            self.require_component(
                PlatformComponent::PlatformStore,
                "agent runtime support requires platform store",
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
                PlatformCapability::Optimization => {
                    self.require_server(
                        FirstPartyMcpServer::Optimization,
                        "optimization capability requires Optimization MCP and cuOpt",
                    )?;
                }
                PlatformCapability::Recording | PlatformCapability::Rrd => {
                    self.require_server(
                        FirstPartyMcpServer::Recording,
                        "recording and rrd capabilities require Recording MCP and hub",
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
        if self.mcp_servers.contains(&FirstPartyMcpServer::View) {
            required.insert("view-renderer");
        }
        if self.mcp_servers.contains(&FirstPartyMcpServer::Stream) {
            required.insert("stream");
        }
        if self.mcp_servers.contains(&FirstPartyMcpServer::Reason) {
            required.insert("reason");
        }
        if self.mcp_servers.contains(&FirstPartyMcpServer::Rerun) {
            required.insert("rerun-bridge");
        }
        if self
            .mcp_servers
            .contains(&FirstPartyMcpServer::Optimization)
        {
            required.insert("cuopt-executor");
        }
        if required.is_empty() && self.gpu_scheduling.is_none() {
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
        validate_digest(&scheduling.evidence_digest)
            .context("gpuScheduling evidenceDigest is invalid")?;
        validate_name("GPU ResourceClaim", &scheduling.allocator.claim_name)?;
        validate_kubernetes_qualified_name(
            "GPU full-device DeviceClass",
            &scheduling.allocator.full_device_class_name,
        )?;
        validate_kubernetes_qualified_name(
            "GPU MIG DeviceClass",
            &scheduling.allocator.mig_device_class_name,
        )?;
        ensure!(
            scheduling.allocator.driver_name == NVIDIA_DRA_DRIVER_NAME,
            "gpuScheduling allocator driverName must be {NVIDIA_DRA_DRIVER_NAME}"
        );
        ensure!(
            scheduling.allocator.configuration_api_version == "resource.nvidia.com/v1beta1",
            "gpuScheduling allocator configurationApiVersion must be resource.nvidia.com/v1beta1"
        );
        validate_managed_gpu_allocator(&scheduling.allocator.installation)?;
        ensure!(
            !scheduling.same_physical_device_groups.is_empty(),
            "gpuScheduling samePhysicalDeviceGroups cannot be empty"
        );
        ensure_unique(
            "GPU same-physical-device group",
            scheduling
                .same_physical_device_groups
                .iter()
                .map(|item| &item.name),
        )?;

        let mut declared = BTreeSet::new();
        let mut group_names = BTreeSet::new();
        for group in &scheduling.same_physical_device_groups {
            validate_name("GPU same-physical-device group", &group.name)?;
            group_names.insert(group.name.as_str());
            ensure!(
                !group.workloads.is_empty(),
                "GPU group {} must contain at least one workload",
                group.name
            );
            ensure!(
                group.maximum_consumers > 0,
                "GPU group {} maximumConsumers must be positive",
                group.name
            );
            let mut consumers = 0_u32;
            for placement in &group.workloads {
                validate_name("GPU workload", &placement.workload)?;
                validate_name("GPU workload Deployment", &placement.deployment)?;
                validate_name("GPU workload container", &placement.container)?;
                ensure!(
                    placement.replicas > 0,
                    "GPU workload {} replicas must be positive",
                    placement.workload
                );
                ensure!(
                    declared.insert(placement.workload.as_str()),
                    "GPU workload {} appears in more than one same-physical-device group",
                    placement.workload
                );
                consumers += u32::from(placement.replicas);
            }
            ensure!(
                consumers <= u32::from(group.maximum_consumers),
                "GPU group {} declares {consumers} workload replicas but maximumConsumers is {}",
                group.name,
                group.maximum_consumers
            );
            match group.isolation {
                GpuIsolation::Exclusive => {
                    ensure!(
                        consumers == 1 && group.maximum_consumers == 1,
                        "exclusive GPU group {} must contain exactly one consumer",
                        group.name
                    );
                    ensure!(
                        group.evidence_digest.is_none()
                            && group.mig_profile.is_none()
                            && group.time_slice_interval.is_none(),
                        "exclusive GPU group {} cannot declare sharing or MIG parameters",
                        group.name
                    );
                }
                GpuIsolation::MeasuredTimeSlicing => {
                    ensure!(
                        group.maximum_consumers >= 2,
                        "time-sliced GPU group {} must allow at least two consumers",
                        group.name
                    );
                    let digest = group.evidence_digest.as_deref().with_context(|| {
                        format!(
                            "time-sliced GPU group {} requires measured evidenceDigest",
                            group.name
                        )
                    })?;
                    validate_digest(digest)?;
                    ensure!(
                        group.time_slice_interval.is_some() && group.mig_profile.is_none(),
                        "time-sliced GPU group {} requires timeSliceInterval and cannot declare migProfile",
                        group.name
                    );
                }
                GpuIsolation::Mig => {
                    ensure!(
                        group
                            .mig_profile
                            .as_deref()
                            .is_some_and(|value| !value.is_empty())
                            && group.time_slice_interval.is_none(),
                        "MIG GPU group {} requires migProfile and cannot declare timeSliceInterval",
                        group.name
                    );
                    let profile = group.mig_profile.as_deref().expect("checked above");
                    ensure!(
                        profile.len() <= 32
                            && profile.bytes().all(|byte| {
                                byte.is_ascii_lowercase()
                                    || byte.is_ascii_digit()
                                    || matches!(byte, b'.' | b'-' | b'+')
                            }),
                        "MIG GPU group {} has invalid migProfile {profile}",
                        group.name
                    );
                    if let Some(digest) = &group.evidence_digest {
                        validate_digest(digest)?;
                    }
                }
            }
        }
        for workload in &required {
            ensure!(
                declared.contains(*workload),
                "gpuScheduling is missing selected workload {workload}"
            );
        }
        for workload in &declared {
            ensure!(
                required.contains(*workload) || self.external_workloads.contains(*workload),
                "gpuScheduling declares unselected workload {workload}"
            );
        }
        ensure!(
            scheduling.same_physical_device_groups.len()
                <= usize::from(scheduling.allocatable_devices),
            "GPU topology declares {} physical-device groups but gpuScheduling exposes {} devices",
            scheduling.same_physical_device_groups.len(),
            scheduling.allocatable_devices
        );
        for constraint in &scheduling.different_physical_device_groups {
            ensure!(
                constraint.groups.len() >= 2,
                "differentPhysicalDeviceGroups entries must name at least two groups"
            );
            for group in &constraint.groups {
                ensure!(
                    group_names.contains(group.as_str()),
                    "different-physical-device constraint references unknown group {group}"
                );
            }
            let mig_modes = scheduling
                .same_physical_device_groups
                .iter()
                .filter(|group| constraint.groups.contains(&group.name))
                .map(|group| group.isolation == GpuIsolation::Mig)
                .collect::<BTreeSet<_>>();
            ensure!(
                mig_modes.len() == 1,
                "one different-physical-device constraint cannot mix MIG and full-device groups"
            );
            ensure!(
                constraint.groups.len() <= usize::from(scheduling.allocatable_devices),
                "different-physical-device constraint needs {} devices but gpuScheduling exposes {}",
                constraint.groups.len(),
                scheduling.allocatable_devices
            );
        }
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
}

impl PlatformComponent {
    fn images(self) -> &'static [&'static str] {
        match self {
            Self::Gateway => &["mcp-gateway"],
            Self::ArtifactService => &["artifact-service"],
            Self::RecordingDataPlane => &["recording-hub", "recording-forwarder"],
            Self::SimulationRuntimeSupport => &["simulation-runtime"],
            Self::AgentRuntimeSupport => &["agent-kernel"],
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
        validate_revision(&self.profile_revision)?;
        self.registry.validate()?;
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

impl DevelopmentImageLock {
    /// Validates a development-only runnable image closure and its evidence lineage.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == DEVELOPMENT_IMAGE_LOCK_SCHEMA,
            "development image lock schemaVersion must be {DEVELOPMENT_IMAGE_LOCK_SCHEMA}"
        );
        ensure!(
            !self.release_eligible,
            "development image lock must declare releaseEligible=false"
        );
        validate_digest(&self.base_deployment_lock_digest)?;
        self.registry.validate()?;
        ensure!(
            !self.images.is_empty(),
            "development image lock must contain at least one image"
        );
        ensure_unique(
            "development image repository",
            self.images.iter().map(|image| &image.repository),
        )?;
        let mut identities = BTreeSet::new();
        for image in &self.images {
            validate_name("development image source", &image.source)?;
            validate_name("development image target", &image.target)?;
            ensure!(
                identities.insert((image.source.clone(), image.target.clone())),
                "development image {}:{} is duplicated",
                image.source,
                image.target
            );
            ensure!(
                image
                    .repository
                    .starts_with(&format!("{}/", self.registry.pull_address)),
                "development image repository {} is outside registry {}",
                image.repository,
                self.registry.pull_address
            );
            ensure!(
                !image.repository.contains('@') && !image.repository.ends_with(":latest"),
                "development image repository must not carry a mutable tag or digest"
            );
            validate_revision(&image.source_revision)?;
            validate_digest(&image.runtime_digest)?;
            match &image.origin {
                DevelopmentImageOrigin::Qualified { publication_digest } => {
                    validate_digest(publication_digest)?;
                    ensure!(
                        publication_digest != &image.runtime_digest,
                        "qualified development image {} must retain a distinct attested publication digest",
                        image.target
                    );
                }
                DevelopmentImageOrigin::Staged {
                    staging_index_digest,
                    stage_evidence_digest,
                } => {
                    validate_digest(staging_index_digest)?;
                    validate_digest(stage_evidence_digest)?;
                }
            }
        }
        Ok(())
    }
}

impl LocalRegistrySpec {
    /// Returns the registry address reachable by host publication tools.
    pub fn push_address(&self) -> Result<String> {
        let parsed = Url::parse(&format!("http://{}", self.host_port))
            .context("local registry hostPort must be HOST:PORT")?;
        ensure!(
            parsed.host_str().is_some()
                && parsed.port().is_some()
                && parsed.path() == "/"
                && parsed.query().is_none()
                && parsed.fragment().is_none(),
            "local registry hostPort must be HOST:PORT"
        );
        Ok(self.host_port.clone())
    }

    /// Returns the registry address visible from k3d nodes.
    pub fn pull_address(&self) -> Result<String> {
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
    let _ = registry.push_address()?;
    let _ = registry.pull_address()?;
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

/// Generates the canonical non-release development image lock schema.
#[must_use]
pub fn development_image_lock_schema() -> schemars::Schema {
    schemars::schema_for!(DevelopmentImageLock)
}

fn validate_releases(
    source: &DeploymentSource,
    root: &Path,
    release_names: &mut BTreeSet<String>,
) -> Result<()> {
    validate_release_metadata(source, release_names)?;
    for release in &source.releases {
        require_directory(&root.join(&release.chart), "Helm chart")?;
        for values in &release.source_values {
            require_file(&root.join(values), "source-owned Helm values")?;
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
                release.values_contract == ReleaseValuesContract::Platform,
                "platform source {} must use the platform values contract for Helm release {}",
                source.name,
                release.name
            ),
            DeploymentSourceRole::Extension => ensure!(
                release.values_contract == ReleaseValuesContract::Extension,
                "extension source {} must use the extension values contract for Helm release {}",
                source.name,
                release.name
            ),
            DeploymentSourceRole::Workload => ensure!(
                release.values_contract == ReleaseValuesContract::VeoveoSource,
                "workload source {} must use the Veoveo source values contract for Helm release {}",
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
        for values in &release.source_values {
            ensure!(
                !values.as_os_str().is_empty() && !values.is_absolute(),
                "source-owned Helm values path for {} must be source-relative",
                release.name
            );
        }
        for values in &release.installation_values {
            ensure!(
                !values.as_os_str().is_empty() && !values.is_absolute(),
                "installation-owned Helm values path for {} must be profile-relative",
                release.name
            );
        }
    }
    Ok(())
}

fn validate_managed_gpu_allocator(installation: &ManagedGpuAllocatorInstallation) -> Result<()> {
    validate_name("GPU allocator Helm release", &installation.release_name)?;
    validate_name("GPU allocator namespace", &installation.namespace)?;
    ensure!(
        installation.chart.coordinate == NVIDIA_DRA_CHART_COORDINATE,
        "GPU allocator chart coordinate must be {NVIDIA_DRA_CHART_COORDINATE}"
    );
    ensure!(
        installation.chart.version == NVIDIA_DRA_VERSION,
        "GPU allocator chart version must be {NVIDIA_DRA_VERSION}"
    );
    ensure!(
        installation.chart.digest == NVIDIA_DRA_CHART_DIGEST,
        "GPU allocator chart digest must be {NVIDIA_DRA_CHART_DIGEST}"
    );
    ensure!(
        installation.chart.content_digest == NVIDIA_DRA_CHART_CONTENT_DIGEST,
        "GPU allocator chart contentDigest must be {NVIDIA_DRA_CHART_CONTENT_DIGEST}"
    );
    validate_digest(&installation.chart.digest)?;
    validate_digest(&installation.chart.content_digest)?;
    ensure!(
        installation.image.repository == NVIDIA_DRA_IMAGE_REPOSITORY,
        "GPU allocator image repository must be {NVIDIA_DRA_IMAGE_REPOSITORY}"
    );
    ensure!(
        installation.image.tag == format!("v{NVIDIA_DRA_VERSION}"),
        "GPU allocator image tag must be v{NVIDIA_DRA_VERSION}"
    );
    ensure!(
        installation.image.digest == NVIDIA_DRA_IMAGE_DIGEST,
        "GPU allocator image digest must be {NVIDIA_DRA_IMAGE_DIGEST}"
    );
    validate_digest(&installation.image.digest)?;
    let expected_platforms = BTreeMap::from([
        (
            "linux/amd64".to_owned(),
            NVIDIA_DRA_IMAGE_AMD64_DIGEST.to_owned(),
        ),
        (
            "linux/arm64".to_owned(),
            NVIDIA_DRA_IMAGE_ARM64_DIGEST.to_owned(),
        ),
    ]);
    ensure!(
        installation.image.platform_digests == expected_platforms,
        "GPU allocator image platformDigests must match the qualified v{NVIDIA_DRA_VERSION} image index"
    );
    for digest in installation.image.platform_digests.values() {
        validate_digest(digest)?;
    }
    ensure!(
        installation.nvidia_driver_root == "/",
        "managed standalone GPU allocator nvidiaDriverRoot must be / for a host-installed driver"
    );
    ensure!(
        !installation.eligible_node_selector.is_empty(),
        "GPU allocator eligibleNodeSelector cannot be empty"
    );
    for (key, value) in &installation.eligible_node_selector {
        validate_label_selector(key, value)?;
    }
    match &installation.conflicting_device_plugin_removal {
        ConflictingGpuDevicePluginRemoval::RequireAbsent => {}
        ConflictingGpuDevicePluginRemoval::DeleteDaemonSet { namespace, name } => {
            validate_name("conflicting device-plugin namespace", namespace)?;
            validate_name("conflicting device-plugin DaemonSet", name)?;
        }
        ConflictingGpuDevicePluginRemoval::UninstallHelmRelease {
            namespace,
            release_name,
            expected_chart_version,
        } => {
            validate_name("conflicting device-plugin namespace", namespace)?;
            validate_name("conflicting device-plugin Helm release", release_name)?;
            ensure!(
                !expected_chart_version.trim().is_empty()
                    && !expected_chart_version.chars().any(char::is_whitespace),
                "conflicting device-plugin expectedChartVersion cannot be empty or contain whitespace"
            );
        }
    }
    ensure!(
        (60..=1_800).contains(&installation.timeout_seconds),
        "GPU allocator timeoutSeconds must be between 60 and 1800"
    );
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
    let url = Url::parse(&format!("https://{address}"))
        .with_context(|| format!("invalid registry address {address}"))?;
    ensure!(
        url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "registry address must contain only a host and optional port"
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

fn validate_kubernetes_qualified_name(kind: &str, name: &str) -> Result<()> {
    ensure!(!name.is_empty(), "{kind} name cannot be empty");
    ensure!(name.len() <= 253, "{kind} name exceeds 253 characters");
    ensure!(
        name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        }) && name.as_bytes()[0].is_ascii_alphanumeric()
            && name.as_bytes()[name.len() - 1].is_ascii_alphanumeric(),
        "{kind} name {name} must be a lowercase Kubernetes qualified name"
    );
    Ok(())
}

fn validate_label_selector(key: &str, value: &str) -> Result<()> {
    let (prefix, name) = key
        .split_once('/')
        .with_context(|| format!("GPU allocator node selector key {key} must be qualified"))?;
    validate_kubernetes_qualified_name("GPU allocator node selector prefix", prefix)?;
    validate_kubernetes_qualified_name("GPU allocator node selector", name)?;
    ensure!(
        value.len() <= 63
            && (value.is_empty()
                || (value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                }) && value.as_bytes()[0].is_ascii_alphanumeric()
                    && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric())),
        "GPU allocator node selector value {value} is not a Kubernetes label value"
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
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::Path,
    };

    use jsonschema::Validator;

    use super::{
        ConflictingGpuDevicePluginRemoval, DeploymentLock, DeploymentSourceRole,
        FirstPartyMcpServer, GatewayDeploymentRequirements, GpuAllocatorMaturityAcceptance,
        GpuDifferentPhysicalDeviceConstraint, GpuDynamicResourceAllocator, GpuIsolation,
        GpuSamePhysicalDeviceGroup, GpuSchedulingProfile, GpuTimeSliceInterval,
        GpuWorkloadPlacement, InstallationPreset, LoadedProfile, ManagedGpuAllocatorInstallation,
        ManagedOciChart, ManagedOciImage, NVIDIA_DRA_CHART_CONTENT_DIGEST,
        NVIDIA_DRA_CHART_COORDINATE, NVIDIA_DRA_CHART_DIGEST,
        NVIDIA_DRA_CONTAINER_TOOLKIT_PACKAGE_VERSION, NVIDIA_DRA_HELM_VERSION,
        NVIDIA_DRA_IMAGE_AMD64_DIGEST, NVIDIA_DRA_IMAGE_ARM64_DIGEST, NVIDIA_DRA_IMAGE_DIGEST,
        NVIDIA_DRA_IMAGE_REPOSITORY, NVIDIA_DRA_KUBERNETES_VERSION, NVIDIA_DRA_VERSION,
        PlannedImage, PlatformCapability, PlatformComponent, PlatformSelection,
        deployment_lock_schema, deployment_profile_schema, development_image_lock_schema,
        validate_managed_gpu_allocator,
    };

    fn managed_gpu_allocator_installation() -> ManagedGpuAllocatorInstallation {
        ManagedGpuAllocatorInstallation {
            release_name: "dra-driver-nvidia-gpu".to_owned(),
            namespace: "nvidia-dra-driver-gpu".to_owned(),
            chart: ManagedOciChart {
                coordinate: NVIDIA_DRA_CHART_COORDINATE.to_owned(),
                version: NVIDIA_DRA_VERSION.to_owned(),
                digest: NVIDIA_DRA_CHART_DIGEST.to_owned(),
                content_digest: NVIDIA_DRA_CHART_CONTENT_DIGEST.to_owned(),
            },
            image: ManagedOciImage {
                repository: NVIDIA_DRA_IMAGE_REPOSITORY.to_owned(),
                tag: format!("v{NVIDIA_DRA_VERSION}"),
                digest: NVIDIA_DRA_IMAGE_DIGEST.to_owned(),
                platform_digests: BTreeMap::from([
                    (
                        "linux/amd64".to_owned(),
                        NVIDIA_DRA_IMAGE_AMD64_DIGEST.to_owned(),
                    ),
                    (
                        "linux/arm64".to_owned(),
                        NVIDIA_DRA_IMAGE_ARM64_DIGEST.to_owned(),
                    ),
                ]),
            },
            nvidia_driver_root: "/".to_owned(),
            eligible_node_selector: BTreeMap::from([(
                "kubernetes.io/hostname".to_owned(),
                "gpu-node".to_owned(),
            )]),
            conflicting_device_plugin_removal: ConflictingGpuDevicePluginRemoval::RequireAbsent,
            maturity_acceptance: GpuAllocatorMaturityAcceptance::TechnologyPreview,
            timeout_seconds: 300,
        }
    }

    #[test]
    fn managed_gpu_allocator_rejects_mutable_or_stale_artifacts() {
        let qualified = managed_gpu_allocator_installation();
        validate_managed_gpu_allocator(&qualified).expect("qualified allocator closure");

        let mut stale_chart = qualified.clone();
        stale_chart.chart.version = "0.4.0".to_owned();
        assert!(
            validate_managed_gpu_allocator(&stale_chart)
                .expect_err("stale chart version must fail")
                .to_string()
                .contains("chart version")
        );

        let mut mutable_image = qualified;
        mutable_image.image.digest =
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned();
        assert!(
            validate_managed_gpu_allocator(&mutable_image)
                .expect_err("unqualified image digest must fail")
                .to_string()
                .contains("image digest")
        );
    }

    #[test]
    fn managed_gpu_allocator_uses_repository_runtime_pins() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let versions = fs::read_to_string(repository.join("deploy/local/k3d/versions.env"))
            .expect("read local runtime versions");

        assert!(versions.contains(&format!(
            "K3S_VERSION=v{NVIDIA_DRA_KUBERNETES_VERSION}-k3s1"
        )));
        assert!(versions.contains(&format!("HELM_VERSION=v{NVIDIA_DRA_HELM_VERSION}")));
        assert!(versions.contains(&format!(
            "NVIDIA_CONTAINER_TOOLKIT_VERSION={NVIDIA_DRA_CONTAINER_TOOLKIT_PACKAGE_VERSION}"
        )));
    }

    fn exclusive_gpu_scheduling(
        workloads: impl IntoIterator<Item = &'static str>,
        allocatable_devices: u16,
    ) -> GpuSchedulingProfile {
        GpuSchedulingProfile {
            runtime_class_name: "nvidia".to_owned(),
            allocatable_devices,
            evidence_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
            allocator: GpuDynamicResourceAllocator {
                claim_name: "veoveo-gpu-placement".to_owned(),
                full_device_class_name: "gpu.nvidia.com".to_owned(),
                mig_device_class_name: "mig.nvidia.com".to_owned(),
                driver_name: "gpu.nvidia.com".to_owned(),
                configuration_api_version: "resource.nvidia.com/v1beta1".to_owned(),
                installation: managed_gpu_allocator_installation(),
            },
            same_physical_device_groups: workloads
                .into_iter()
                .map(|workload| GpuSamePhysicalDeviceGroup {
                    name: workload.to_owned(),
                    workloads: vec![GpuWorkloadPlacement {
                        workload: workload.to_owned(),
                        deployment: match workload {
                            "view-renderer" => "view-mcp",
                            "cuopt-executor" => "optimization-mcp",
                            value => value,
                        }
                        .to_owned(),
                        container: match workload {
                            "view-renderer" => "view-mcp",
                            value => value,
                        }
                        .to_owned(),
                        replicas: 1,
                    }],
                    isolation: GpuIsolation::Exclusive,
                    maximum_consumers: 1,
                    evidence_digest: None,
                    mig_profile: None,
                    time_slice_interval: None,
                })
                .collect(),
            different_physical_device_groups: Vec::new(),
        }
    }

    #[test]
    fn loads_repository_profile() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile = repository.join("showcase/sumo/deploy/deployment.json");
        let loaded = LoadedProfile::load(&profile, &repository).expect("load SUMO profile");
        assert_eq!(loaded.definition.name, "sumo");
        assert_eq!(loaded.definition.sources.len(), 2);
        assert!(loaded.definition.sources[0].image_groups.is_empty());
        assert_eq!(loaded.definition.sources[1].image_groups, ["showcase-sumo"]);
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

        images.pop();
        images.push(PlannedImage {
            source: "veoveo".to_owned(),
            target: "unnecessary-platform-image".to_owned(),
            reference: "registry.example.internal/veoveo/unnecessary-platform-image:revision"
                .to_owned(),
        });
        assert!(
            loaded
                .validate_image_plan(&images)
                .expect_err("unnecessary platform image must fail")
                .to_string()
                .contains("unnecessary Veoveo image targets")
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
    fn agent_runtime_support_selects_the_external_agent_kernel_image() {
        let selection = PlatformSelection {
            installation_preset: InstallationPreset::Custom,
            components: BTreeSet::from([
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::AgentRuntimeSupport,
            ]),
            mcp_servers: BTreeSet::new(),
            artifact_audiences: BTreeSet::new(),
            external_workloads: BTreeSet::new(),
            gpu_scheduling: None,
        }
        .resolve()
        .expect("valid external agent runtime selection");
        assert_eq!(
            selection.required_images(),
            BTreeSet::from(["agent-kernel".to_owned(), "mcp-gateway".to_owned()])
        );
    }

    #[test]
    fn optimization_image_closure_includes_gpu_executor() {
        let requirements =
            serde_json::from_value::<GatewayDeploymentRequirements>(serde_json::json!({
                "platformCapabilities": ["optimization"],
                "artifactAudiences": []
            }))
            .expect("decode portable Optimization requirement");
        assert_eq!(
            requirements.platform_capabilities,
            BTreeSet::from([PlatformCapability::Optimization])
        );
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
            gpu_scheduling: Some(exclusive_gpu_scheduling(["cuopt-executor"], 1)),
        }
        .resolve()
        .expect("valid Optimization selection");
        selection
            .satisfy(&requirements)
            .expect("Optimization requirement must be formally satisfiable");
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
    fn two_physical_groups_fail_on_one_device() {
        let error = PlatformSelection {
            installation_preset: InstallationPreset::Custom,
            components: BTreeSet::from([
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::ObjectStore,
                PlatformComponent::SimulationRuntimeSupport,
            ]),
            mcp_servers: BTreeSet::new(),
            artifact_audiences: BTreeSet::new(),
            external_workloads: BTreeSet::from([
                "external-simulator".to_owned(),
                "external-view".to_owned(),
            ]),
            gpu_scheduling: Some(exclusive_gpu_scheduling(
                ["external-simulator", "external-view"],
                1,
            )),
        }
        .resolve()
        .expect_err("two physical groups cannot fit one GPU");
        assert!(error.to_string().contains("physical-device groups"));
    }

    #[test]
    fn shared_gpu_pairing_resolves_two_distinct_physical_groups() {
        let workload = |workload: &str, deployment: &str, container: &str| GpuWorkloadPlacement {
            workload: workload.to_owned(),
            deployment: deployment.to_owned(),
            container: container.to_owned(),
            replicas: 1,
        };
        let group = |name: &str, workloads: Vec<GpuWorkloadPlacement>, digit: char| {
            GpuSamePhysicalDeviceGroup {
                name: name.to_owned(),
                workloads,
                isolation: GpuIsolation::MeasuredTimeSlicing,
                maximum_consumers: 2,
                evidence_digest: Some(format!("sha256:{}", digit.to_string().repeat(64))),
                mig_profile: None,
                time_slice_interval: Some(GpuTimeSliceInterval::Default),
            }
        };
        let scheduling = GpuSchedulingProfile {
            runtime_class_name: "nvidia".to_owned(),
            allocatable_devices: 2,
            evidence_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
            allocator: GpuDynamicResourceAllocator {
                claim_name: "paired-gpus".to_owned(),
                full_device_class_name: "gpu.nvidia.com".to_owned(),
                mig_device_class_name: "mig.nvidia.com".to_owned(),
                driver_name: "gpu.nvidia.com".to_owned(),
                configuration_api_version: "resource.nvidia.com/v1beta1".to_owned(),
                installation: managed_gpu_allocator_installation(),
            },
            same_physical_device_groups: vec![
                group(
                    "simulation",
                    vec![
                        workload("external-simulator", "external-simulator", "simulator"),
                        workload("external-live-view", "external-simulator", "simulator"),
                    ],
                    '2',
                ),
                group(
                    "planning",
                    vec![
                        workload("external-optimizer", "external-optimizer", "optimizer"),
                        workload("external-view", "external-view", "view"),
                    ],
                    '3',
                ),
            ],
            different_physical_device_groups: vec![GpuDifferentPhysicalDeviceConstraint {
                groups: BTreeSet::from(["planning".to_owned(), "simulation".to_owned()]),
            }],
        };
        let selection = PlatformSelection {
            installation_preset: InstallationPreset::Custom,
            components: BTreeSet::from([
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::ObjectStore,
                PlatformComponent::SimulationRuntimeSupport,
            ]),
            mcp_servers: BTreeSet::new(),
            artifact_audiences: BTreeSet::new(),
            external_workloads: BTreeSet::from([
                "external-live-view".to_owned(),
                "external-optimizer".to_owned(),
                "external-simulator".to_owned(),
                "external-view".to_owned(),
            ]),
            gpu_scheduling: Some(scheduling),
        }
        .resolve()
        .expect("two paired groups fit two physical devices");

        let scheduling = selection.gpu_scheduling.unwrap();
        assert_eq!(scheduling.same_physical_device_groups.len(), 2);
        assert_eq!(scheduling.different_physical_device_groups.len(), 1);
    }

    #[test]
    fn generated_schemas_are_closed_and_compile() {
        for schema in [
            deployment_profile_schema(),
            deployment_lock_schema(),
            development_image_lock_schema(),
        ] {
            let value = serde_json::to_value(schema).expect("serialize schema");
            Validator::new(&value).expect("compile schema");
        }
    }
}
