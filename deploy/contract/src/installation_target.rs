//! Installation-owned inputs for smoke checks against a running release.

use std::{
    collections::BTreeSet,
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::{Host, Url};

/// Supported installed-smoke input format.
pub const INSTALLATION_TARGET_SCHEMA: &str = "veoveo.ai/installation-target/v1";

/// JSON Schema for installation-owned smoke inputs. Runtime validation also checks
/// cross-field admission, duplicate entries, and loopback routing.
pub fn installation_target_schema() -> schemars::Schema {
    schemars::schema_for!(InstallationTarget)
}

/// An installed release and the identity used to exercise its public surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationTarget {
    /// Format identifier; older or unknown formats are rejected.
    pub schema: String,
    /// Explicit cluster context and release namespace.
    pub kubernetes: InstallationKubernetesTarget,
    /// Loopback HTTP origin forwarded to the installation ingress.
    #[schemars(with = "String", url)]
    pub local_base_url: Url,
    /// Public HTTPS origin.
    #[schemars(with = "String", url)]
    pub public_base_url: Url,
    /// Public control-plane document, relative to this target file.
    pub control_plane: PathBuf,
    /// Deployments which must have available replicas in the release namespace.
    pub expected_deployments: Vec<String>,
    /// Required allocatable NVIDIA GPU resources across the cluster.
    pub minimum_gpu_shares: u32,
    /// Installation-selected OAuth client, resource profile and scopes.
    pub operator: InstallationClient,
    /// Optional administrator for scenarios that create grants or inspect ownership.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub administrator: Option<InstallationClient>,
    /// Native Catalog SDK routing, required only by its installed scenario.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_catalog: Option<InstallationRecordingCatalog>,
    /// In-cluster Python consumer and its explicitly selected fixture signer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_consumer: Option<InstallationArtifactConsumer>,
}

/// Private-plane coordinates used only by artifact SDK acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationArtifactConsumer {
    /// Secret in the target namespace with internal-signing-key-id and
    /// internal-signing-key-der-b64 keys. The harness never exports its contents.
    pub internal_signing_secret: String,
    /// Deployment whose Python runtime contains the installed Veoveo SDK.
    pub python_deployment: String,
    /// Artifact service origin reachable from that deployment.
    #[schemars(with = "String", url)]
    pub artifact_service_url: Url,
    /// Explicit Artifact service process selected only by recovery acceptance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_recovery: Option<InstallationArtifactServiceRecovery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationArtifactServiceRecovery {
    pub deployment: String,
    pub pod: String,
    pub container: String,
}

/// Kubernetes target without ambient context or namespace defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationKubernetesTarget {
    /// Context in the operator's kubeconfig.
    pub context: String,
    /// Namespace of the release.
    pub namespace: String,
}

/// Machine identity and contexts admitted by the installation's control plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationClient {
    /// OAuth client registration; its private key stays in the caller's environment.
    pub client_id: String,
    /// Gateway profile to exercise.
    pub profile: String,
    /// Complete set of requested scopes; the scenario never silently adds scopes.
    pub scopes: Vec<String>,
    /// Work Context used for fixture publication and reads.
    pub work_context: String,
    /// A distinct admitted Work Context used to verify isolation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison_context: Option<String>,
}

/// Connection coordinates for a native Rerun Catalog SDK client container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationRecordingCatalog {
    /// Rerun endpoint using rerun+https, or loopback-routed rerun+http.
    #[schemars(with = "String", url)]
    pub redap_url: Url,
    /// Container-only hostname overrides; no host DNS configuration is changed.
    pub host_mappings: Vec<InstallationHostMapping>,
}

/// A single hostname override for an isolated SDK client container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationHostMapping {
    /// Hostname in the Rerun endpoint.
    pub hostname: String,
    /// Address reachable by the SDK client's host network.
    pub address: IpAddr,
}

