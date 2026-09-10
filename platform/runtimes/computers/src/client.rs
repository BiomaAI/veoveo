use crate::{
    Binding, DevelopmentTemplate, Observation, Phase, Result, RuntimeFailure, protocol::v1 as api,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tonic::{
    Code, Request,
    transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity},
};
use uuid::Uuid;
use zeroize::Zeroizing;
pub const GATEWAY_VERSION: &str = "0.0.117-veoveo.2";
pub(crate) type Client = api::open_shell_client::OpenShellClient<Channel>;

pub struct GatewayConfig {
    provider_instance_id: Uuid,
    endpoint: String,
    workspace: String,
    ca_path: PathBuf,
    cert_path: PathBuf,
    key_path: PathBuf,
}
impl GatewayConfig {
    /// Validate referenced trust material before installation-side writes, without
    /// requiring the provider to be online. This creates no qualified runtime.
    pub async fn validate(&self) -> Result<()> {
        self.transport().await.map(|_| ())
    }
    async fn transport(&self) -> Result<Endpoint> {
        let (host, _) = endpoint_parts(&self.endpoint)?;
        let ca = read_file(&self.ca_path).await?;
        let cert = read_file(&self.cert_path).await?;
        let key = read_file(&self.key_path).await?;
        let tls = ClientTlsConfig::new()
            .domain_name(host)
            .ca_certificate(Certificate::from_pem(&*ca))
            .identity(Identity::from_pem(&*cert, &*key));
        Endpoint::from_shared(format!("https://{}", self.endpoint))
            .map_err(|_| RuntimeFailure::InvalidConfiguration)?
            .connect_timeout(Duration::from_secs(10))
            .tls_config(tls)
            .map_err(|_| RuntimeFailure::InvalidConfiguration)
    }
    pub fn new(
        provider_instance_id: Uuid,
        endpoint: String,
        workspace: String,
        ca_path: PathBuf,
        cert_path: PathBuf,
        key_path: PathBuf,
    ) -> Result<Self> {
        if provider_instance_id.is_nil() {
            return Err(RuntimeFailure::InvalidConfiguration);
        }
        endpoint_parts(&endpoint)?;
        if workspace.is_empty()
            || workspace.len() > 19
            || workspace.split('-').any(|p| {
                p.is_empty()
                    || !p
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
        {
            return Err(RuntimeFailure::InvalidConfiguration);
        }
        for p in [&ca_path, &cert_path, &key_path] {
            validate_path(p)?;
        }
        Ok(Self {
            provider_instance_id,
            endpoint,
            workspace,
            ca_path,
            cert_path,
            key_path,
        })
    }
}
pub(crate) fn validate_path(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(RuntimeFailure::InvalidConfiguration);
    }
    Ok(())
}
pub(crate) fn endpoint_parts(endpoint: &str) -> Result<(&str, u16)> {
    let (host, port) = endpoint
        .rsplit_once(':')
        .ok_or(RuntimeFailure::InvalidConfiguration)?;
    if host.is_empty()
        || host.len() > 253
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
        || port.is_empty()
        || !port.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(RuntimeFailure::InvalidConfiguration);
    }
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or(RuntimeFailure::InvalidConfiguration)?;
    Ok((host, port))
}
pub(crate) async fn read_file(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    let size = tokio::fs::metadata(path)
        .await
        .map_err(|_| RuntimeFailure::InvalidConfiguration)?
        .len();
    if size == 0 || size > 1024 * 1024 {
        return Err(RuntimeFailure::InvalidConfiguration);
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| RuntimeFailure::InvalidConfiguration)?;
    if bytes.len() > 1024 * 1024 {
        return Err(RuntimeFailure::InvalidConfiguration);
    }
    Ok(Zeroizing::new(bytes))
}
pub(crate) fn request<T>(message: T, seconds: u64) -> Request<T> {
    let mut request = Request::new(message);
    request.set_timeout(Duration::from_secs(seconds));
    request
}

