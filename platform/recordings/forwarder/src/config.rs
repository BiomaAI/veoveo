use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Result, ensure};
use clap::{Parser, ValueEnum};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ClientAssertionAlgorithm {
    Rs256,
    Es256,
    EdDsa,
}

#[derive(Debug, Clone, Parser)]
#[command(name = "recording-forwarder")]
pub struct ForwarderConfig {
    /// Loopback address for native Rerun gRPC producers.
    #[arg(long, default_value = "127.0.0.1:9876")]
    pub bind: SocketAddr,

    /// Canonical gateway origin, such as <https://gateway.example/>.
    #[arg(long, env = "VEOVEO_GATEWAY_URL")]
    pub gateway_url: Url,

    /// Optional physical gateway origin used inside the installation network.
    ///
    /// Discovery, OAuth audiences, protected-resource identity, and Host headers
    /// continue to use `gateway_url`.
    #[arg(long, env = "VEOVEO_RECORDING_GATEWAY_TRANSPORT_URL")]
    pub gateway_transport_url: Option<Url>,

    /// Expected OAuth protected-resource URI from ingest discovery.
    #[arg(long, env = "VEOVEO_RECORDING_INGEST_RESOURCE")]
    pub protected_resource: Url,

    #[arg(long, env = "VEOVEO_RECORDING_PRODUCER_CLIENT_ID")]
    pub client_id: String,

    /// PEM private key used only to sign private_key_jwt assertions.
    #[arg(long, env = "VEOVEO_RECORDING_PRODUCER_PRIVATE_KEY_PEM_FILE")]
    pub private_key_pem_file: PathBuf,

    #[arg(long, env = "VEOVEO_RECORDING_PRODUCER_KEY_ID")]
    pub key_id: String,

    #[arg(
        long,
        env = "VEOVEO_RECORDING_PRODUCER_SIGNING_ALGORITHM",
        default_value = "rs256"
    )]
    pub signing_algorithm: ClientAssertionAlgorithm,

    #[arg(long, default_value = "/var/lib/veoveo-recording-forwarder")]
    pub queue_dir: PathBuf,

    #[arg(long, default_value_t = 1_073_741_824)]
    pub maximum_queue_bytes: u64,

    #[arg(long, default_value_t = 4_096)]
    pub batch_message_limit: usize,

    /// Maximum source-generation span retained in one durable batch.
    ///
    /// Rerun gRPC carries chunks without an SDK-flush marker. Chunk IDs retain
    /// their monotonic source generation time, which gives the forwarder a
    /// reactive boundary without adding a wall-clock flush task.
    #[arg(long, default_value_t = 750)]
    pub maximum_batch_source_span_ms: u64,

    #[arg(long, default_value_t = 64 * 1024 * 1024)]
    pub grpc_memory_limit_bytes: u64,

    #[arg(long, default_value_t = 30)]
    pub shutdown_drain_seconds: u64,

    /// Maximum lifetime of one gateway discovery, OAuth, or ingest request.
    ///
    /// The durable queue survives a deferred upload. A request without a
    /// deadline would instead wedge the only uploader and its shutdown path.
    #[arg(long, default_value_t = 15)]
    pub request_timeout_seconds: u64,

    /// Finish durable recordings for the same Rerun application when a new
    /// recording store arrives. This is intended for one-recording producer
    /// slots whose recording identity changes with a workload generation.
    #[arg(long, default_value_t = false)]
    pub finish_superseded_recordings: bool,
}

