mod auth;
mod config;
mod host;
mod tasks;

use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use axum::{Json, Router, http::StatusCode, middleware, routing::get};
use clap::Parser;
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, session::local::LocalSessionManager,
};
use serde_json::json;
use tokio::sync::Semaphore;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    ServerSlug, SubscriptionHub, TelemetryGuard, TokenIssuer, init_server_telemetry,
    public_allowed_hosts,
};
use veoveo_task_runtime::{TaskRuntime, TaskRuntimeConfig};

use crate::{
    mcp::ViewMcp,
    renderer::RendererHandle,
    source::{LayerCatalog, LayerCatalogFile},
    state::ViewService,
};

use auth::{InternalAuthState, authenticate_internal};
use config::Args;
use host::validate_host;
use tasks::{ViewTaskExtension, recover_tasks};

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
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );

    let catalog_bytes = tokio::fs::read(&args.layer_catalog)
        .await
        .with_context(|| format!("read layer catalog {}", args.layer_catalog.display()))?;
    let catalog_file: LayerCatalogFile = serde_json::from_slice(&catalog_bytes)
        .with_context(|| format!("parse layer catalog {}", args.layer_catalog.display()))?;
    let catalog = LayerCatalog::from_definitions(catalog_file.layers, args.source_config())?;
    let renderer = RendererHandle::start(args.renderer_config())?;
    tracing::info!(
        adapter = renderer.adapter().name,
        backend = renderer.adapter().backend,
        device_type = renderer.adapter().device_type,
        "hardware renderer initialized"
    );
    let views = Arc::new(ViewService::new(args.view_config(), catalog, renderer));

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
    let recovery = tasks.recover().await?;
    let state = Arc::new(AppState {
        views,
        tasks,
        captures: Semaphore::new(args.max_captures_in_flight),
        subscriptions: SubscriptionHub::new(),
    });
    recover_tasks(state.clone(), recovery.resumable).await?;

    let cancellation = tokio_util::sync::CancellationToken::new();
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let auth_state = InternalAuthState { verifier };
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(ViewMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(cancellation.child_token()),
    );
    let task_extension = Arc::new(veoveo_mcp_task_extension::TaskExtensionAdapter::new(
        Arc::new(ViewTaskExtension::new(state.clone())),
        veoveo_mcp_task_extension::ServerDiscovery::new(
            std::collections::BTreeMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({})),
            ]),
            veoveo_mcp_task_extension::Implementation {
                name: SERVER_SLUG.to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some("Headless hardware-rendered 3D Tiles points of view.".to_owned()),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            veoveo_mcp_task_extension::task_extension_middleware::<ViewTaskExtension>,
        ))
        .layer(middleware::from_fn_with_state(
            auth_state,
            authenticate_internal,
        ));

    let readiness_state = state.clone();
    let server_router = Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route(
            "/readyz",
            get(move || {
                let adapter = readiness_state.views.adapter().clone();
                async move {
                    let status = if adapter.hardware_accelerated && adapter.nvidia {
                        StatusCode::OK
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    };
                    (status, Json(adapter))
                }
            }),
        )
        .nest("/mcp", mcp_router);
    let router = Router::new()
        .nest(public_endpoint.mount_path(), server_router)
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        service = "veoveo-view-mcp",
        %address,
        mcp_path = public_endpoint.path("mcp"),
        readiness_path = public_endpoint.path("readyz"),
        "listening"
    );
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await?;
    Ok(())
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
