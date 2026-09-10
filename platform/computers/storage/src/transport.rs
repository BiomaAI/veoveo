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

#[derive(Deserialize, Serialize)]
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
    Handoff(wire::HandoffRequest),
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
    if let Ok(request) = serde_json::from_slice::<wire::BoundRequest>(bytes) {
        return Ok(Request::Bound(request));
    }
    serde_json::from_slice::<wire::HandoffRequest>(bytes)
        .map(Request::Handoff)
        .map_err(|_| StorageError::InvalidIdentity)
}
fn uuid(id: &wire::IdentityId) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|_| StorageError::InvalidIdentity)
}
async fn request(service: &Service, bytes: &[u8]) -> Result<Vec<u8>> {
    match parse(bytes)? {
        Request::Handoff(request) => {
            let provider_id = uuid(&request.provider_id)?;
            let computer_id = uuid(&request.computer_id)?;
            let capacity = service
                .handoff(crate::Handoff {
                    operation_id: uuid(&request.operation_id)?,
                    source_resource_id: request.source_resource_id.to_string(),
                    source: HomeIdentity {
                        provider_id,
                        computer_id,
                        instance_id: uuid(&request.source_instance_id)?,
                        template_fingerprint: request.source_template_fingerprint.to_string(),
                    },
                    target: HomeIdentity {
                        provider_id,
                        computer_id,
                        instance_id: uuid(&request.target_instance_id)?,
                        template_fingerprint: request.target_template_fingerprint.to_string(),
                    },
                })
                .await?;
            encode(&wire::HandoffReply {
                schema: request.schema,
                operation: "handoff"
                    .parse()
                    .map_err(|_| StorageError::InvalidIdentity)?,
                provider_id: request.provider_id,
                computer_id: request.computer_id,
                operation_id: request.operation_id,
                source_instance_id: request.source_instance_id,
                source_template_fingerprint: request.source_template_fingerprint,
                source_resource_id: request.source_resource_id,
                target_instance_id: request.target_instance_id,
                target_template_fingerprint: request.target_template_fingerprint,
                status: "ready".parse().map_err(|_| StorageError::InvalidIdentity)?,
                capacity_bytes: (capacity as i64).into(),
            })
        }
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
    fn handoff_identity_is_closed_and_fits_the_frame_at_the_resource_bound() {
        let value = serde_json::json!({
            "schema": SCHEMA, "operation": "handoff", "providerId": Uuid::from_u128(100),
            "computerId": Uuid::from_u128(200), "operationId": Uuid::from_u128(300),
            "sourceInstanceId": Uuid::from_u128(200), "sourceTemplateFingerprint": "a".repeat(64),
            "sourceResourceId": "r".repeat(128), "targetInstanceId": Uuid::from_u128(400),
            "targetTemplateFingerprint": "b".repeat(64),
        });
        let raw = serde_json::to_vec(&value).unwrap();
        assert!(matches!(parse(&raw), Ok(Request::Handoff(_))));
        for field in [
            "sourceInstanceId",
            "targetInstanceId",
            "operationId",
            "sourceResourceId",
        ] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(parse(&serde_json::to_vec(&missing).unwrap()).is_err());
            let mut duplicate = raw.clone();
            duplicate.pop();
            duplicate.extend(format!(",\"{field}\":{} }}", value[field]).bytes());
            assert!(parse(&duplicate).is_err());
        }
        let mut oversized_resource = value.clone();
        oversized_resource["sourceResourceId"] = "r".repeat(129).into();
        assert!(parse(&serde_json::to_vec(&oversized_resource).unwrap()).is_err());
        let mut reply = value;
        reply["status"] = "ready".into();
        reply["capacityBytes"] = 274877906944u64.into();
        let reply: wire::HandoffReply = serde_json::from_value(reply).unwrap();
        assert!(encode(&reply).unwrap().len() <= MAX_FRAME);
    }
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