impl InstallationTarget {
    /// Decode and validate without consulting the ambient cluster or credentials.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let target: Self = serde_json::from_slice(bytes).context("decoding installation target")?;
        target.validate()?;
        Ok(target)
    }

    /// Load an installation target; relative control-plane paths resolve beside it.
    pub fn load(path: &Path) -> Result<Self> {
        Self::decode(&fs::read(path).with_context(|| format!("reading {}", path.display()))?)
    }

    /// Resolve the public control-plane file relative to the target document.
    pub fn control_plane_path(&self, target_path: &Path) -> PathBuf {
        target_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(&self.control_plane)
    }

    /// Validate origins and structured coordinates before any external action.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == INSTALLATION_TARGET_SCHEMA,
            "installation target schema must be {INSTALLATION_TARGET_SCHEMA}; regenerate the target"
        );
        ensure!(
            !self.kubernetes.context.trim().is_empty()
                && !self.kubernetes.context.chars().any(char::is_control),
            "kubernetes.context must be nonempty without control characters"
        );
        ensure!(
            dns_label(&self.kubernetes.namespace),
            "kubernetes.namespace must be a DNS label"
        );
        validate_origin(&self.local_base_url, "http", "localBaseUrl")?;
        ensure!(
            loopback(&self.local_base_url),
            "localBaseUrl must use a loopback host"
        );
        validate_origin(&self.public_base_url, "https", "publicBaseUrl")?;
        ensure!(
            !self.control_plane.as_os_str().is_empty() && self.control_plane.is_relative(),
            "controlPlane must be a nonempty path relative to the installation target"
        );
        ensure!(
            !self.expected_deployments.is_empty(),
            "expectedDeployments must not be empty"
        );
        let mut deployments = BTreeSet::new();
        for name in &self.expected_deployments {
            ensure!(
                dns_name(name) && deployments.insert(name),
                "expectedDeployments contains an invalid or repeated name"
            );
        }
        self.operator.validate("operator")?;
        if let Some(administrator) = &self.administrator {
            administrator.validate("administrator")?;
        }
        if let Some(catalog) = &self.recording_catalog {
            catalog.validate()?;
        }
        if let Some(consumer) = &self.artifact_consumer {
            ensure!(
                dns_name(&consumer.internal_signing_secret)
                    && dns_name(&consumer.python_deployment),
                "artifactConsumer Secret and deployment names must be DNS names"
            );
            ensure!(
                matches!(consumer.artifact_service_url.scheme(), "http" | "https"),
                "artifactConsumer.artifactServiceUrl must use HTTP or HTTPS"
            );
            validate_origin(
                &consumer.artifact_service_url,
                consumer.artifact_service_url.scheme(),
                "artifactConsumer.artifactServiceUrl",
            )?;
            if let Some(recovery) = &consumer.service_recovery {
                ensure!(
                    dns_name(&recovery.deployment)
                        && dns_name(&recovery.pod)
                        && dns_name(&recovery.container),
                    "artifactConsumer.serviceRecovery workload names must be DNS names"
                );
                ensure!(
                    self.expected_deployments.contains(&recovery.deployment),
                    "artifactConsumer.serviceRecovery deployment must be declared in expectedDeployments"
                );
                ensure!(
                    recovery.container == "artifact-service",
                    "artifactConsumer.serviceRecovery.container must select the artifact-service process, not a sidecar"
                );
                ensure!(
                    recovery.deployment != consumer.python_deployment,
                    "Artifact service recovery must not replace the Python consumer"
                );
            }
        }
        Ok(())
    }
}

impl InstallationClient {
    fn validate(&self, role: &str) -> Result<()> {
        for (field, value) in [
            ("clientId", &self.client_id),
            ("profile", &self.profile),
            ("workContext", &self.work_context),
        ] {
            ensure!(
                identifier(value),
                "{role}.{field} must be a nonempty identifier"
            );
        }
        ensure!(!self.scopes.is_empty(), "{role}.scopes must not be empty");
        let mut scopes = BTreeSet::new();
        for scope in &self.scopes {
            ensure!(
                oauth_scope(scope) && scopes.insert(scope),
                "{role}.scopes contains an invalid or repeated scope"
            );
        }
        if let Some(context) = &self.comparison_context {
            ensure!(
                identifier(context) && context != &self.work_context,
                "{role}.comparisonContext must be a distinct nonempty identifier"
            );
        }
        Ok(())
    }
}