impl ForwarderConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.bind.ip().is_loopback(),
            "Rerun gRPC bind must be loopback"
        );
        ensure!(
            self.gateway_url.scheme() == "https"
                || (self.gateway_url.scheme() == "http"
                    && self.gateway_url.host_str().is_some_and(is_loopback_host)),
            "gateway URL must use HTTPS or explicit loopback HTTP"
        );
        ensure!(
            self.gateway_url.path() == "/"
                && self.gateway_url.query().is_none()
                && self.gateway_url.fragment().is_none(),
            "gateway URL must be an origin without a path, query, or fragment"
        );
        if let Some(transport) = &self.gateway_transport_url {
            ensure!(
                matches!(transport.scheme(), "http" | "https")
                    && transport.host_str().is_some()
                    && transport.path() == "/"
                    && transport.query().is_none()
                    && transport.fragment().is_none()
                    && transport.username().is_empty()
                    && transport.password().is_none(),
                "gateway transport URL must be an HTTP(S) origin without credentials, a path, query, or fragment"
            );
        }
        ensure!(
            (self.protected_resource.scheme() == "https"
                || (self.protected_resource.scheme() == "http"
                    && self
                        .protected_resource
                        .host_str()
                        .is_some_and(is_loopback_host)))
                && self.protected_resource.query().is_none()
                && self.protected_resource.fragment().is_none(),
            "recording ingest protected resource must use HTTPS or loopback HTTP without a query or fragment"
        );
        ensure!(
            self.protected_resource.origin() == self.gateway_url.origin(),
            "recording ingest protected resource must use the canonical gateway origin"
        );
        ensure!(
            !self.client_id.trim().is_empty() && !self.key_id.trim().is_empty(),
            "OAuth client and key IDs must not be empty"
        );
        ensure!(
            self.queue_dir.is_absolute(),
            "queue directory must be absolute"
        );
        ensure!(
            self.private_key_pem_file.is_absolute(),
            "producer private-key path must be absolute"
        );
        ensure!(
            self.maximum_queue_bytes > 0,
            "maximum queue bytes must be positive"
        );
        ensure!(
            self.batch_message_limit > 0 && self.batch_message_limit <= 65_536,
            "batch message limit must be in 1..=65536"
        );
        ensure!(
            self.maximum_batch_source_span_ms > 0 && self.maximum_batch_source_span_ms <= 10_000,
            "maximum batch source span must be in 1..=10000 milliseconds"
        );
        ensure!(
            self.grpc_memory_limit_bytes > 0,
            "gRPC memory limit must be positive"
        );
        ensure!(
            self.shutdown_drain_seconds > 0,
            "shutdown drain window must be positive"
        );
        ensure!(
            self.request_timeout_seconds > 0 && self.request_timeout_seconds <= 300,
            "gateway request timeout must be in 1..=300 seconds"
        );
        Ok(())
    }

    pub fn gateway_transport_url(&self) -> &Url {
        self.gateway_transport_url
            .as_ref()
            .unwrap_or(&self.gateway_url)
    }

    pub fn shutdown_drain_window(&self) -> Duration {
        Duration::from_secs(self.shutdown_drain_seconds)
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_seconds)
    }

    pub fn maximum_batch_source_span(&self) -> Duration {
        Duration::from_millis(self.maximum_batch_source_span_ms)
    }
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ForwarderConfig {
        ForwarderConfig {
            bind: "127.0.0.1:9876".parse().unwrap(),
            gateway_url: Url::parse("https://veoveo.example/").unwrap(),
            gateway_transport_url: None,
            protected_resource: Url::parse("https://veoveo.example/ingest/recordings").unwrap(),
            client_id: "sensor-a".to_owned(),
            private_key_pem_file: PathBuf::from("/run/secrets/sensor-key.pem"),
            key_id: "sensor-a-2026".to_owned(),
            signing_algorithm: ClientAssertionAlgorithm::Rs256,
            queue_dir: PathBuf::from("/var/lib/forwarder"),
            maximum_queue_bytes: 1024,
            batch_message_limit: 10,
            maximum_batch_source_span_ms: 750,
            grpc_memory_limit_bytes: 1024,
            shutdown_drain_seconds: 10,
            request_timeout_seconds: 15,
            finish_superseded_recordings: false,
        }
    }

    #[test]
    fn rejects_non_loopback_rerun_receiver() {
        let mut config = config();
        assert!(config.validate().is_ok());
        config.bind = "0.0.0.0:9876".parse().unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_an_explicit_cluster_transport_without_changing_public_identity() {
        let mut config = config();
        config.gateway_transport_url = Some(Url::parse("http://mcp-gateway:8788/").unwrap());
        assert!(config.validate().is_ok());

        config.protected_resource = Url::parse("https://other.example/ingest/recordings").unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn bounds_gateway_request_deadline() {
        let mut config = config();
        assert_eq!(config.request_timeout(), Duration::from_secs(15));
        config.request_timeout_seconds = 0;
        assert!(config.validate().is_err());
        config.request_timeout_seconds = 301;
        assert!(config.validate().is_err());
    }

    #[test]
    fn bounds_reactive_source_span() {
        let mut config = config();
        assert_eq!(
            config.maximum_batch_source_span(),
            Duration::from_millis(750)
        );
        config.maximum_batch_source_span_ms = 0;
        assert!(config.validate().is_err());
        config.maximum_batch_source_span_ms = 10_001;
        assert!(config.validate().is_err());
    }
}
