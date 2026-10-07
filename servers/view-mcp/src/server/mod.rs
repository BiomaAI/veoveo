pub(crate) mod auth;
mod config;
mod recovery;
pub(crate) mod setup;
pub(crate) mod tasks;

use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use clap::Parser;
use tokio::sync::Semaphore;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, SubscriptionHub, TelemetryGuard,
    hosting::{Hosted, HostedServer},
    init_server_telemetry,
};
use veoveo_task_runtime::{DurableTasks, TaskRuntime, TaskRuntimeConfig};

use crate::{
    mcp::{ViewMcp, ViewSubscriptions},
    renderer::RendererHandle,
    source::{LayerCatalog, LayerCatalogFile},
    state::ViewService,
};

use config::Args;
use tasks::ViewTaskExtension;

pub(crate) const SERVER_SLUG: &str = "view";

pub(crate) struct AppState {
    pub views: Arc<ViewService>,
    pub tasks: TaskRuntime,
    pub captures: Semaphore,
    pub subscriptions: SubscriptionHub,
}

pub async fn run() -> Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-view-mcp", "info,veoveo_view_mcp=debug")?;
    let args = Args::parse();
    args.validate()?;
    std::sync::LazyLock::force(&setup::SERVER_SETUP);
    let public_deployment = args.public_deployment()?;

    let catalog_bytes = tokio::fs::read(&args.layer_catalog)
        .await
        .with_context(|| format!("read layer catalog {}", args.layer_catalog.display()))?;
    let catalog_file: LayerCatalogFile = serde_json::from_slice(&catalog_bytes)
        .with_context(|| format!("parse layer catalog {}", args.layer_catalog.display()))?;
    let catalog = LayerCatalog::from_definitions(catalog_file.layers, args.source_config())?;
    let renderer = RendererHandle::start(args.renderer_config())?;
    let _renderer_shutdown = renderer.shutdown_on_drop();
    tracing::info!(
        adapter = renderer.adapter().name,
        backend = renderer.adapter().backend,
        device_type = renderer.adapter().device_type,
        "hardware renderer initialized"
    );
    let artifacts = HttpArtifactPlane::new(&args.artifact_service_url);
    let views = Arc::new(ViewService::new(
        args.view_config(),
        catalog,
        renderer,
        artifacts,
    ));

    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            args.surreal_endpoint.clone(),
            args.surreal_namespace.clone(),
            args.surreal_database.clone(),
            args.surreal_auth_level,
            args.surreal_username.clone(),
            args.surreal_password.clone(),
        ),
        SERVER_SLUG,
        format!("{SERVER_SLUG}-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let mut recovery = tasks.observe_startup_recovery().await?;
    let initial = futures::StreamExt::next(&mut recovery).await.transpose()?;
    let state = Arc::new(AppState {
        views,
        tasks,
        captures: Semaphore::new(args.max_captures_in_flight),
        subscriptions: SubscriptionHub::new(),
    });
    if let Some(initial) = initial {
        tasks::recover_tasks(state.clone(), initial.resumable).await?;
    }
    let observer = recovery::RecoveryObserver::start(state.clone(), recovery);

    let readiness_state = state.clone();
    let server = HostedServer::for_domain::<ViewMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler(move || {
            Hosted::new(ViewMcp::new(state.clone())).with_tasks(DurableTasks::with_resources(
                ViewTaskExtension::new(state.clone()),
                ViewSubscriptions::new(state.clone()),
            ))
        })
        // View serves only on a hardware-accelerated NVIDIA adapter.
        .readiness(move || {
            let adapter = readiness_state.views.adapter().clone();
            async move { adapter.hardware_accelerated && adapter.nvidia }
        })
        .build();
    observer
        .serve(server.serve(SocketAddr::from(([0, 0, 0, 0], args.port))))
        .await
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