impl InstallationRecordingCatalog {
    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.redap_url.scheme(), "rerun+http" | "rerun+https"),
            "recordingCatalog.redapUrl must use rerun+http or rerun+https"
        );
        validate_origin(
            &self.redap_url,
            self.redap_url.scheme(),
            "recordingCatalog.redapUrl",
        )?;
        let mut hosts = BTreeSet::new();
        for mapping in &self.host_mappings {
            ensure!(
                dns_name(&mapping.hostname) && hosts.insert(&mapping.hostname),
                "recordingCatalog.hostMappings contains an invalid or repeated hostname"
            );
        }
        if self.redap_url.scheme() == "rerun+http" {
            let routed_to_loopback = self.host_mappings.iter().any(|mapping| {
                Some(mapping.hostname.as_str()) == self.redap_url.host_str()
                    && mapping.address.is_loopback()
            });
            ensure!(
                loopback(&self.redap_url) || routed_to_loopback,
                "unencrypted recordingCatalog.redapUrl must route to loopback"
            );
        }
        Ok(())
    }
}

fn validate_origin(url: &Url, scheme: &str, field: &str) -> Result<()> {
    ensure!(
        url.scheme() == scheme
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && matches!(url.path(), "" | "/")
            && url.query().is_none()
            && url.fragment().is_none(),
        "{field} must be a {scheme} origin without credentials, path, query, or fragment"
    );
    Ok(())
}

fn loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        Some(Host::Domain(host)) => {
            host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
        }
        None => false,
    }
}

fn dns_label(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name.as_bytes()[name.len() - 1].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn dns_name(name: &str) -> bool {
    name.len() <= 253 && name.split('.').all(dns_label)
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b':'))
}

