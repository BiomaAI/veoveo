use std::{net::SocketAddr, sync::Arc};

use rustls::ServerConfig;
use tokio::{
    io::AsyncReadExt,
    net::TcpStream,
    sync::{OwnedSemaphorePermit, Semaphore},
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use tokio_util::sync::CancellationToken;
use veoveo_simulation_pose::{PoseLimits, PoseStreamDecoder};
use x509_parser::{extensions::GeneralName, parse_x509_certificate};

use crate::state::PoseIngress;

pub(crate) async fn serve(
    address: SocketAddr,
    ingress: Arc<PoseIngress>,
    tls_config: Arc<ServerConfig>,
    maximum_connections: usize,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    let acceptor = TlsAcceptor::from(tls_config);
    let connections = Arc::new(Semaphore::new(maximum_connections));
    ingress.mark_tls_listening();
    tracing::info!(%address, "Simulation View mTLS pose ingress listening");
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            accepted = listener.accept() => {
                let (socket, peer) = accepted?;
                let Ok(permit) = connections.clone().try_acquire_owned() else {
                    tracing::warn!(%peer, "pose connection rejected at capacity");
                    continue;
                };
                let ingress = ingress.clone();
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    if let Err(error) = connection(acceptor, ingress, socket, permit).await {
                        tracing::warn!(%peer, %error, "pose producer connection closed");
                    }
                });
            }
        }
    }
}

async fn connection(
    acceptor: TlsAcceptor,
    ingress: Arc<PoseIngress>,
    socket: TcpStream,
    _permit: OwnedSemaphorePermit,
) -> anyhow::Result<()> {
    let mut stream = acceptor.accept(socket).await?;
    let producer_spiffe_id = peer_spiffe_id(&stream)?;
    let mut decoder = PoseStreamDecoder::new(PoseLimits::default());
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Ok(());
        }
        for snapshot in decoder.push(&buffer[..read])? {
            ingress.publish(&producer_spiffe_id, snapshot).await?;
        }
    }
}

fn peer_spiffe_id(stream: &TlsStream<TcpStream>) -> anyhow::Result<String> {
    let certificates = stream
        .get_ref()
        .1
        .peer_certificates()
        .ok_or_else(|| anyhow::anyhow!("mTLS client certificate is missing"))?;
    let leaf = certificates
        .first()
        .ok_or_else(|| anyhow::anyhow!("mTLS client certificate chain is empty"))?;
    let (_, certificate) = parse_x509_certificate(leaf.as_ref())
        .map_err(|_| anyhow::anyhow!("mTLS client certificate is invalid"))?;
    let names = certificate
        .subject_alternative_name()
        .map_err(|_| anyhow::anyhow!("mTLS client SAN is invalid"))?
        .ok_or_else(|| anyhow::anyhow!("mTLS client URI SAN is missing"))?;
    let mut spiffe_ids = names
        .value
        .general_names
        .iter()
        .filter_map(|name| match name {
            GeneralName::URI(uri) if uri.starts_with("spiffe://") => Some((*uri).to_owned()),
            _ => None,
        });
    let identity = spiffe_ids
        .next()
        .ok_or_else(|| anyhow::anyhow!("mTLS client SPIFFE URI SAN is missing"))?;
    anyhow::ensure!(
        spiffe_ids.next().is_none(),
        "mTLS client certificate has multiple SPIFFE URI SANs"
    );
    Ok(identity)
}
