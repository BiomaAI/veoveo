mod config;
mod events;
mod http;
mod state;
mod tls;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{TelemetryGuard, init_server_telemetry};

use config::Args;
use state::PoseIngress;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard = init_server_telemetry(
        "veoveo-simulation-view-pose",
        "info,veoveo_simulation_view_pose_ingress=debug",
    )?;
    let args = Args::parse();
    args.validate()?;
    let ingress = Arc::new(PoseIngress::new(args.state_config())?);
    let cancellation = CancellationToken::new();
    tokio::spawn(events::announce_runtime_generation(
        args.runtime_event_url()?,
        args.control_token.clone(),
        cancellation.child_token(),
    ));
    let http = http::serve(
        args.http_address(),
        ingress.clone(),
        cancellation.child_token(),
    );
    let tls = tls::serve(
        args.tls_address(),
        ingress,
        args.tls_config()?,
        args.maximum_connections,
        cancellation.child_token(),
    );
    tokio::select! {
        result = http => result?,
        result = tls => result?,
        _ = tokio::signal::ctrl_c() => {},
    }
    cancellation.cancel();
    Ok(())
}
