#[path = "computers-mcp/config.rs"]
mod config;
use clap::Parser;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    ServerSlug, TokenIssuer,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = config::Args::parse();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _telemetry = veoveo_mcp_contract::init_server_telemetry("veoveo-computers-mcp", "info")?;
    let config = veoveo_computers_mcp::config::Configuration::load(&args.config)
        .await?
        .prepare()
        .await?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new("computers")?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let tasks = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        veoveo_task_runtime::TaskRuntime::connect(
            args.store(),
            "computers",
            format!("computers-{}", uuid::Uuid::now_v7()),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Computers store connection timed out"))?
    .map_err(|_| anyhow::anyhow!("Computers store connection failed"))?;
    let shutdown = CancellationToken::new();
    let signal = shutdown.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            if let Ok(mut terminate) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            } else {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
        #[cfg(not(unix))]
        let _ = tokio::signal::ctrl_c().await;
        signal.cancel();
    });
    veoveo_computers_mcp::server::serve(config, tasks, verifier, shutdown).await
}