#[derive(Clone)]
pub struct OpenShellRuntime {
    pub(crate) provider_instance_id: Uuid,
    pub(crate) client: Client,
    pub(crate) workspace: String,
    pub(crate) endpoint: Endpoint,
    pub(crate) address: String,
}
impl OpenShellRuntime {
    pub fn provider_instance_id(&self) -> Uuid {
        self.provider_instance_id
    }
    /// Establish an installation-owned mTLS connection and admit the exact pin.
    pub async fn connect(config: GatewayConfig) -> Result<Self> {
        let endpoint = config.transport().await?;
        let channel = endpoint
            .connect()
            .await
            .map_err(|_| RuntimeFailure::Unavailable)?;
        let runtime = Self {
            provider_instance_id: config.provider_instance_id,
            client: Client::new(channel)
                .max_decoding_message_size(1024 * 1024)
                .max_encoding_message_size(1024 * 1024),
            workspace: config.workspace,
            endpoint,
            address: config.endpoint,
        };
        runtime.ready().await?;
        Ok(runtime)
    }
    pub async fn ready(&self) -> Result<()> {
        let response = self
            .client
            .clone()
            .get_gateway_info(request(api::GetGatewayInfoRequest {}, 10))
            .await
            .map_err(|_| RuntimeFailure::Unavailable)?
            .into_inner();
        if response.gateway_version != GATEWAY_VERSION
            || response.compute_drivers.len() != 1
            || response.compute_drivers[0].name != "docker"
        {
            return Err(RuntimeFailure::VersionMismatch);
        }
        let workspace = self
            .client
            .clone()
            .get_workspace(request(
                api::GetWorkspaceRequest {
                    name: self.workspace.clone(),
                },
                5,
            ))
            .await
            .map_err(|_| RuntimeFailure::InvalidConfiguration)?
            .into_inner()
            .workspace
            .ok_or(RuntimeFailure::InvalidConfiguration)?;
        if workspace.metadata.as_ref().map(|meta| meta.name.as_str())
            != Some(self.workspace.as_str())
            || workspace.status.as_ref().map(|status| status.phase)
                != Some(crate::protocol::datamodel::v1::WorkspacePhase::Active as i32)
        {
            return Err(RuntimeFailure::InvalidConfiguration);
        }
        Ok(())
    }
    fn observation(
        &self,
        response: api::SandboxResponse,
        binding: &Binding,
    ) -> Result<Observation> {
        Observation::checked(
            response.sandbox.ok_or(RuntimeFailure::BindingMismatch)?,
            binding,
            &self.workspace,
        )
    }
    pub async fn get(&self, binding: &Binding) -> Result<Option<Observation>> {
        match self
            .client
            .clone()
            .get_sandbox(request(
                api::GetSandboxRequest {
                    name: binding.name(),
                    workspace: self.workspace.clone(),
                },
                15,
            ))
            .await
        {
            Ok(response) => self.observation(response.into_inner(), binding).map(Some),
            Err(status) if status.code() == Code::NotFound => Ok(None),
            Err(_) => Err(RuntimeFailure::Unavailable),
        }
    }
    pub async fn create(
        &self,
        binding: &Binding,
        template: &DevelopmentTemplate,
    ) -> Result<Observation> {
        if template.fingerprint() != binding.template_fingerprint() {
            return Err(RuntimeFailure::BindingMismatch);
        }
        // A new container cannot carry the previous container's writable layer.
        // Replacement is supported only with the stable, separately retained home.
        if binding.replacement_instance_id().is_some() && template.persistent_home().is_none() {
            return Err(RuntimeFailure::InvalidTemplate);
        }
        if let Some(existing) = self.get(binding).await? {
            return Ok(existing);
        }
        match self
            .client
            .clone()
            .create_sandbox(request(
                api::CreateSandboxRequest {
                    name: binding.name(),
                    workspace: self.workspace.clone(),
                    labels: binding.labels(),
                    spec: Some(template.bound_spec(binding)?),
                    ..Default::default()
                },
                30,
            ))
            .await
        {
            Ok(response) => self.observation(response.into_inner(), binding),
            Err(status) if status.code() == Code::AlreadyExists => self
                .get(binding)
                .await?
                .ok_or(RuntimeFailure::LifecycleUnknown),
            Err(_) => Err(RuntimeFailure::LifecycleUnknown),
        }
    }
    pub async fn start(&self, binding: &Binding, before: &Observation) -> Result<Observation> {
        check_source(before, Phase::Stopped)?;
        let current = self.get(binding).await?.ok_or(RuntimeFailure::NotFound)?;
        if current.sandbox_id != before.sandbox_id {
            return Err(RuntimeFailure::BindingMismatch);
        }
        if matches!(
            current.phase,
            Phase::Ready | Phase::Starting | Phase::Provisioning
        ) {
            return Ok(current);
        }
        if current.phase != Phase::Stopped {
            return Err(RuntimeFailure::InvalidState);
        }
        if current.main_process_instance_id != before.main_process_instance_id {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let response = self
            .client
            .clone()
            .start_sandbox(request(
                api::StartSandboxRequest {
                    name: binding.name(),
                    workspace: self.workspace.clone(),
                },
                30,
            ))
            .await
            .map_err(|_| RuntimeFailure::LifecycleUnknown)?;
        let observed = self.observation(response.into_inner(), binding)?;
        if observed.sandbox_id != current.sandbox_id {
            return Err(RuntimeFailure::BindingMismatch);
        }
        Ok(observed)
    }
    pub async fn stop(&self, binding: &Binding, before: &Observation) -> Result<Observation> {
        check_source(before, Phase::Ready)?;
        let current = self.get(binding).await?.ok_or(RuntimeFailure::NotFound)?;
        if current.sandbox_id != before.sandbox_id
            || current.main_process_instance_id != before.main_process_instance_id
        {
            return Err(RuntimeFailure::BindingMismatch);
        }
        if matches!(current.phase, Phase::Stopped | Phase::Stopping) {
            return Ok(current);
        }
        if current.phase != Phase::Ready {
            return Err(RuntimeFailure::InvalidState);
        }
        let response = self
            .client
            .clone()
            .stop_sandbox(request(
                api::StopSandboxRequest {
                    name: binding.name(),
                    workspace: self.workspace.clone(),
                },
                30,
            ))
            .await
            .map_err(|_| RuntimeFailure::LifecycleUnknown)?;
        let observed = self.observation(response.into_inner(), binding)?;
        if observed.sandbox_id != current.sandbox_id {
            return Err(RuntimeFailure::BindingMismatch);
        }
        Ok(observed)
    }
}

fn check_source(before: &Observation, phase: Phase) -> Result<()> {
    if before.phase != phase
        || !crate::models::identifier(&before.sandbox_id)
        || !crate::models::identifier(&before.main_process_instance_id)
    {
        return Err(RuntimeFailure::BindingMismatch);
    }
    Ok(())
}
