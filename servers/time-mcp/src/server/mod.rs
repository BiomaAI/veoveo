pub(super) mod auth;
mod config;
mod host;
mod tasks;

use std::{collections::BTreeMap, net::SocketAddr, sync::Arc, time::Duration};

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
    acquisition::{AcquisitionService, AcquisitionServiceConfig},
    admin,
    authority::{AuthorityContext, LeapSecondTable},
    catalog::TimeCatalog,
    clock::{ClockMonitor, ClockSource},
    contract::AuthorityReleaseId,
    mcp::TimeMcp,
    registry::AuthorityRegistry,
    state::TimeApplication,
};

use auth::{AdminAuthState, InternalAuthState, authenticate_internal, authorize_admin};
use config::Args;
use host::validate_host;
use tasks::{TimeTaskExtension, recover_tasks};

const SERVER_SLUG: &str = "time";

pub async fn run() -> Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-time-mcp", "info,veoveo_time_mcp=debug")?;
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
    let catalog = TimeCatalog::new(tasks.platform_store().clone());
    let leap_seconds = LeapSecondTable::from_path(&args.bootstrap_leap_seconds_file).await?;
    let bootstrap = AuthorityContext::from_paths(
        AuthorityReleaseId::new(args.bootstrap_tzdb_release_id.clone())
            .map_err(anyhow::Error::msg)?,
        AuthorityReleaseId::new(args.bootstrap_leap_seconds_release_id.clone())
            .map_err(anyhow::Error::msg)?,
        &args.bootstrap_tzdb_dir,
        leap_seconds,
    )?;
    let authorities = AuthorityRegistry::new(
        bootstrap,
        args.bootstrap_tzdb_dir.clone(),
        args.bootstrap_leap_seconds_file.clone(),
    );
    let clock = ClockMonitor::new(
        args.ntpd_observation_socket
            .clone()
            .map_or(ClockSource::System, |observation_socket| {
                ClockSource::NtpdRs { observation_socket }
            }),
        Duration::from_secs(args.clock_observation_timeout_seconds),
    );
    let acquisitions = Arc::new(AcquisitionService::new(
        AcquisitionServiceConfig {
            scratch_root: args.acquisition_scratch_root.clone(),
            release_root: args.release_root.clone(),
            zic_executable: args.zic_executable.clone(),
            maximum_source_bytes: args.maximum_source_bytes,
            maximum_expanded_bytes: args.maximum_expanded_bytes,
            timeout: Duration::from_secs(args.acquisition_timeout_seconds),
        },
        catalog.clone(),
    )?);
    let state = Arc::new(TimeApplication {
        tasks,
        catalog,
        authorities,
        clock,
        acquisitions,
        subscriptions: Arc::new(SubscriptionHub::new()),
        activation: Arc::new(tokio::sync::Mutex::new(())),
        event_watchers: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
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
            move || Ok(TimeMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_stateful_mode(true)
            .with_json_response(false)
            .with_cancellation_token(cancellation.child_token()),
    );
    let task_extension = Arc::new(veoveo_mcp_task_extension::TaskExtensionAdapter::new(
        Arc::new(TimeTaskExtension::new(state.clone())),
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
            Some("Versioned temporal authority and operational scheduling.".to_owned()),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            veoveo_mcp_task_extension::task_extension_middleware::<TimeTaskExtension>,
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
    let health_state = state.clone();
    let server_router = Router::new()
        .route(
            "/healthz",
            get(move || {
                let state = health_state.clone();
                async move {
                    let clock_observed = state.clock.quality().await.is_ok();
                    let status = if clock_observed {
                        axum::http::StatusCode::OK
                    } else {
                        axum::http::StatusCode::SERVICE_UNAVAILABLE
                    };
                    (
                        status,
                        axum::Json(json!({"authority": true, "clock_observed": clock_observed})),
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
    tracing::info!(service = "veoveo-time-mcp", %address, mcp_path = public_endpoint.path("mcp"), admin_path = public_endpoint.path("admin"), "listening");
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
