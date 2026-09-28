//! Service construction, HTTP wiring, and owned observer shutdown.
use super::{SERVER_SLUG, UavSimMcp, fake_state, resources};
use crate::server::{
    auth::{InternalMcpAuthState, authenticate_internal_mcp},
    config::{AdapterKind, Args},
    control_authority::VehicleControlAuthority,
    host::validate_host,
    live_view::{LiveViewConfig, LiveViewService},
    live_view_audit::LiveViewAudit,
    state::AppState,
    task_worker::resume_queued_operation,
};
use crate::{
    adapter::{Adapter, FakeAdapter, HttpAdapter},
    contract::SimulationLifecycle,
};
use axum::{Router, extract::State, http::StatusCode, middleware, routing::get};
use clap::Parser;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    LiveSessionId, ServerSlug, SubscriptionHub, TelemetryGuard, TokenIssuer, init_server_telemetry,
    public_allowed_hosts,
};
use veoveo_task_runtime::{TaskRuntime, TaskRuntimeConfig};

pub(in crate::server) async fn serve() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-uav-sim-mcp", "info,veoveo_uav_sim_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
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
    let control_authority = VehicleControlAuthority::new(tasks.platform_store().clone());
    let adapter = match args.adapter {
        AdapterKind::Http => Adapter::Http(Box::new(HttpAdapter::new(
            args.adapter_url()?,
            args.adapter_timeout()?,
            args.adapter_operation_timeout()?,
            args.adapter_bearer_token.clone(),
            tasks.platform_store().clone(),
            &args.recording_tenant_key,
        )?)),
        AdapterKind::Fake => Adapter::Fake(Arc::new(Mutex::new(FakeAdapter::new(fake_state()?)))),
    };
    let adapter = Arc::new(adapter);
    if let Some(path) = args.world_bootstrap_file.as_deref() {
        super::super::world_bootstrap::apply(path, &adapter).await?;
    }
    let session_id = adapter.state().await?.session_id;
    let runtime_session_id = LiveSessionId::new(session_id.to_string())?;
    let live_view_audit = LiveViewAudit::new(tasks.platform_store().clone());
    let live_views = LiveViewService::new(
        adapter.clone(),
        live_view_audit.clone(),
        LiveViewConfig {
            session_duration: args.live_view_session_duration()?,
            public_stream_url: args.public_stream_url.clone(),
            maximum_frame_age_ms: args.live_view_maximum_frame_age_ms,
        },
    )?;
    let stream_url = url::Url::parse(&args.public_stream_url)?;
    let live_view_connect_origin = stream_url.origin().ascii_serialization();
    anyhow::ensure!(
        live_view_connect_origin != "null",
        "public live-stream URL must have an HTTP(S) origin"
    );
    let subscribers = Arc::new(SubscriptionHub::new());
    let runtime_event_listener = (args.adapter == AdapterKind::Http).then(|| {
        super::super::runtime_events::RuntimeEventListener::new(
            runtime_session_id,
            args.world_bootstrap_file.clone(),
            adapter.clone(),
        )
    });
    let state = Arc::new(AppState {
        session_id,
        adapter,
        tasks,
        control_authority,
        subscribers: subscribers.clone(),
        live_views: live_views.clone(),
        live_view_audit,
        live_view_connect_origin,
    });
    for snapshot in recovery.resumable {
        resume_queued_operation(state.clone(), snapshot)
            .await
            .map_err(anyhow::Error::msg)?;
    }
    super::super::task_worker::reconcile_mission_retention(&state).await?;

    let shutdown = CancellationToken::new();
    let target_observer = tokio::spawn(super::super::agent_targets::observe(
        state.tasks.platform_store().clone(),
        subscribers.clone(),
        shutdown.child_token(),
    ));
    let resource_observer = tokio::spawn(resources::observe(
        state.tasks.platform_store().clone(),
        subscribers.clone(),
        shutdown.child_token(),
    ));
    let runtime_event_task = runtime_event_listener
        .map(|listener| tokio::spawn(listener.run(subscribers.clone(), shutdown.child_token())));
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(UavSimMcp::new(state.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(shutdown.child_token()),
    );
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn_with_state(
            InternalMcpAuthState {
                verifier: verifier.clone(),
            },
            authenticate_internal_mcp,
        ));
    // Read-only well-known projection (contract C20) behind the same gateway
    // authentication as the MCP surface.
    let admin_router = super::super::admin::router().layer(middleware::from_fn_with_state(
        InternalMcpAuthState { verifier },
        authenticate_internal_mcp,
    ));
    anyhow::ensure!(
        args.live_stream_gate_port != args.port,
        "live-stream gate port must differ from the MCP port"
    );
    let live_stream_gate = super::super::live_stream::LiveStreamGate::new(
        live_views,
        subscribers,
        &args.public_stream_url,
        &args.runtime_stream_url,
        args.adapter_bearer_token.clone(),
    )?;
    let router = Router::new()
        .nest(
            public_endpoint.mount_path(),
            Router::new()
                .route("/healthz", get(|| async { "ok" }))
                .route("/readyz", get(ready))
                .nest("/admin", admin_router)
                .nest("/mcp", mcp_router),
        )
        .with_state(state)
        .layer(middleware::from_fn_with_state(allowed_hosts, validate_host))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    let live_stream_address = SocketAddr::from(([0, 0, 0, 0], args.live_stream_gate_port));
    tracing::info!(%address, public_url = public_endpoint.public_url(), "UAV simulation MCP listening");
    let listener = tokio::net::TcpListener::bind(address).await?;
    let server = std::future::IntoFuture::into_future(
        axum::serve(listener, router).with_graceful_shutdown({
            let shutdown = shutdown.clone();
            async move {
                let _ = tokio::signal::ctrl_c().await;
                shutdown.cancel();
            }
        }),
    );
    let mut live_stream_task =
        tokio::spawn(live_stream_gate.run(live_stream_address, shutdown.child_token()));
    tokio::pin!(server);
    let result = tokio::select! {
        result = &mut server => result.map_err(Into::into),
        result = &mut live_stream_task => match result {
            Ok(result) => result,
            Err(error) => Err(error.into()),
        },
    };
    shutdown.cancel();
    if !live_stream_task.is_finished() {
        live_stream_task.await??;
    }
    target_observer.await?;
    resource_observer.await?;
    if let Some(task) = runtime_event_task {
        task.await?;
    }
    result
}

async fn ready(State(state): State<Arc<AppState>>) -> StatusCode {
    match state.adapter.state().await {
        Ok(simulation) if simulation.lifecycle != SimulationLifecycle::Failed => StatusCode::OK,
        Ok(_) => StatusCode::SERVICE_UNAVAILABLE,
        Err(error) => {
            tracing::warn!(%error, "UAV simulation MCP readiness failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
