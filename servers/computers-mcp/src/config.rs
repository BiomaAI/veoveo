//! Installation inputs contain references to trust material, never credentials.
mod execution;
use execution::ExecutionConfiguration;
pub(crate) use execution::PreparedExecution;

use crate::{
    ApplicationError, MaintenanceProfiles, MaintenanceTransition, NamedTemplate, RetainedHomes,
    Templates,
};
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;
use veoveo_computers::CapacityPolicy;
use veoveo_computers::session_grants::SessionGrantPolicy;
use veoveo_computers_runtime::{
    AllocationConfig, DevelopmentTemplate, GatewayConfig, PERSISTENT_COMMAND, PersistentHome,
    parse_policy,
};

type Result<T> = std::result::Result<T, ConfigurationError>;
#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError {
    #[error("Computers config must be a readable regular JSON file of at most 1 MiB")]
    Document,
    #[error(
        "Computers config does not match veoveo.io/computers-service/v2 at line {line}, column {column}"
    )]
    Shape { line: usize, column: usize },
    #[error("Computers requires a non-nil providerInstanceId and a nonzero listen port")]
    Identity,
    #[error("Computers requires 1 to 64 valid allowedHosts authorities")]
    Hosts,
    #[error("Computers requires explicit canonical allowedOrigins and valid access lifetimes")]
    Access,
    #[error("Computer template {index} has an invalid profile or fingerprint")]
    Template { index: usize },
    #[error(
        "Computers requires 1 to 64 distinct admitted templates and an admitted defaultTemplate fingerprint"
    )]
    Templates,
    #[error("Computer maintenance requires distinct admitted and compatible source/target pairs")]
    Maintenance,
    #[error(
        "Computer execution requires valid policy, Artifact endpoint and qualified default template"
    )]
    Execution,
    #[error(
        "Computer command keys require 1 to 4 distinct IDs and private regular files of exactly 32 bytes"
    )]
    ExecutionKey,
    #[error("Computer provider endpoint, workspace and mTLS file references must be valid")]
    ProviderTrust,
    #[error("Computer allocator endpoint and mTLS file references must be valid")]
    StorageTrust,
    #[error("Computer configuration validation exceeded ten seconds")]
    TimedOut,
}
#[derive(Deserialize)]
pub enum ConfigSchema {
    #[serde(rename = "veoveo.io/computers-service/v2")]
    V2,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Configuration {
    schema: ConfigSchema,
    listen: SocketAddr,
    allowed_hosts: Vec<String>,
    allowed_origins: Vec<String>,
    access: SessionGrantPolicy,
    provider_instance_id: Uuid,
    capacity: Capacity,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Capacity {
    Unconfigured,
    #[serde(rename_all = "camelCase")]
    OpenshellDocker {
        gateway: Box<Gateway>,
        allocator: TlsEndpoint,
        limits: CapacityPolicy,
        templates: Vec<Template>,
        default_template: String,
        execution: Box<ExecutionConfiguration>,
        maintenance_transitions: Vec<MaintenanceTransition>,
    },
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TlsEndpoint {
    endpoint: String,
    ca_file: PathBuf,
    certificate_file: PathBuf,
    key_file: PathBuf,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Gateway {
    transport: TlsEndpoint,
    workspace: String,
}
impl Gateway {
    pub(crate) fn config(&self, id: Uuid) -> std::result::Result<GatewayConfig, ApplicationError> {
        GatewayConfig::new(
            id,
            self.transport.endpoint.clone(),
            self.workspace.clone(),
            self.transport.ca_file.clone(),
            self.transport.certificate_file.clone(),
            self.transport.key_file.clone(),
        )
        .map_err(|_| ApplicationError::Configuration)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Template {
    id: String,
    fingerprint: String,
    image: String,
    cpus: u32,
    memory_mib: u32,
    home_capacity_mib: u32,
    temporary_mib: u32,
    // Private provider policy uses its generated protobuf JSON mapping. The
    // runtime enforces the selected retained containment profile after parsing.
    policy: serde_json::Value,
}
pub struct PreparedConfiguration {
    pub(crate) listen: SocketAddr,
    pub(crate) allowed_hosts: Vec<String>,
    pub(crate) allowed_origins: crate::server::BrowserOrigins,
    pub(crate) access: SessionGrantPolicy,
    pub(crate) provider_instance_id: Uuid,
    pub(crate) templates: Templates,
    pub(crate) provider: Option<PreparedProvider>,
}
pub(crate) struct PreparedProvider {
    pub gateway: Gateway,
    pub homes: RetainedHomes,
    pub limits: CapacityPolicy,
    pub execution: PreparedExecution,
    pub maintenance: MaintenanceProfiles,
}
impl Configuration {
    pub async fn load(path: &Path) -> Result<Self> {
        tokio::time::timeout(Duration::from_secs(10), Self::read(path))
            .await
            .map_err(|_| ConfigurationError::TimedOut)?
    }
    async fn read(path: &Path) -> Result<Self> {
        use tokio::io::AsyncReadExt;
        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|_| ConfigurationError::Document)?;
        if !metadata.is_file() || metadata.len() > 1024 * 1024 {
            return Err(ConfigurationError::Document);
        }
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|_| ConfigurationError::Document)?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| ConfigurationError::Document)?;
        if bytes.len() > 1024 * 1024 {
            return Err(ConfigurationError::Document);
        }
        serde_json::from_slice(&bytes).map_err(|e| ConfigurationError::Shape {
            line: e.line(),
            column: e.column(),
        })
    }
    /// Validate the complete selected profile before connecting to or mutating
    /// the platform store. Provider availability is a later observable state.
    pub async fn prepare(self) -> Result<PreparedConfiguration> {
        tokio::time::timeout(Duration::from_secs(10), self.prepare_inner())
            .await
            .map_err(|_| ConfigurationError::TimedOut)?
    }
    async fn prepare_inner(self) -> Result<PreparedConfiguration> {
        let ConfigSchema::V2 = self.schema;
        if self.provider_instance_id.is_nil() || self.listen.port() == 0 {
            return Err(ConfigurationError::Identity);
        }
        if self.allowed_hosts.is_empty()
            || self.allowed_hosts.len() > 64
            || self
                .allowed_hosts
                .iter()
                .any(|h| veoveo_mcp_contract::parse_allowed_host_authority(h).is_none())
        {
            return Err(ConfigurationError::Hosts);
        }
        let allowed_origins = crate::server::BrowserOrigins::new(self.allowed_origins)
            .map_err(|_| ConfigurationError::Access)?;
        self.access
            .validate()
            .map_err(|_| ConfigurationError::Access)?;
        let (templates, provider) = match self.capacity {
            Capacity::Unconfigured => (
                Templates::new(vec![], None).map_err(|_| ConfigurationError::Templates)?,
                None,
            ),
            Capacity::OpenshellDocker {
                gateway,
                allocator,
                limits,
                templates,
                default_template,
                execution,
                maintenance_transitions,
            } => {
                if templates.is_empty() || templates.len() > 64 {
                    return Err(ConfigurationError::Templates);
                }
                let mut admitted = Vec::with_capacity(templates.len());
                for (index, t) in templates.into_iter().enumerate() {
                    let runtime = DevelopmentTemplate::new(
                        t.image,
                        t.cpus,
                        t.memory_mib,
                        parse_policy(&t.policy)
                            .map_err(|_| ConfigurationError::Template { index })?,
                        PERSISTENT_COMMAND.map(str::to_owned).into(),
                        Some(
                            PersistentHome::new(t.home_capacity_mib, t.temporary_mib)
                                .map_err(|_| ConfigurationError::Template { index })?,
                        ),
                    )
                    .map_err(|_| ConfigurationError::Template { index })?;
                    if runtime.fingerprint() != t.fingerprint {
                        return Err(ConfigurationError::Template { index });
                    }
                    admitted.push(
                        NamedTemplate::new(t.id, runtime)
                            .map_err(|_| ConfigurationError::Template { index })?,
                    );
                }
                let templates = Templates::new(admitted, Some(default_template))
                    .map_err(|_| ConfigurationError::Templates)?;
                let maintenance =
                    MaintenanceProfiles::new(templates.runtimes(), maintenance_transitions)
                        .map_err(|_| ConfigurationError::Maintenance)?;
                let execution = execution.prepare(&templates).await?;
                gateway
                    .config(self.provider_instance_id)
                    .map_err(|_| ConfigurationError::ProviderTrust)?
                    .validate()
                    .await
                    .map_err(|_| ConfigurationError::ProviderTrust)?;
                let config = AllocationConfig::new(
                    allocator.endpoint,
                    allocator.ca_file,
                    allocator.certificate_file,
                    allocator.key_file,
                )
                .map_err(|_| ConfigurationError::StorageTrust)?;
                let homes =
                    RetainedHomes::new(self.provider_instance_id, config, &templates.runtimes())
                        .await
                        .map_err(|_| ConfigurationError::StorageTrust)?;
                (
                    templates,
                    Some(PreparedProvider {
                        gateway: *gateway,
                        homes,
                        limits,
                        execution,
                        maintenance,
                    }),
                )
            }
        };
        Ok(PreparedConfiguration {
            listen: self.listen,
            allowed_hosts: self.allowed_hosts,
            allowed_origins,
            access: self.access,
            provider_instance_id: self.provider_instance_id,
            templates,
            provider,
        })
    }
}
