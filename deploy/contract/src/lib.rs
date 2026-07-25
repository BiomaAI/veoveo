//! Typed repository deployment profiles and local registry declarations.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

/// Canonical repository development deployment profile.
pub const PROFILE_SCHEMA: &str = "veoveo.io/deployment/v1";
/// Canonical local OCI registry declaration.
pub const REGISTRY_SCHEMA: &str = "veoveo.io/local-registry/v1";

/// A validated repository development deployment profile.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentProfile {
    /// Profile schema identifier.
    pub schema_version: String,
    /// Stable profile name.
    pub name: String,
    /// Image publication destination.
    pub registry: RegistryReference,
    /// Ordered Docker Bake phases.
    pub image_groups: Vec<String>,
    /// Kubernetes destination.
    pub kubernetes: KubernetesTarget,
    /// Installation namespace.
    pub namespace: String,
    /// Resources applied before Helm.
    #[serde(default)]
    pub resources: ResourceSet,
    /// Ordered Helm releases.
    pub releases: Vec<ReleaseSpec>,
    /// Additional deployments awaited after Helm.
    #[serde(default)]
    pub wait_for_deployments: Vec<String>,
}

/// An OCI registry selected by a deployment profile.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryReference {
    /// Registry host and optional port, without a URL scheme.
    pub address: String,
    /// Optional repository-local lifecycle declaration.
    pub local_config: Option<PathBuf>,
}

/// Repository-local OCI registry lifecycle settings.
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KubernetesTarget {
    /// Explicit kubeconfig context.
    pub context: String,
    /// Optional repository-managed k3d cluster.
    pub local_cluster: Option<LocalClusterSpec>,
}

/// Repository-managed local k3d cluster.
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Default, Deserialize)]
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
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigMapSpec {
    /// Kubernetes object name.
    pub name: String,
    /// Data key to source path.
    pub files: BTreeMap<String, PathBuf>,
}

/// An environment-backed development Secret.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretSpec {
    /// Kubernetes object name.
    pub name: String,
    /// Secret data entries.
    pub data_from_env: Vec<SecretEnvironmentEntry>,
}

/// One Secret data entry loaded from an environment variable.
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretFormat {
    /// Uninterpreted Secret text.
    #[default]
    Opaque,
    /// Canonical gateway internal trust JWKS.
    GatewayInternalTrustJwks,
}

/// A Helm release selected by a deployment profile.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseSpec {
    /// Helm release name.
    pub name: String,
    /// Chart root.
    pub chart: PathBuf,
    /// Ordered values files.
    #[serde(default)]
    pub values: Vec<PathBuf>,
    /// Whether Helm creates the namespace.
    #[serde(default)]
    pub create_namespace: bool,
    /// Helm operation timeout.
    pub timeout_seconds: u64,
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::LoadedProfile;

    #[test]
    fn loads_repository_profile() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile = repository.join("showcase/sumo/deploy/deployment.json");
        let loaded = LoadedProfile::load(&profile, &repository).expect("load SUMO profile");
        assert_eq!(loaded.definition.name, "sumo");
        assert_eq!(loaded.definition.image_groups.len(), 3);
    }
}
