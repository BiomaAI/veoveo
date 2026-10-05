#[path = "computers-mcp/config.rs"]
mod config;
use clap::Parser;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::GatewayInternalTrustBundle;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    veoveo_computers_mcp::protocol::validate_contract();
    let args = config::Args::parse();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _telemetry = veoveo_mcp_contract::init_server_telemetry("veoveo-computers-mcp", "info")?;
    let config = veoveo_computers_mcp::config::Configuration::load(&args.config)
        .await?
        .prepare()
        .await?;
    let trust = GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?;
    let store_args = args.store();
    let store_config = veoveo_platform_store::StoreConfig::builder(
        store_args.endpoint,
        store_args.namespace,
        store_args.database,
        store_args.credentials,
    )
    .audit_targets(veoveo_gateway_catalog::audit_target_registry()?)
    .build()?;
    let platform = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        veoveo_platform_store::PlatformStore::connect(store_config),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Computers store connection timed out"))?
    .map_err(|_| anyhow::anyhow!("Computers store connection failed"))?;
    let tasks = veoveo_task_runtime::TaskRuntime::new(
        platform,
        "computers",
        format!("computers-{}", uuid::Uuid::now_v7()),
    );
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
    veoveo_computers_mcp::server::serve(
        config,
        tasks,
        trust,
        shutdown,
        veoveo_gateway_catalog::registry()?,
    )
    .await
}
