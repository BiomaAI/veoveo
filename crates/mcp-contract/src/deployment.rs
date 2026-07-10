use std::{collections::BTreeSet, fmt, path::Path};

use anyhow::{Result, anyhow, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ServerSlug;

macro_rules! deployment_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                validate_path_segment(&value, stringify!($name))?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = anyhow::Error;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

deployment_id!(
    DeploymentProfileId,
    "Stable identifier for one canonical self-hosted installation profile."
);
deployment_id!(
    DeploymentRequirementId,
    "Stable identifier for one deployment requirement."
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicDeployment {
    base_url: String,
    host_authority: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPublicEndpoint {
    public_url: String,
    mount_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelfHostedDeploymentPlan {
    pub profiles: Vec<SelfHostedDeploymentProfile>,
}

/// An autonomous Veoveo installation owned by one enterprise.
///
/// Tenants are internal isolation boundaries inside this installation. This is
/// deliberately not a vendor-hosted or multi-customer control-plane model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelfHostedDeploymentProfile {
    pub id: DeploymentProfileId,
    pub installation_scope: InstallationScope,
    pub installation_form: InstallationForm,
    pub connectivity: ConnectivityMode,
    pub tenant_model: TenantModel,
    pub platform_store: PlatformStoreDeployment,
    pub object_store: ObjectStoreDeployment,
    pub analytical_runtime: AnalyticalRuntimeDeployment,
    pub ingress: IngressDeployment,
    pub identity_provider: IdentityProviderDeployment,
    pub secret_manager: SecretManagerDeployment,
    pub service_to_service: ServiceToServiceSecurity,
    pub telemetry: TelemetryDeployment,
    #[serde(default)]
    pub services: BTreeSet<DeploymentServiceKind>,
    pub retention: DataRetentionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallationScope {
    OneEnterprise,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum InstallationForm {
    Compose,
    Helm,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConnectivityMode {
    Connected,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TenantModel {
    pub kind: TenantModelKind,
    pub tenant_keys_are_installation_local: bool,
    pub cross_tenant_access_denied_by_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TenantModelKind {
    InternalTenants,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentServiceKind {
    Gateway,
    ConsoleBff,
    Console,
    ArtifactService,
    ArtifactMcp,
    RecordingHub,
    RecordingMcp,
    HostedMcpServer,
    PlatformStore,
    ObjectStore,
    TelemetryCollector,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlatformStoreDeployment {
    pub engine: PlatformStoreEngine,
    pub version: SurrealDbVersion,
    pub storage_engine: SurrealStorageEngine,
    pub topology: DatabaseTopology,
    pub database_ha: DatabaseHighAvailability,
    pub durable_volume_required: bool,
    pub changefeed_source_of_truth: ChangefeedSourceOfTruth,
    pub live_queries: LiveQueryRole,
    pub endpoint: DeploymentEndpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlatformStoreEngine {
    SurrealDb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SurrealDbVersion {
    #[serde(rename = "3.2.0")]
    #[schemars(rename = "3.2.0")]
    V3_2_0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SurrealStorageEngine {
    RocksDb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseTopology {
    SingleNode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseHighAvailability {
    OutOfScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangefeedSourceOfTruth {
    DurableOutbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveQueryRole {
    BestEffortLatencyPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectStoreDeployment {
    pub kind: ObjectStoreKind,
    pub endpoint: DeploymentEndpoint,
    pub bucket: String,
    pub server_side_encryption_required: bool,
    pub customer_managed_keys_supported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectStoreKind {
    RustFs,
    ExternalS3Compatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AnalyticalRuntimeDeployment {
    pub engine: AnalyticalRuntimeEngine,
    pub purpose: AnalyticalRuntimePurpose,
    pub arbitrary_sql: bool,
    pub owner_scoped_workspaces: bool,
    pub durable_platform_state: bool,
    pub external_data_access: ExternalDataAccess,
    pub container_sandbox_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnalyticalRuntimeEngine {
    DuckDb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnalyticalRuntimePurpose {
    IsolatedAnalyticalWorkspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalDataAccess {
    GovernedIngestArtifactOrAttach,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IngressDeployment {
    pub kind: IngressKind,
    pub public_base_url: DeploymentEndpoint,
    pub tls_terminated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IngressKind {
    ComposePublishedPort,
    KubernetesIngress,
    ExternalReverseProxy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IdentityProviderDeployment {
    pub kind: IdentityProviderKind,
    pub issuer: DeploymentEndpoint,
    pub discovery_available_offline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityProviderKind {
    ExternalOidc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretManagerDeployment {
    pub kind: SecretManagerKind,
    pub existing_secret_name: String,
    pub rotation_owned_by_operator: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretManagerKind {
    ComposeSecretFiles,
    KubernetesExistingSecret,
    ExternalSecretManager,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceToServiceSecurity {
    pub gateway_identity: GatewayToServerIdentity,
    pub transport: ServiceToServiceTransport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GatewayToServerIdentity {
    GatewaySignedEd25519Jwt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceToServiceTransport {
    PrivateNetworkPlaintext,
    MutualTls,
    ServiceMeshMtls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TelemetryDeployment {
    pub collector: TelemetryCollectorKind,
    pub endpoint: DeploymentEndpoint,
    #[serde(default)]
    pub signals: BTreeSet<TelemetrySignal>,
    pub siem_export_supported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryCollectorKind {
    OpenTelemetryCollector,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TelemetrySignal {
    Logs,
    Traces,
    Metrics,
    AuditEvents,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DataRetentionPolicy {
    pub task_metadata_days: u32,
    pub artifact_metadata_days: u32,
    pub artifact_bytes_days: u32,
    pub usage_analytics_days: u32,
    pub audit_event_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DeploymentEndpoint(String);

impl DeploymentEndpoint {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_endpoint(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for DeploymentEndpoint {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<DeploymentEndpoint> for String {
    fn from(value: DeploymentEndpoint) -> Self {
        value.0
    }
}

impl PublicDeployment {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        let (base_url, host_authority) = normalize_base_url(base_url.as_ref())?;
        Ok(Self {
            base_url,
            host_authority,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn host_authority(&self) -> &str {
        &self.host_authority
    }

    pub fn server(&self, server_slug: impl AsRef<str>) -> Result<ServerPublicEndpoint> {
        let server_slug = normalize_server_slug(server_slug.as_ref())?;
        let mount_path = format!("/{server_slug}");
        let public_url = format!("{}{}", self.base_url, mount_path);
        Ok(ServerPublicEndpoint {
            public_url,
            mount_path,
        })
    }
}

impl SelfHostedDeploymentPlan {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let plan = serde_json::from_str::<Self>(&text)?;
        plan.validate()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<()> {
        if self.profiles.is_empty() {
            bail!("deployment plan must define at least one profile");
        }
        let mut ids = BTreeSet::new();
        let mut shapes = BTreeSet::new();
        for profile in &self.profiles {
            if !ids.insert(profile.id.clone()) {
                bail!("duplicate deployment profile `{}`", profile.id);
            }
            shapes.insert((profile.installation_form, profile.connectivity));
            profile.validate()?;
        }
        for shape in [
            (InstallationForm::Compose, ConnectivityMode::Connected),
            (InstallationForm::Compose, ConnectivityMode::Offline),
            (InstallationForm::Helm, ConnectivityMode::Connected),
            (InstallationForm::Helm, ConnectivityMode::Offline),
        ] {
            if !shapes.contains(&shape) {
                bail!(
                    "deployment plan must include canonical `{:?}` `{:?}` installation profile",
                    shape.0,
                    shape.1
                );
            }
        }
        Ok(())
    }
}

impl SelfHostedDeploymentProfile {
    pub fn validate(&self) -> Result<()> {
        if self.installation_scope != InstallationScope::OneEnterprise {
            bail!(
                "deployment profile `{}` must be one enterprise installation",
                self.id
            );
        }
        if self.tenant_model.kind != TenantModelKind::InternalTenants
            || !self.tenant_model.tenant_keys_are_installation_local
            || !self.tenant_model.cross_tenant_access_denied_by_default
        {
            bail!(
                "deployment profile `{}` must use installation-local internal tenants with deny-by-default isolation",
                self.id
            );
        }
        self.platform_store.validate(&self.id)?;
        self.object_store.validate(&self.id)?;
        self.analytical_runtime.validate(&self.id)?;
        self.ingress.validate(&self.id, self.installation_form)?;
        self.identity_provider
            .validate(&self.id, self.connectivity)?;
        self.secret_manager
            .validate(&self.id, self.installation_form)?;
        self.service_to_service.validate(&self.id)?;
        self.telemetry.validate(&self.id)?;
        self.retention.validate(&self.id)?;

        let required = BTreeSet::from([
            DeploymentServiceKind::Gateway,
            DeploymentServiceKind::ConsoleBff,
            DeploymentServiceKind::Console,
            DeploymentServiceKind::ArtifactService,
            DeploymentServiceKind::ArtifactMcp,
            DeploymentServiceKind::RecordingHub,
            DeploymentServiceKind::RecordingMcp,
            DeploymentServiceKind::HostedMcpServer,
            DeploymentServiceKind::PlatformStore,
            DeploymentServiceKind::ObjectStore,
            DeploymentServiceKind::TelemetryCollector,
        ]);
        if self.services != required {
            bail!(
                "deployment profile `{}` services must exactly describe the canonical autonomous installation",
                self.id
            );
        }
        if self.connectivity == ConnectivityMode::Offline
            && !self.identity_provider.discovery_available_offline
        {
            bail!(
                "offline deployment profile `{}` requires an identity provider reachable inside the offline boundary",
                self.id
            );
        }
        Ok(())
    }
}

impl PlatformStoreDeployment {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        if self.engine != PlatformStoreEngine::SurrealDb
            || self.version != SurrealDbVersion::V3_2_0
            || self.storage_engine != SurrealStorageEngine::RocksDb
            || self.topology != DatabaseTopology::SingleNode
            || self.database_ha != DatabaseHighAvailability::OutOfScope
            || !self.durable_volume_required
            || self.changefeed_source_of_truth != ChangefeedSourceOfTruth::DurableOutbox
            || self.live_queries != LiveQueryRole::BestEffortLatencyPath
        {
            bail!(
                "deployment profile `{profile}` must use required SurrealDB 3.2.0 single-node RocksDB; database HA is out of scope, durable outbox is authoritative, and LIVE is latency-only"
            );
        }
        if !(self.endpoint.as_str().starts_with("ws://")
            || self.endpoint.as_str().starts_with("wss://"))
        {
            bail!("deployment profile `{profile}` SurrealDB endpoint must use ws or wss");
        }
        Ok(())
    }
}

impl ObjectStoreDeployment {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        if self.bucket.is_empty() || self.bucket.chars().any(char::is_whitespace) {
            bail!("deployment profile `{profile}` object-store bucket is invalid");
        }
        if self.kind == ObjectStoreKind::ExternalS3Compatible
            && !self.endpoint.as_str().starts_with("https://")
        {
            bail!("deployment profile `{profile}` external object store must use HTTPS");
        }
        Ok(())
    }
}

impl AnalyticalRuntimeDeployment {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        if self.engine != AnalyticalRuntimeEngine::DuckDb
            || self.purpose != AnalyticalRuntimePurpose::IsolatedAnalyticalWorkspace
            || !self.arbitrary_sql
            || !self.owner_scoped_workspaces
            || self.durable_platform_state
            || self.external_data_access != ExternalDataAccess::GovernedIngestArtifactOrAttach
            || !self.container_sandbox_required
        {
            bail!(
                "deployment profile `{profile}` must keep DuckDB as a sandboxed arbitrary-SQL analytical workspace and never as the durable platform store"
            );
        }
        Ok(())
    }
}

impl IngressDeployment {
    fn validate(&self, profile: &DeploymentProfileId, form: InstallationForm) -> Result<()> {
        if !self.tls_terminated {
            bail!("deployment profile `{profile}` ingress must terminate TLS");
        }
        if !self.public_base_url.as_str().starts_with("https://") {
            bail!("deployment profile `{profile}` public ingress must use HTTPS");
        }
        let valid_for_form = matches!(
            (form, self.kind),
            (
                InstallationForm::Compose,
                IngressKind::ComposePublishedPort | IngressKind::ExternalReverseProxy
            ) | (
                InstallationForm::Helm,
                IngressKind::KubernetesIngress | IngressKind::ExternalReverseProxy
            )
        );
        if !valid_for_form {
            bail!(
                "deployment profile `{profile}` ingress choice does not match its installation form"
            );
        }
        Ok(())
    }
}

impl IdentityProviderDeployment {
    fn validate(
        &self,
        profile: &DeploymentProfileId,
        connectivity: ConnectivityMode,
    ) -> Result<()> {
        if self.kind != IdentityProviderKind::ExternalOidc {
            bail!("deployment profile `{profile}` must use an external OIDC provider");
        }
        if !self.issuer.as_str().starts_with("https://") {
            bail!("deployment profile `{profile}` OIDC issuer must use HTTPS");
        }
        if connectivity == ConnectivityMode::Offline && !self.discovery_available_offline {
            bail!("deployment profile `{profile}` cannot depend on online OIDC discovery");
        }
        Ok(())
    }
}

impl SecretManagerDeployment {
    fn validate(&self, profile: &DeploymentProfileId, form: InstallationForm) -> Result<()> {
        if self.existing_secret_name.is_empty()
            || self.existing_secret_name.chars().any(char::is_whitespace)
        {
            bail!("deployment profile `{profile}` must name an existing secret");
        }
        let valid_for_form = matches!(
            (form, self.kind),
            (
                InstallationForm::Compose,
                SecretManagerKind::ComposeSecretFiles
            ) | (
                InstallationForm::Compose,
                SecretManagerKind::ExternalSecretManager
            ) | (
                InstallationForm::Helm,
                SecretManagerKind::KubernetesExistingSecret
            ) | (
                InstallationForm::Helm,
                SecretManagerKind::ExternalSecretManager
            )
        );
        if !valid_for_form {
            bail!(
                "deployment profile `{profile}` secret-manager choice does not match its installation form"
            );
        }
        if !self.rotation_owned_by_operator {
            bail!("deployment profile `{profile}` secret rotation must be operator-owned");
        }
        Ok(())
    }
}

impl ServiceToServiceSecurity {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        if self.gateway_identity != GatewayToServerIdentity::GatewaySignedEd25519Jwt {
            bail!("deployment profile `{profile}` requires gateway-signed Ed25519 JWT identity");
        }
        Ok(())
    }
}

impl TelemetryDeployment {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        let required = BTreeSet::from([
            TelemetrySignal::Logs,
            TelemetrySignal::Traces,
            TelemetrySignal::Metrics,
            TelemetrySignal::AuditEvents,
        ]);
        if self.signals != required || !self.siem_export_supported {
            bail!(
                "deployment profile `{profile}` telemetry must collect logs, traces, metrics, audit events, and support SIEM export"
            );
        }
        Ok(())
    }
}

impl DataRetentionPolicy {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        for (name, days) in [
            ("task_metadata_days", self.task_metadata_days),
            ("artifact_metadata_days", self.artifact_metadata_days),
            ("artifact_bytes_days", self.artifact_bytes_days),
            ("usage_analytics_days", self.usage_analytics_days),
            ("audit_event_days", self.audit_event_days),
        ] {
            if days == 0 {
                bail!("deployment profile `{profile}` retention `{name}` must be greater than 0");
            }
        }
        Ok(())
    }
}

impl ServerPublicEndpoint {
    pub fn public_url(&self) -> &str {
        &self.public_url
    }

    pub fn mount_path(&self) -> &str {
        &self.mount_path
    }

    pub fn path(&self, child: &str) -> String {
        let child = child.trim_matches('/');
        if child.is_empty() {
            self.mount_path.clone()
        } else {
            format!("{}/{}", self.mount_path, child)
        }
    }

    pub fn url(&self, child: &str) -> String {
        let child = child.trim_matches('/');
        if child.is_empty() {
            self.public_url.clone()
        } else {
            format!("{}/{}", self.public_url, child)
        }
    }
}

fn normalize_base_url(input: &str) -> Result<(String, String)> {
    let value = input.trim().trim_end_matches('/').to_string();
    if value.is_empty() {
        return Err(anyhow!("missing PUBLIC_BASE_URL"));
    }
    let authority = if let Some(rest) = value.strip_prefix("http://") {
        rest
    } else if let Some(rest) = value.strip_prefix("https://") {
        rest
    } else {
        return Err(anyhow!(
            "PUBLIC_BASE_URL must start with http:// or https://"
        ));
    };
    if value.contains(['?', '#']) || value.chars().any(char::is_whitespace) {
        return Err(anyhow!(
            "PUBLIC_BASE_URL must not contain whitespace, query, or fragment"
        ));
    }
    if authority.is_empty() {
        return Err(anyhow!("PUBLIC_BASE_URL must include a host"));
    }
    if authority.contains('/') {
        return Err(anyhow!(
            "PUBLIC_BASE_URL must be an origin and must not include a path"
        ));
    }
    if authority.contains('@') {
        return Err(anyhow!("PUBLIC_BASE_URL must not include userinfo"));
    }
    let host_authority = authority.to_string();
    Ok((value, host_authority))
}

fn normalize_server_slug(input: &str) -> Result<String> {
    let value = input.trim();
    validate_path_segment(value, "server slug")?;
    ServerSlug::new(value)?;
    Ok(value.to_string())
}

fn validate_path_segment(value: &str, name: &str) -> Result<()> {
    if value.is_empty() {
        return Err(anyhow!("{name} must not be empty"));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
    }) {
        return Err(anyhow!(
            "{name} must contain only lowercase ASCII letters, digits, hyphen, or underscore"
        ));
    }
    Ok(())
}

fn validate_endpoint(value: &str) -> Result<()> {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        bail!("deployment endpoint must not be empty or contain whitespace");
    }
    if !(value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("ws://")
        || value.starts_with("wss://"))
    {
        bail!("deployment endpoint must use http, https, ws, or wss");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
