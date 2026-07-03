use std::{collections::BTreeSet, fmt, path::Path};

use anyhow::{Result, anyhow, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{DataLabelId, SecretSource, ServerSlug};

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
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
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
    "Self-hosted deployment profile id, such as `local`, `enterprise`, or `regulated`."
);
deployment_id!(
    DeploymentRequirementId,
    "Stable id for a deployment requirement entry."
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelfHostedDeploymentProfile {
    pub id: DeploymentProfileId,
    pub kind: DeploymentProfileKind,
    #[serde(default)]
    pub required_services: BTreeSet<DeploymentServiceKind>,
    pub service_to_service: ServiceToServiceSecurity,
    #[serde(default)]
    pub secret_sources: BTreeSet<SecretSource>,
    #[serde(default)]
    pub object_stores: Vec<ObjectStoreDeployment>,
    #[serde(default)]
    pub state_stores: Vec<StateStoreDeployment>,
    #[serde(default)]
    pub telemetry_sinks: Vec<TelemetrySinkDeployment>,
    #[serde(default)]
    pub ingress: Vec<NetworkBoundaryRule>,
    #[serde(default)]
    pub egress: Vec<NetworkBoundaryRule>,
    pub retention: DataRetentionPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regulated_controls: Option<RegulatedDataControls>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentProfileKind {
    Local,
    Enterprise,
    Regulated,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentServiceKind {
    Gateway,
    HostedMcpServer,
    ObjectStore,
    StateStore,
    SecretManager,
    IdentityProvider,
    AuthorizationServer,
    TelemetryCollector,
    TunnelOrIngress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceToServiceSecurity {
    pub gateway_identity: GatewayToServerIdentity,
    pub transport: ServiceToServiceTransport,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GatewayToServerIdentity {
    GatewaySignedJwt,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ServiceToServiceTransport {
    PrivateNetworkPlaintext,
    Tls,
    MutualTls,
    ServiceMeshMtls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectStoreDeployment {
    pub id: DeploymentRequirementId,
    pub kind: ObjectStoreKind,
    #[serde(default)]
    pub servers: BTreeSet<ServerSlug>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<DeploymentEndpoint>,
    pub server_side_encryption_required: bool,
    pub customer_managed_keys_required: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ObjectStoreKind {
    S3Compatible,
    AwsS3,
    CloudflareR2,
    AzureBlob,
    Gcs,
    EnterpriseManaged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StateStoreDeployment {
    pub id: DeploymentRequirementId,
    pub kind: StateStoreKind,
    #[serde(default)]
    pub owners: BTreeSet<StateStoreOwner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<DeploymentEndpoint>,
    pub durable_volume_required: bool,
    pub encrypted_at_rest_required: bool,
    pub customer_managed_keys_required: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StateStoreKind {
    #[serde(rename = "duckdb")]
    DuckDb,
    EnterpriseManaged,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StateStoreOwner {
    Gateway,
    Server { server: ServerSlug },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TelemetrySinkDeployment {
    pub id: DeploymentRequirementId,
    pub kind: TelemetrySinkKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<DeploymentEndpoint>,
    #[serde(default)]
    pub signals: BTreeSet<TelemetrySignal>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TelemetrySinkKind {
    OpenTelemetryCollector,
    Siem,
    EnterpriseManaged,
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
pub struct NetworkBoundaryRule {
    pub id: DeploymentRequirementId,
    pub target_kind: NetworkTargetKind,
    pub target: NetworkTarget,
    #[serde(default)]
    pub ports: BTreeSet<u16>,
    pub tls_required: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum NetworkTargetKind {
    Gateway,
    HostedMcpServer,
    ObjectStore,
    StateStore,
    SecretManager,
    IdentityProvider,
    AuthorizationServer,
    TelemetryCollector,
    TunnelOrIngress,
    ExternalProviderApi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DataRetentionPolicy {
    pub task_metadata_days: u32,
    pub artifact_metadata_days: u32,
    pub artifact_bytes_days: u32,
    pub usage_analytics_days: u32,
    pub audit_event_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RegulatedDataControls {
    #[serde(default)]
    pub allowed_labels: BTreeSet<DataLabelId>,
    pub require_us_person: bool,
    pub require_private_network: bool,
    pub require_customer_managed_keys: bool,
    pub require_audit_export: bool,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct NetworkTarget(String);

impl NetworkTarget {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_network_target(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for NetworkTarget {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<NetworkTarget> for String {
    fn from(value: NetworkTarget) -> Self {
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
        let mut ids = BTreeSet::new();
        for profile in &self.profiles {
            if !ids.insert(profile.id.clone()) {
                bail!("duplicate deployment profile `{}`", profile.id);
            }
            profile.validate()?;
        }
        if self.profiles.is_empty() {
            bail!("deployment plan must define at least one profile");
        }
        Ok(())
    }
}

impl SelfHostedDeploymentProfile {
    pub fn validate(&self) -> Result<()> {
        require_nonempty(
            !self.required_services.is_empty(),
            &self.id,
            "required_services",
        )?;
        require_nonempty(!self.secret_sources.is_empty(), &self.id, "secret_sources")?;
        self.validate_secret_sources()?;
        require_nonempty(!self.object_stores.is_empty(), &self.id, "object_stores")?;
        require_nonempty(!self.state_stores.is_empty(), &self.id, "state_stores")?;
        require_nonempty(
            !self.telemetry_sinks.is_empty(),
            &self.id,
            "telemetry_sinks",
        )?;
        require_nonempty(!self.ingress.is_empty(), &self.id, "ingress")?;
        require_nonempty(!self.egress.is_empty(), &self.id, "egress")?;
        self.retention.validate(&self.id)?;
        self.validate_required_services()?;

        for object_store in &self.object_stores {
            object_store.validate(&self.id)?;
        }
        for state_store in &self.state_stores {
            state_store.validate(&self.id)?;
        }
        self.validate_state_store_coverage()?;
        for telemetry_sink in &self.telemetry_sinks {
            telemetry_sink.validate(&self.id)?;
        }
        for rule in self.ingress.iter().chain(&self.egress) {
            rule.validate(&self.id)?;
        }
        self.validate_network_coverage()?;
        self.service_to_service.validate(&self.id, self.kind)?;

        match self.kind {
            DeploymentProfileKind::Local => {}
            DeploymentProfileKind::Enterprise | DeploymentProfileKind::Regulated => {
                self.validate_enterprise_boundary()?;
                if self.secret_sources.contains(&SecretSource::Env) {
                    bail!(
                        "deployment profile `{}` cannot use env secrets for {:?}",
                        self.id,
                        self.kind
                    );
                }
                if !self.secret_sources.iter().any(enterprise_secret_source) {
                    bail!(
                        "deployment profile `{}` must declare an enterprise secret source",
                        self.id
                    );
                }
            }
        }

        if self.kind == DeploymentProfileKind::Regulated {
            let controls = self.regulated_controls.as_ref().ok_or_else(|| {
                anyhow!(
                    "deployment profile `{}` must declare regulated controls",
                    self.id
                )
            })?;
            controls.validate(&self.id)?;
        }

        Ok(())
    }

    fn validate_required_services(&self) -> Result<()> {
        for service in [
            DeploymentServiceKind::Gateway,
            DeploymentServiceKind::HostedMcpServer,
            DeploymentServiceKind::ObjectStore,
            DeploymentServiceKind::StateStore,
            DeploymentServiceKind::TelemetryCollector,
            DeploymentServiceKind::TunnelOrIngress,
        ] {
            self.require_service(service)?;
        }

        if matches!(
            self.kind,
            DeploymentProfileKind::Enterprise | DeploymentProfileKind::Regulated
        ) {
            for service in [
                DeploymentServiceKind::SecretManager,
                DeploymentServiceKind::IdentityProvider,
                DeploymentServiceKind::AuthorizationServer,
            ] {
                self.require_service(service)?;
            }
        }

        Ok(())
    }

    fn validate_secret_sources(&self) -> Result<()> {
        for source in &self.secret_sources {
            if !implemented_secret_source(source) {
                bail!(
                    "deployment profile `{}` uses secret source `{source:?}` that is not implemented by the gateway resolver",
                    self.id
                );
            }
        }
        Ok(())
    }

    fn validate_enterprise_boundary(&self) -> Result<()> {
        for rule in self.ingress.iter().chain(&self.egress) {
            if !rule.tls_required {
                bail!(
                    "deployment profile `{}` network rule `{}` must require TLS",
                    self.id,
                    rule.id
                );
            }
        }

        for object_store in &self.object_stores {
            if !object_store.server_side_encryption_required {
                bail!(
                    "deployment profile `{}` object store `{}` must require server-side encryption",
                    self.id,
                    object_store.id
                );
            }
            if !object_store.customer_managed_keys_required {
                bail!(
                    "deployment profile `{}` object store `{}` must require customer-managed keys",
                    self.id,
                    object_store.id
                );
            }
        }

        for state_store in &self.state_stores {
            if !state_store.durable_volume_required {
                bail!(
                    "deployment profile `{}` state store `{}` must require durable storage",
                    self.id,
                    state_store.id
                );
            }
            if !state_store.encrypted_at_rest_required {
                bail!(
                    "deployment profile `{}` state store `{}` must require encryption at rest",
                    self.id,
                    state_store.id
                );
            }
            if !state_store.customer_managed_keys_required {
                bail!(
                    "deployment profile `{}` state store `{}` must require customer-managed keys",
                    self.id,
                    state_store.id
                );
            }
        }

        if !self
            .telemetry_sinks
            .iter()
            .any(|sink| sink.signals.contains(&TelemetrySignal::AuditEvents))
        {
            bail!(
                "deployment profile `{}` must export audit events to a telemetry sink",
                self.id
            );
        }

        Ok(())
    }

    fn validate_network_coverage(&self) -> Result<()> {
        for rule in self.ingress.iter().chain(&self.egress) {
            if let Some(service) = rule.target_kind.service_kind()
                && !self.required_services.contains(&service)
            {
                bail!(
                    "deployment profile `{}` network rule `{}` targets service `{service:?}` that is not declared in required_services",
                    self.id,
                    rule.id
                );
            }
        }

        if !self.ingress.iter().any(|rule| {
            matches!(
                rule.target_kind,
                NetworkTargetKind::Gateway
                    | NetworkTargetKind::AuthorizationServer
                    | NetworkTargetKind::TunnelOrIngress
            )
        }) {
            bail!(
                "deployment profile `{}` must declare ingress for gateway, authorization server, or tunnel/ingress",
                self.id
            );
        }

        for kind in [
            NetworkTargetKind::HostedMcpServer,
            NetworkTargetKind::ObjectStore,
            NetworkTargetKind::TelemetryCollector,
        ] {
            self.require_egress_target(kind)?;
        }

        if self.secret_sources.iter().any(enterprise_secret_source) {
            self.require_egress_target(NetworkTargetKind::SecretManager)?;
        }

        if self
            .required_services
            .contains(&DeploymentServiceKind::IdentityProvider)
        {
            self.require_egress_target(NetworkTargetKind::IdentityProvider)?;
        }

        if self
            .state_stores
            .iter()
            .any(|store| store.kind == StateStoreKind::EnterpriseManaged)
        {
            self.require_egress_target(NetworkTargetKind::StateStore)?;
        }

        Ok(())
    }

    fn require_egress_target(&self, kind: NetworkTargetKind) -> Result<()> {
        if self.egress.iter().any(|rule| rule.target_kind == kind) {
            Ok(())
        } else {
            bail!(
                "deployment profile `{}` must declare egress for `{kind:?}`",
                self.id
            )
        }
    }

    fn validate_state_store_coverage(&self) -> Result<()> {
        let has_gateway = self.state_stores.iter().any(|store| {
            store
                .owners
                .iter()
                .any(|owner| matches!(owner, StateStoreOwner::Gateway))
        });
        if !has_gateway {
            bail!(
                "deployment profile `{}` must declare a state store owned by the gateway",
                self.id
            );
        }

        let deployed_servers = self
            .object_stores
            .iter()
            .flat_map(|store| store.servers.iter().cloned())
            .collect::<BTreeSet<_>>();
        let state_store_servers = self
            .state_stores
            .iter()
            .flat_map(|store| store.owners.iter())
            .filter_map(|owner| match owner {
                StateStoreOwner::Gateway => None,
                StateStoreOwner::Server { server } => Some(server.clone()),
            })
            .collect::<BTreeSet<_>>();
        if state_store_servers.is_empty() {
            bail!(
                "deployment profile `{}` must declare at least one hosted-server state store",
                self.id
            );
        }
        for server in &state_store_servers {
            if !deployed_servers.contains(server) {
                bail!(
                    "deployment profile `{}` state store owner references undeployed hosted server `{server}`",
                    self.id
                );
            }
        }
        for server in deployed_servers {
            if !state_store_servers.contains(&server) {
                bail!(
                    "deployment profile `{}` must declare a state store for hosted server `{server}`",
                    self.id
                );
            }
        }

        Ok(())
    }

    fn require_service(&self, service: DeploymentServiceKind) -> Result<()> {
        if self.required_services.contains(&service) {
            Ok(())
        } else {
            bail!(
                "deployment profile `{}` must declare required service `{service:?}`",
                self.id
            )
        }
    }
}

impl ObjectStoreDeployment {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        require_nonempty(!self.servers.is_empty(), profile, "object_stores.servers")?;
        if self.customer_managed_keys_required && !self.server_side_encryption_required {
            bail!(
                "deployment profile `{profile}` object store `{}` requires customer-managed keys without server-side encryption",
                self.id
            );
        }
        Ok(())
    }
}

impl StateStoreDeployment {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        require_nonempty(!self.owners.is_empty(), profile, "state_stores.owners")?;
        if self.customer_managed_keys_required && !self.encrypted_at_rest_required {
            bail!(
                "deployment profile `{profile}` state store `{}` requires customer-managed keys without encryption at rest",
                self.id
            );
        }
        if matches!(self.kind, StateStoreKind::DuckDb) && self.endpoint.is_some() {
            bail!(
                "deployment profile `{profile}` DuckDB state store `{}` must not declare an endpoint",
                self.id
            );
        }
        Ok(())
    }
}

impl TelemetrySinkDeployment {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        require_nonempty(!self.signals.is_empty(), profile, "telemetry_sinks.signals")
    }
}

impl NetworkBoundaryRule {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        require_nonempty(!self.ports.is_empty(), profile, "network.ports")?;
        if self.ports.contains(&0) {
            bail!(
                "deployment profile `{profile}` network rule `{}` cannot use port 0",
                self.id
            );
        }
        if self.target_kind == NetworkTargetKind::ExternalProviderApi && !self.tls_required {
            bail!(
                "deployment profile `{profile}` external provider network rule `{}` must require TLS",
                self.id
            );
        }
        Ok(())
    }
}

impl NetworkTargetKind {
    fn service_kind(self) -> Option<DeploymentServiceKind> {
        match self {
            Self::Gateway => Some(DeploymentServiceKind::Gateway),
            Self::HostedMcpServer => Some(DeploymentServiceKind::HostedMcpServer),
            Self::ObjectStore => Some(DeploymentServiceKind::ObjectStore),
            Self::StateStore => Some(DeploymentServiceKind::StateStore),
            Self::SecretManager => Some(DeploymentServiceKind::SecretManager),
            Self::IdentityProvider => Some(DeploymentServiceKind::IdentityProvider),
            Self::AuthorizationServer => Some(DeploymentServiceKind::AuthorizationServer),
            Self::TelemetryCollector => Some(DeploymentServiceKind::TelemetryCollector),
            Self::TunnelOrIngress => Some(DeploymentServiceKind::TunnelOrIngress),
            Self::ExternalProviderApi => None,
        }
    }
}

impl ServiceToServiceSecurity {
    fn validate(&self, profile: &DeploymentProfileId, kind: DeploymentProfileKind) -> Result<()> {
        match self.gateway_identity {
            GatewayToServerIdentity::GatewaySignedJwt => {}
        }

        if matches!(
            kind,
            DeploymentProfileKind::Enterprise | DeploymentProfileKind::Regulated
        ) && !self.transport.is_authenticated_transport()
        {
            bail!(
                "deployment profile `{profile}` requires mTLS or service-mesh mTLS for gateway-to-server transport"
            );
        }
        Ok(())
    }
}

impl ServiceToServiceTransport {
    fn is_authenticated_transport(self) -> bool {
        matches!(self, Self::MutualTls | Self::ServiceMeshMtls)
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

impl RegulatedDataControls {
    fn validate(&self, profile: &DeploymentProfileId) -> Result<()> {
        require_nonempty(
            !self.allowed_labels.is_empty(),
            profile,
            "regulated.allowed_labels",
        )?;
        if !self.require_us_person {
            bail!("deployment profile `{profile}` regulated controls require US-person gating");
        }
        if !self.require_private_network {
            bail!("deployment profile `{profile}` regulated controls require private networking");
        }
        if !self.require_customer_managed_keys {
            bail!(
                "deployment profile `{profile}` regulated controls require customer-managed keys"
            );
        }
        if !self.require_audit_export {
            bail!("deployment profile `{profile}` regulated controls require audit export");
        }
        Ok(())
    }
}

fn require_nonempty(condition: bool, profile: &DeploymentProfileId, field: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        bail!("deployment profile `{profile}` must declare `{field}`")
    }
}

fn enterprise_secret_source(source: &SecretSource) -> bool {
    matches!(source, SecretSource::Vault | SecretSource::HcpVault)
}

fn implemented_secret_source(source: &SecretSource) -> bool {
    matches!(
        source,
        SecretSource::Env | SecretSource::Vault | SecretSource::HcpVault
    )
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
    Ok(value.to_string())
}

fn validate_path_segment(value: &str, name: &str) -> Result<()> {
    if value.is_empty() {
        return Err(anyhow!("{name} must not be empty"));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
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
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        bail!("deployment endpoint must start with http:// or https://");
    }
    Ok(())
}

fn validate_network_target(value: &str) -> Result<()> {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        bail!("network target must not be empty or contain whitespace");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