// RFC 6749 scope-token: printable ASCII except space, quotation mark and backslash.
fn oauth_scope(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|c| c == 0x21 || (0x23..=0x5b).contains(&c) || (0x5d..=0x7e).contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn target() -> serde_json::Value {
        json!({
            "schema": INSTALLATION_TARGET_SCHEMA,
            "kubernetes": {"context":"k3d-fork", "namespace":"fork"},
            "localBaseUrl":"http://127.0.0.1:18781", "publicBaseUrl":"https://fork.example.test",
            "controlPlane":"gateway.json", "expectedDeployments":["mcp-gateway"], "minimumGpuShares":1,
            "operator":{"clientId":"review-client", "profile":"review", "workContext":"inspection",
                "comparisonContext":"other-inspection", "scopes":["review:use"]},
            "recordingCatalog":{"redapUrl":"rerun+http://fork.example.test:18781",
                "hostMappings":[{"hostname":"fork.example.test", "address":"127.0.0.1"}]},
            "artifactConsumer":{"internalSigningSecret":"fork-fixture-signer",
                "pythonDeployment":"python-consumer", "artifactServiceUrl":"http://artifact-plane:8790"}
        })
    }

    fn decode(value: &serde_json::Value) -> Result<InstallationTarget> {
        InstallationTarget::decode(&serde_json::to_vec(value).unwrap())
    }

    #[test]
    fn resolves_public_control_plane_beside_the_target() {
        let target = decode(&target()).unwrap();
        assert_eq!(
            target.control_plane_path(Path::new("fork/installation.json")),
            Path::new("fork/gateway.json")
        );
        assert_eq!(target.kubernetes.context, "k3d-fork");
    }

    #[test]
    fn committed_installation_inputs_validate() {
        for bytes in [
            include_bytes!("../../../examples/bioma/installation-target.json").as_slice(),
            include_bytes!("../../../testing/fixtures/fork-installation/installation-target.json")
                .as_slice(),
        ] {
            InstallationTarget::decode(bytes).unwrap();
        }
    }

    #[test]
    fn rejects_other_formats_and_unknown_fields() {
        let mut value = target();
        value["schema"] = json!("veoveo.ai/installation-target/v0");
        assert!(
            decode(&value)
                .unwrap_err()
                .to_string()
                .contains("regenerate")
        );
        for path in [
            "",
            "/kubernetes",
            "/operator",
            "/recordingCatalog",
            "/artifactConsumer",
            "/recordingCatalog/hostMappings/0",
        ] {
            let mut value = target();
            value.pointer_mut(path).unwrap()["unknown"] = json!(true);
            assert!(decode(&value).is_err(), "{path}");
        }
    }

    #[test]
    fn rejects_unsafe_or_ambiguous_coordinates() {
        for (path, bad) in [
            ("/localBaseUrl", json!("http://remote.example")),
            ("/publicBaseUrl", json!("http://fork.example.test")),
            (
                "/publicBaseUrl",
                json!("https://user:secret@fork.example.test"),
            ),
            ("/publicBaseUrl", json!("https://fork.example.test/path")),
            (
                "/publicBaseUrl",
                json!("https://fork.example.test/?key=secret"),
            ),
            ("/controlPlane", json!("/etc/gateway.json")),
            ("/kubernetes/namespace", json!("bad/name")),
            (
                "/expectedDeployments",
                json!(["mcp-gateway", "mcp-gateway"]),
            ),
            ("/operator/scopes", json!(["a", "a"])),
            ("/operator/scopes", json!(["a b"])),
            ("/operator/comparisonContext", json!("inspection")),
            ("/recordingCatalog/hostMappings", json!([])),
            (
                "/artifactConsumer/internalSigningSecret",
                json!("bad/secret"),
            ),
            ("/artifactConsumer/pythonDeployment", json!("BadDeployment")),
            (
                "/artifactConsumer/artifactServiceUrl",
                json!("http://user:secret@artifact-plane"),
            ),
        ] {
            let mut value = target();
            *value.pointer_mut(path).unwrap() = bad;
            assert!(decode(&value).is_err(), "{path}");
        }
    }

    #[test]
    fn artifact_service_recovery_requires_explicit_closed_workload_selection() {
        let mut value = target();
        value["expectedDeployments"] = json!(["mcp-gateway", "artifact-service"]);
        value["artifactConsumer"]["serviceRecovery"] = json!({
            "deployment":"artifact-service", "pod":"artifact-service-selected", "container":"artifact-service"
        });
        decode(&value).unwrap();
        for (field, invalid) in [
            ("deployment", json!("artifact-mcp")),
            ("pod", json!("BadPod")),
            ("container", json!("bad/container")),
            ("container", json!("metrics")),
            ("unknown", json!(true)),
        ] {
            let mut invalid_target = value.clone();
            invalid_target["artifactConsumer"]["serviceRecovery"][field] = invalid;
            assert!(decode(&invalid_target).is_err(), "{field}");
        }
        value["expectedDeployments"] = json!(["mcp-gateway", "python-consumer"]);
        value["artifactConsumer"]["serviceRecovery"]["deployment"] = json!("python-consumer");
        assert!(decode(&value).is_err());
    }

    #[test]
    fn accepts_tls_catalog_without_host_overrides_and_ipv6_loopback() {
        let mut value = target();
        value["localBaseUrl"] = json!("http://[::1]:18781");
        value["recordingCatalog"]["redapUrl"] = json!("rerun+https://catalog.example.test");
        value["recordingCatalog"]["hostMappings"] = json!([]);
        decode(&value).unwrap();
    }
    #[test]
    fn administrator_obeys_the_same_closed_selection_rules() {
        let mut value = target();
        value["administrator"] = value["operator"].clone();
        decode(&value).unwrap();
        for (field, invalid) in [
            ("clientId", json!("")),
            ("scopes", json!(["a", "a"])),
            ("comparisonContext", json!("inspection")),
            ("unknown", json!(true)),
        ] {
            let mut invalid_target = value.clone();
            invalid_target["administrator"][field] = invalid;
            assert!(decode(&invalid_target).is_err(), "{field}");
        }
    }
}
