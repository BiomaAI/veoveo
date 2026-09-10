use crate::{
    Binding, Result, RuntimeFailure,
    client::{endpoint_parts, read_file, validate_path},
    models::valid_fingerprint,
};
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use uuid::Uuid;
const SCHEMA: &str = "veoveo.io/computer-storage/v1";
const MAXIMUM_FRAME: usize = 1024;

fn storage_identity(id: Uuid) -> Result<wire::IdentityId> {
    id.to_string()
        .parse()
        .map_err(|_| RuntimeFailure::BindingMismatch)
}

// Generated from the Veoveo-owned IDL; these are private installation wire types.
// No credential, TLS configuration, or provider token is part of this schema.
#[allow(dead_code)]
pub mod wire {
    include!(concat!(env!("OUT_DIR"), "/allocation.rs"));
}
pub struct AllocationConfig {
    endpoint: String,
    ca_path: PathBuf,
    cert_path: PathBuf,
    key_path: PathBuf,
}
impl AllocationConfig {
    pub fn new(
        endpoint: String,
        ca_path: PathBuf,
        cert_path: PathBuf,
        key_path: PathBuf,
    ) -> Result<Self> {
        endpoint_parts(&endpoint)?;
        for p in [&ca_path, &cert_path, &key_path] {
            validate_path(p)?;
        }
        Ok(Self {
            endpoint,
            ca_path,
            cert_path,
            key_path,
        })
    }
}
#[derive(Clone)]
pub struct HomeAllocator {
    endpoint: String,
    tls: Arc<ClientConfig>,
    fingerprint: String,
    capacity_bytes: u64,
    provider_id: Uuid,
}
impl HomeAllocator {
    pub async fn new(
        config: AllocationConfig,
        provider_id: Uuid,
        fingerprint: String,
        capacity_bytes: u64,
    ) -> Result<Self> {
        if provider_id.is_nil()
            || !valid_fingerprint(&fingerprint)
            || !(512 * 1024 * 1024..=256 * 1024 * 1024 * 1024).contains(&capacity_bytes)
        {
            return Err(RuntimeFailure::InvalidConfiguration);
        }
        let ca = read_file(&config.ca_path).await?;
        let cert = read_file(&config.cert_path).await?;
        let key = read_file(&config.key_path).await?;
        let mut roots = RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(&ca) {
            roots
                .add(cert.map_err(|_| RuntimeFailure::InvalidConfiguration)?)
                .map_err(|_| RuntimeFailure::InvalidConfiguration)?;
        }
        if roots.is_empty() {
            return Err(RuntimeFailure::InvalidConfiguration);
        }
        let certs = CertificateDer::pem_slice_iter(&cert)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| RuntimeFailure::InvalidConfiguration)?;
        let key = PrivateKeyDer::from_pem_slice(&key)
            .map_err(|_| RuntimeFailure::InvalidConfiguration)?;
        let tls =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|_| RuntimeFailure::InvalidConfiguration)?
                .with_root_certificates(roots)
                .with_client_auth_cert(certs, key)
                .map_err(|_| RuntimeFailure::InvalidConfiguration)?;
        Ok(Self {
            endpoint: config.endpoint,
            tls: Arc::new(tls),
            fingerprint,
            capacity_bytes,
            provider_id,
        })
    }
    pub async fn ready(&self) -> Result<()> {
        let request = wire::ReadyRequest {
            provider_id: storage_identity(self.provider_id)?,
            schema: SCHEMA
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            operation: "ready"
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            template_fingerprint: self
                .fingerprint
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
        };
        let expected = wire::ReadyReply {
            provider_id: request.provider_id.clone(),
            schema: request.schema,
            operation: "ready"
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            status: "ready"
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            template_fingerprint: request.template_fingerprint.clone(),
            capacity_bytes: i64::try_from(self.capacity_bytes)
                .map_err(|_| RuntimeFailure::AllocationFailed)?
                .into(),
        };
        self.call(&request, &expected, 10).await
    }
    pub async fn prepare(&self, binding: &Binding) -> Result<()> {
        self.bound(wire::BoundOperation::Prepare, binding).await
    }
    pub async fn restore(&self, binding: &Binding) -> Result<()> {
        self.bound(wire::BoundOperation::Restore, binding).await
    }
    pub async fn handoff(
        &self,
        operation_id: Uuid,
        source: &Binding,
        target: &Binding,
        source_resource_id: &str,
    ) -> Result<()> {
        if source.computer_id() != target.computer_id()
            || target.template_fingerprint() != self.fingerprint
            || source.replacement_instance_id() == target.replacement_instance_id()
            || target.replacement_instance_id().is_none()
        {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let request = wire::HandoffRequest {
            schema: SCHEMA
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            operation: "handoff"
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            provider_id: storage_identity(self.provider_id)?,
            computer_id: storage_identity(source.computer_id())?,
            operation_id: storage_identity(operation_id)?,
            source_instance_id: storage_identity(
                source
                    .replacement_instance_id()
                    .unwrap_or(source.computer_id()),
            )?,
            source_template_fingerprint: source
                .template_fingerprint()
                .parse()
                .map_err(|_| RuntimeFailure::BindingMismatch)?,
            source_resource_id: source_resource_id
                .parse()
                .map_err(|_| RuntimeFailure::BindingMismatch)?,
            target_instance_id: storage_identity(
                target
                    .replacement_instance_id()
                    .ok_or(RuntimeFailure::BindingMismatch)?,
            )?,
            target_template_fingerprint: self
                .fingerprint
                .parse()
                .map_err(|_| RuntimeFailure::BindingMismatch)?,
        };
        let expected = wire::HandoffReply {
            schema: request.schema,
            operation: "handoff"
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            provider_id: request.provider_id.clone(),
            computer_id: request.computer_id.clone(),
            operation_id: request.operation_id.clone(),
            source_instance_id: request.source_instance_id.clone(),
            source_template_fingerprint: request.source_template_fingerprint.clone(),
            source_resource_id: request.source_resource_id.clone(),
            target_instance_id: request.target_instance_id.clone(),
            target_template_fingerprint: request.target_template_fingerprint.clone(),
            status: "ready"
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            capacity_bytes: (self.capacity_bytes as i64).into(),
        };
        self.call(&request, &expected, 180).await
    }
    async fn bound(&self, operation: wire::BoundOperation, binding: &Binding) -> Result<()> {
        if binding.template_fingerprint() != self.fingerprint {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let request = wire::BoundRequest {
            provider_id: storage_identity(self.provider_id)?,
            instance_id: storage_identity(
                binding
                    .replacement_instance_id()
                    .unwrap_or(binding.computer_id()),
            )?,
            schema: SCHEMA
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            operation,
            template_fingerprint: self
                .fingerprint
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            computer_id: binding
                .computer_id()
                .to_string()
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
        };
        let expected = wire::BoundReply {
            provider_id: request.provider_id.clone(),
            instance_id: request.instance_id.clone(),
            schema: request.schema,
            operation: request.operation,
            status: "ready"
                .parse()
                .map_err(|_| RuntimeFailure::AllocationFailed)?,
            template_fingerprint: request.template_fingerprint.clone(),
            computer_id: request.computer_id.clone(),
            capacity_bytes: i64::try_from(self.capacity_bytes)
                .map_err(|_| RuntimeFailure::AllocationFailed)?
                .into(),
        };
        self.call(&request, &expected, 180).await
    }
    async fn call<Request: Serialize, Reply: DeserializeOwned + PartialEq>(
        &self,
        request: &Request,
        expected: &Reply,
        seconds: u64,
    ) -> Result<()> {
        let raw = serde_json::to_vec(&request).map_err(|_| RuntimeFailure::AllocationFailed)?;
        if raw.is_empty() || raw.len() > MAXIMUM_FRAME {
            return Err(RuntimeFailure::AllocationFailed);
        }
        tokio::time::timeout(Duration::from_secs(seconds), async {
            let (host, port) = endpoint_parts(&self.endpoint)?;
            let tcp = TcpStream::connect((host, port))
                .await
                .map_err(|_| RuntimeFailure::AllocationFailed)?;
            let name = ServerName::try_from(host.to_owned())
                .map_err(|_| RuntimeFailure::AllocationFailed)?;
            let mut tls = tokio::time::timeout(
                Duration::from_secs(5),
                tokio_rustls::TlsConnector::from(self.tls.clone()).connect(name, tcp),
            )
            .await
            .map_err(|_| RuntimeFailure::AllocationFailed)?
            .map_err(|_| RuntimeFailure::AllocationFailed)?;
            tls.write_u32(raw.len() as u32)
                .await
                .map_err(|_| RuntimeFailure::AllocationFailed)?;
            tls.write_all(&raw)
                .await
                .map_err(|_| RuntimeFailure::AllocationFailed)?;
            tls.flush()
                .await
                .map_err(|_| RuntimeFailure::AllocationFailed)?;
            let size = tls
                .read_u32()
                .await
                .map_err(|_| RuntimeFailure::AllocationFailed)? as usize;
            if size == 0 || size > MAXIMUM_FRAME {
                return Err(RuntimeFailure::AllocationFailed);
            }
            let mut bytes = vec![0; size];
            tls.read_exact(&mut bytes)
                .await
                .map_err(|_| RuntimeFailure::AllocationFailed)?;
            validate_reply(&bytes, expected)?;
            // Dropping the authenticated socket has no allocation side effect.
            Ok(())
        })
        .await
        .map_err(|_| RuntimeFailure::AllocationFailed)?
    }
}
pub(super) fn validate_reply<Reply: DeserializeOwned + PartialEq>(
    bytes: &[u8],
    expected: &Reply,
) -> Result<()> {
    // Deserialize the concrete generated reply directly from the original bytes,
    // never through Value (which would discard duplicate keys). Serde can also
    // deserialize a struct from a sequence, but this protocol permits objects only.
    if bytes.is_empty()
        || bytes.len() > MAXIMUM_FRAME
        || bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{')
    {
        return Err(RuntimeFailure::AllocationFailed);
    }
    let reply: Reply =
        serde_json::from_slice(bytes).map_err(|_| RuntimeFailure::AllocationFailed)?;
    // The schema cannot express this call's admitted identity or capacity. Typify
    // also does not enforce all numeric bounds; exact typed equality is required.
    if &reply != expected {
        return Err(RuntimeFailure::AllocationFailed);
    }
    Ok(())
}
