pub(super) mod auth;
mod config;
mod host;
mod tasks;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Result;
use axum::{Router, middleware, routing::get};
use clap::Parser;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde_json::json;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    ServerSlug, SubscriptionHub, TelemetryGuard, TokenIssuer, init_server_telemetry,
    public_allowed_hosts,
};
use veoveo_task_runtime::{TaskRuntime, TaskRuntimeConfig};

use crate::{
    acquisition::{
        AcquisitionHelper, AcquisitionHelperConfig, AcquisitionService, AcquisitionServiceConfig,
    },
    admin,
    analytics::{MapAnalytics, MapAnalyticsConfig},
    artifacts::ArtifactRepository,
    catalog::MapCatalog,
    geography::GeographyService,
    mcp::MapMcp,
    release_products::{ReleaseProductConfig, ReleaseProducts},
    routes::{
        RouteService,
        valhalla::{
            ValhallaClient, ValhallaClientConfig, ValhallaPlanner, ValhallaProcess,
            ValhallaProcessConfig,
        },
    },
    state::MapApplication,
};

use auth::{AdminAuthState, InternalAuthState, authenticate_internal, authorize_admin};
use config::Args;
use host::validate_host;
use tasks::{MapTaskExtension, recover_tasks};

const SERVER_SLUG: &str = "map";

pub async fn run() -> Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-map-mcp", "info,veoveo_map_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
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

    let catalog = MapCatalog::new(tasks.platform_store().clone());
    let analytics = MapAnalytics::open(MapAnalyticsConfig {
        database_path: args.map_database.clone(),
        spill_dir: args.duckdb_spill_dir.clone(),
        spatial_extension: args.spatial_extension.clone(),
        memory_limit: args.duckdb_memory_limit.clone(),
        threads: args.duckdb_threads,
    })?;
    analytics.verify_spatial()?;
    let valhalla_client = ValhallaClient::new(ValhallaClientConfig {
        base_url: args.valhalla_url.clone(),
        timeout: Duration::from_secs(args.valhalla_timeout_seconds),
    })?;
    let valhalla_process = ValhallaProcess::start(
        ValhallaProcessConfig {
            executable: args.valhalla_executable.clone(),
            config_file: args.valhalla_config.clone(),
            concurrency: args.valhalla_concurrency,
            startup_timeout: Duration::from_secs(args.valhalla_startup_timeout_seconds),
        },
        &valhalla_client,
    )
    .await?;
    let routes = RouteService::new(
        catalog.clone(),
        analytics.clone(),
        ValhallaPlanner::new(valhalla_client.clone()),
    );
    let artifacts = ArtifactRepository::new(args.artifact_service_url.clone());
    let products = ReleaseProducts::new(
        ReleaseProductConfig {
            release_root: args.release_root.clone(),
            valhalla_active_dir: args.valhalla_active_dir.clone(),
            maximum_routing_expanded_bytes: args.max_routing_expanded_bytes,
        },
        analytics.clone(),
    )?;
    let helper = AcquisitionHelper::new(AcquisitionHelperConfig {
        python_executable: args.helper_python.clone(),
        module: args.helper_module.clone(),
        maximum_output_bytes: args.max_artifact_bytes,
    })?;
    let acquisitions = Arc::new(AcquisitionService::new(
        AcquisitionServiceConfig {
            scratch_root: args.acquisition_scratch_root.clone(),
            mount_root: args.source_mount_root.clone(),
            secret_root: args.source_secret_root.clone(),
            maximum_artifact_bytes: args.max_artifact_bytes,
        },
        catalog.clone(),
        helper,
        artifacts.clone(),
        products.clone(),
    )?);
    let state = Arc::new(MapApplication {
        tasks,
        catalog: catalog.clone(),
        analytics: analytics.clone(),
        routes,
        geography: GeographyService::new(catalog, analytics.clone()),
        acquisitions,
        artifacts,
        products,
        valhalla_process: valhalla_process.clone(),
        activation: Arc::new(tokio::sync::Mutex::new(())),
        subscriptions: Arc::new(SubscriptionHub::new()),
    });
    recover_tasks(state.clone(), recovery.resumable).await?;

    let cancellation = tokio_util::sync::CancellationToken::new();
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let auth_state = InternalAuthState {
        verifier: verifier.clone(),
    };
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(MapMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_cancellation_token(cancellation.child_token()),
    );
    let task_extension = Arc::new(veoveo_mcp_task_extension::TaskExtensionAdapter::new(
        Arc::new(MapTaskExtension::new(state.clone())),
        veoveo_mcp_task_extension::ServerDiscovery::new(
            std::collections::BTreeMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({})),
                ("prompts".to_owned(), json!({})),
            ]),
            veoveo_mcp_task_extension::Implementation {
                name: SERVER_SLUG.to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some("Versioned Earth geography and durable logistics routing.".to_owned()),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            veoveo_mcp_task_extension::task_extension_middleware::<MapTaskExtension>,
        ))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_internal,
        ));
    let admin_router = admin::router(state.clone())
        .layer(middleware::from_fn_with_state(
            AdminAuthState {
                required_scope: args.admin_scope.clone(),
            },
            authorize_admin,
        ))
        .layer(middleware::from_fn_with_state(
            auth_state,
            authenticate_internal,
        ));
    let health_analytics = analytics.clone();
    let health_valhalla = valhalla_client.clone();
    let health_process = valhalla_process.clone();
    let server_router = Router::new()
        .route(
            "/healthz",
            get(move || {
                let analytics = health_analytics.clone();
                let valhalla = health_valhalla.clone();
                let process = health_process.clone();
                async move {
                    let spatial = tokio::task::spawn_blocking(move || analytics.verify_spatial())
                        .await
                        .is_ok_and(|result| result.is_ok());
                    let routing = process.exited().await.is_ok_and(|exited| !exited)
                        && valhalla.health().await.is_ok();
                    let status = if spatial && routing {
                        axum::http::StatusCode::OK
                    } else {
                        axum::http::StatusCode::SERVICE_UNAVAILABLE
                    };
                    (
                        status,
                        axum::Json(json!({"spatial": spatial, "routing": routing})),
                    )
                }
            }),
        )
        .nest("/mcp", mcp_router)
        .nest("/admin", admin_router);
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
        service = "veoveo-map-mcp",
        %address,
        mcp_path = public_endpoint.path("mcp"),
        admin_path = public_endpoint.path("admin"),
        "listening"
    );
    let listener = tokio::net::TcpListener::bind(address).await?;
    let serve_result = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await;
    valhalla_process.stop().await;
    serve_result?;
    Ok(())
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
