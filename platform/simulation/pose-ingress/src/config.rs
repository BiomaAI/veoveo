use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use rustls::{
    RootCertStore, ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    server::WebPkiClientVerifier,
};

use crate::state::PoseIngressConfig;

#[derive(Parser)]
#[command(
    name = "simulation-view-pose",
    about = "Mutually authenticated Simulation View latest-pose ingress"
)]
pub(crate) struct Args {
    #[arg(long, default_value_t = 8811)]
    pub http_port: u16,
    #[arg(long, default_value_t = 7443)]
    pub tls_port: u16,
    #[arg(
        long,
        env = "SIMULATION_VIEW_POSE_DIRECTORY",
        default_value = "/dev/shm/veoveo/simulation-view"
    )]
    pub pose_directory: PathBuf,
    #[arg(
        long,
        env = "SIMULATION_VIEW_POSE_CONTROL_TOKEN",
        hide_env_values = true
    )]
    pub control_token: String,
    #[arg(long, env = "SIMULATION_VIEW_POSE_TLS_CERT_DER")]
    pub tls_certificate_der: PathBuf,
    #[arg(long, env = "SIMULATION_VIEW_POSE_TLS_KEY_DER", hide_env_values = true)]
    pub tls_private_key_der: PathBuf,
    #[arg(long, env = "SIMULATION_VIEW_POSE_CLIENT_CA_DER")]
    pub client_ca_der: PathBuf,
    #[arg(long, default_value_t = 64)]
    pub maximum_sessions: usize,
    #[arg(long, default_value_t = 64)]
    pub maximum_connections: usize,
}

impl Args {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.http_port > 0 && self.tls_port > 0 && self.http_port != self.tls_port,
            "HTTP and TLS ports must be positive and distinct"
        );
        anyhow::ensure!(
            self.pose_directory.is_absolute()
                && self.pose_directory != Path::new("/")
                && self.pose_directory.components().count() >= 3,
            "pose directory must be a narrow absolute path"
        );
        anyhow::ensure!(
            self.control_token.len() >= 32
                && self.control_token.len() <= 512
                && !self.control_token.chars().any(char::is_whitespace),
            "pose control token must contain 32 to 512 non-whitespace characters"
        );
        anyhow::ensure!(
            self.maximum_sessions > 0 && self.maximum_connections > 0,
            "pose ingress limits must be positive"
        );
        Ok(())
    }

    pub fn http_address(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.http_port))
    }

    pub fn tls_address(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.tls_port))
    }

    pub fn state_config(&self) -> PoseIngressConfig {
        PoseIngressConfig::new(
            self.pose_directory.clone(),
            &self.control_token,
            self.maximum_sessions,
        )
    }

    pub fn tls_config(&self) -> anyhow::Result<Arc<ServerConfig>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let server_certificate = std::fs::read(&self.tls_certificate_der)?;
        let server_key = std::fs::read(&self.tls_private_key_der)?;
        let client_ca = std::fs::read(&self.client_ca_der)?;
        anyhow::ensure!(
            !server_certificate.is_empty() && !server_key.is_empty() && !client_ca.is_empty(),
            "TLS DER inputs must not be empty"
        );
        let mut roots = RootCertStore::empty();
        roots.add(CertificateDer::from(client_ca))?;
        let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
        let config = ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_client_cert_verifier(verifier)
            .with_single_cert(
                vec![CertificateDer::from(server_certificate)],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(server_key)),
            )?;
        Ok(Arc::new(config))
    }
}
