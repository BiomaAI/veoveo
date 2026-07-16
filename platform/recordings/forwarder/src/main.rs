use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;
use veoveo_recording_forwarder::ForwarderConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    veoveo_recording_forwarder::run(ForwarderConfig::parse()).await
}
