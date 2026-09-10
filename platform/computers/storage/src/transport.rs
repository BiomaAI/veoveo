//! One authenticated, bounded request per TLS connection. The worker trust root
//! is installation-owned and must be distinct from provider guest trust.
use crate::{HomeIdentity, Result, Service, StorageError};
use rustls::{
    RootCertStore, ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Semaphore,
};
use uuid::Uuid;
use veoveo_computers_runtime::storage_protocol as wire;

const SCHEMA: &str = "veoveo.io/computer-storage/v1";
const MAX_FRAME: usize = 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TlsConfig {
    pub worker_ca: PathBuf,
    pub certificate: PathBuf,
    pub private_key: PathBuf,
}
impl TlsConfig {
    pub fn load(&self) -> Result<Arc<ServerConfig>> {
        let ca = read(&self.worker_ca, false)?;
        let certificate = read(&self.certificate, false)?;
        let key = read(&self.private_key, true)?;
        let mut roots = RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(&ca) {
            roots
                .add(cert.map_err(|_| StorageError::InvalidIdentity)?)
                .map_err(|_| StorageError::InvalidIdentity)?;
        }
        if roots.is_empty() {
            return Err(StorageError::InvalidIdentity);
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots),
            provider.clone(),
        )
        .build()
        .map_err(|_| StorageError::InvalidIdentity)?;
        let certs = CertificateDer::pem_slice_iter(&certificate)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| StorageError::InvalidIdentity)?;
        let key = PrivateKeyDer::from_pem_slice(&key).map_err(|_| StorageError::InvalidIdentity)?;
        let config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|_| StorageError::InvalidIdentity)?
            .with_client_cert_verifier(verifier)
            .with_single_cert(certs, key)
            .map_err(|_| StorageError::InvalidIdentity)?;
        Ok(Arc::new(config))
    }
}
fn read(path: &std::path::Path, secret: bool) -> Result<Vec<u8>> {
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    if !path.is_absolute() {
        return Err(StorageError::InvalidIdentity);
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags((nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_NONBLOCK).bits())
        .open(path)
        .map_err(|_| StorageError::InvalidIdentity)?;
    let metadata = file.metadata().map_err(|_| StorageError::InvalidIdentity)?;
    if !metadata.is_file()
        || metadata.len() > 65536
        || metadata.mode() & 0o022 != 0
        || (secret
            && (metadata.mode() & 0o077 != 0 || metadata.uid() != nix::unistd::geteuid().as_raw()))
    {
        return Err(StorageError::InvalidIdentity);
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| StorageError::InvalidIdentity)?;
    if bytes.is_empty() || bytes.len() > 65536 {
        return Err(StorageError::InvalidIdentity);
    }
    Ok(bytes)
}

pub async fn serve(
    listener: TcpListener,
    tls: Arc<ServerConfig>,
    service: Arc<Service>,
) -> Result<()> {
    let acceptor = tokio_rustls::TlsAcceptor::from(tls);
    let slots = Arc::new(Semaphore::new(16));
    let mut requests = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            connection = listener.accept() => {
                let (tcp, _) = connection.map_err(|_| StorageError::Unavailable)?;
                let Ok(permit) = slots.clone().try_acquire_owned() else { drop(tcp); continue; };
                let acceptor = acceptor.clone();
                let service = service.clone();
                requests.spawn(async move {
                    let _permit = permit;
                    let _ = tokio::time::timeout(Duration::from_secs(180), async {
                        let mut tls = tokio::time::timeout(Duration::from_secs(5), acceptor.accept(tcp))
                            .await.map_err(|_| StorageError::Unavailable)?
                            .map_err(|_| StorageError::InvalidIdentity)?;
                        let bytes = tokio::time::timeout(Duration::from_secs(5), async {
                            let size = tls.read_u32().await.map_err(|_| StorageError::Unavailable)? as usize;
                            if size == 0 || size > MAX_FRAME { return Err(StorageError::InvalidIdentity); }
                            let mut bytes = vec![0; size];
                            tls.read_exact(&mut bytes).await.map_err(|_| StorageError::Unavailable)?;
                            Ok(bytes)
                        }).await.map_err(|_| StorageError::Unavailable)??;
                        let reply = match request(&service, &bytes).await {
                            Ok(reply) => reply,
                            Err(_) => encode(&wire::FailureReply {
                                schema: SCHEMA.parse().map_err(|_| StorageError::InvalidIdentity)?,
                                status: "error".parse().map_err(|_| StorageError::InvalidIdentity)?,
                            })?,
                        };
                        tls.write_u32(reply.len() as u32).await.map_err(|_| StorageError::Unavailable)?;
                        tls.write_all(&reply).await.map_err(|_| StorageError::Unavailable)?;
                        tls.shutdown().await.map_err(|_| StorageError::Unavailable)
                    }).await;
                });
            }
            _ = requests.join_next(), if !requests.is_empty() => {}
        }
    }
}
fn encode<T: Serialize>(reply: &T) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(reply).map_err(|_| StorageError::InvalidIdentity)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(StorageError::InvalidIdentity);
    }
    Ok(bytes)
}
enum Request {
    Ready(wire::ReadyRequest),
    Bound(wire::BoundRequest),
}
fn parse(bytes: &[u8]) -> Result<Request> {
    if bytes.is_empty()
        || bytes.len() > MAX_FRAME
        || bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{')
    {
        return Err(StorageError::InvalidIdentity);
    }
    // Both closed structs deserialize from the original bytes. An untagged
    // enum/Value intermediary would erase duplicate object keys.
    if let Ok(request) = serde_json::from_slice::<wire::ReadyRequest>(bytes) {
        return Ok(Request::Ready(request));
    }
    serde_json::from_slice::<wire::BoundRequest>(bytes)
        .map(Request::Bound)
        .map_err(|_| StorageError::InvalidIdentity)
}
fn uuid(id: &wire::IdentityId) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|_| StorageError::InvalidIdentity)
}
async fn request(service: &Service, bytes: &[u8]) -> Result<Vec<u8>> {
    match parse(bytes)? {
        Request::Ready(request) => {
            let capacity = service
                .ready(uuid(&request.provider_id)?, &request.template_fingerprint)
                .await?;
            encode(&wire::ReadyReply {
                schema: request.schema,
                operation: "ready".parse().map_err(|_| StorageError::InvalidIdentity)?,
                provider_id: request.provider_id,
                template_fingerprint: request.template_fingerprint,
                status: "ready".parse().map_err(|_| StorageError::InvalidIdentity)?,
                capacity_bytes: (capacity as i64).into(),
            })
        }
        Request::Bound(request) => {
            let home = HomeIdentity {
                provider_id: uuid(&request.provider_id)?,
                computer_id: uuid(&request.computer_id)?,
                instance_id: uuid(&request.instance_id)?,
                template_fingerprint: request.template_fingerprint.to_string(),
            };
            home.binding()?;
            let capacity = match request.operation {
                wire::BoundOperation::Prepare => service.prepare(home).await?,
                wire::BoundOperation::Restore => service.restore(home).await?,
            };
            encode(&wire::BoundReply {
                schema: request.schema,
                operation: request.operation,
                provider_id: request.provider_id,
                instance_id: request.instance_id,
                computer_id: request.computer_id,
                template_fingerprint: request.template_fingerprint,
                status: "ready".parse().map_err(|_| StorageError::InvalidIdentity)?,
                capacity_bytes: (capacity as i64).into(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_reject_duplicate_unknown_sequence_and_unbounded_frames() {
        let good = format!(
            r#"{{"schema":"{SCHEMA}","operation":"ready","providerId":"{}","templateFingerprint":"{}"}}"#,
            Uuid::from_u128(1),
            "f".repeat(64)
        );
        assert!(matches!(parse(good.as_bytes()), Ok(Request::Ready(_))));
        for field in [
            r#""operation":"ready""#,
            r#""capacityBytes":536870912"#,
            r#""providerId":null"#,
        ] {
            let bad = format!("{},{} }}", &good[..good.len() - 1], field);
            assert!(parse(bad.as_bytes()).is_err());
        }
        assert!(parse(b"[]").is_err());
        assert!(parse(&vec![b' '; 1025]).is_err());
    }
}
