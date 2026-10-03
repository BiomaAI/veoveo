//! Streamable HTTP entrypoint for the dedicated artifact MCP server.

use std::{net::SocketAddr, sync::Arc};

use clap::Parser;
use tokio_util::sync::CancellationToken;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, TelemetryGuard,
    hosting::{Hosted, HostedServer, ListenOnly},
    init_server_telemetry,
};
use veoveo_platform_store::PlatformStore;

#[path = "server/config.rs"]
mod config;
#[path = "server/handler.rs"]
mod handler;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/setup.rs"]
mod setup;
#[path = "server/subscriptions.rs"]
mod subscriptions;

#[cfg(test)]
#[path = "../../../../testing/fixtures/store.rs"]
mod store_fixture;
#[cfg(test)]
#[path = "server/subscription_tests/mod.rs"]
mod subscription_tests;

use config::Args;
use handler::{AppState, ArtifactListener, ArtifactMcp};
use subscriptions::{ArtifactSubscriptions, start_dispatcher};

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-artifact-mcp", "info,veoveo_artifact_mcp=debug")?;
    let args = Args::parse();
    std::sync::LazyLock::force(&setup::SERVER_SETUP);
    let public_deployment = args.public_deployment()?;
    let store = PlatformStore::connect(args.store_config()?).await?;
    let plane = HttpArtifactPlane::new(args.artifact_service_url);
    let subscriptions = ArtifactSubscriptions::new(store.clone());
    let state = Arc::new(AppState {
        plane: plane.clone(),
        subscriptions: subscriptions.clone(),
        public_base_url: public_deployment
            .base_url()
            .trim_end_matches('/')
            .to_owned(),
    });
    let cancellation = CancellationToken::new();
    start_dispatcher(
        store.clone(),
        subscriptions,
        cancellation.child_token(),
        veoveo_platform_store::ChangefeedConsumerId::new(format!(
            "artifact/{}/{}",
            args.replica_id, args.port
        ))?,
    )
    .await?;

    let server = hosted_server(
        state,
        store,
        &public_deployment,
        args.allow_loopback_hosts,
        args.allowed_hosts,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    )?;
    let stop = server.cancellation_token();
    let served = server
        .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
        .await;
    cancellation.cancel();
    stop.cancel();
    served
}

/// Builds the hosted Artifact server for the binary and the subscription tests.
fn hosted_server(
    state: Arc<AppState>,
    store: PlatformStore,
    deployment: &veoveo_mcp_contract::PublicDeployment,
    allow_loopback_hosts: bool,
    allowed_hosts: Vec<String>,
    trust: GatewayInternalTrustBundle,
) -> anyhow::Result<HostedServer> {
    let readiness_plane = state.plane.clone();
    Ok(HostedServer::for_domain::<ArtifactMcp>()
        .deployment(deployment, allow_loopback_hosts)?
        .allowed_hosts(allowed_hosts)
        .readiness(move || {
            let store = store.clone();
            let plane = readiness_plane.clone();
            async move {
                matches!(
                    tokio::time::timeout(std::time::Duration::from_secs(5), async {
                        store.healthcheck().await.is_ok() && plane.readiness().await.is_ok()
                    })
                    .await,
                    Ok(true)
                )
            }
        })
        .internal_trust(trust)?
        .handler(move || {
            Hosted::new(ArtifactMcp::new(state.clone())).with_tasks(ListenOnly::new(
                ArtifactListener {
                    state: state.clone(),
                },
            ))
        })
        .build())
}
