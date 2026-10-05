//! Service construction, HTTP wiring, and owned observer shutdown.
use super::{SERVER_SLUG, UavSimMcp, fake_state, resources};
use crate::contract::LiveSessionId;
use crate::server::{
    config::{AdapterKind, Args},
    control_authority::VehicleControlAuthority,
    live_view::{LiveViewConfig, LiveViewService},
    live_view_audit::LiveViewAudit,
    state::AppState,
    task_extension::UavSimTaskExtension,
    task_worker::resume_queued_operation,
};
use crate::{
    adapter::{Adapter, FakeAdapter, HttpAdapter},
    contract::SimulationLifecycle,
};
use clap::Parser;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, SubscriptionHub, TelemetryGuard,
    hosting::{Hosted, HostedServer},
    init_server_telemetry,
};
use veoveo_task_runtime::{DurableTasks, TaskRuntime, TaskRuntimeConfig, WithResources};

pub(in crate::server) async fn serve() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-uav-sim-mcp", "info,veoveo_uav_sim_mcp=debug")?;
    std::sync::LazyLock::force(&super::super::setup::SERVER_SETUP);
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
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
    let tasks = crate::server::task_catalog::UavTaskContributions::bind(tasks)?;
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
    let runtime_session_id = LiveSessionId::parse(&session_id)?;
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
        live_view_audit: live_view_audit.clone(),
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
    anyhow::ensure!(
        args.live_stream_gate_port != args.port,
        "live-stream gate port must differ from the MCP port"
    );
    let live_stream_gate = super::super::live_stream::LiveStreamGate::new(
        live_views.clone(),
        subscribers,
        &args.public_stream_url,
        &args.runtime_stream_url,
        args.adapter_bearer_token.clone(),
    )?;
    let readiness_state = state.clone();
    let hosted_state = state.clone();
    let hosted = HostedServer::for_domain::<UavSimMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler(move || hosted(hosted_state.clone()))
        .readiness(move || {
            let state = readiness_state.clone();
            async move { ready(&state).await }
        })
        .build();
    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    let live_stream_address = SocketAddr::from(([0, 0, 0, 0], args.live_stream_gate_port));
    // Live-view audit closure or the live-stream gate also stops the server.
    let server = hosted.serve_with_shutdown(address, {
        let shutdown = shutdown.clone();
        let audit = live_view_audit.clone();
        async move {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("install SIGTERM handler");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {},
                _ = audit.closed() => {}, _ = shutdown.cancelled() => {},
            }
            shutdown.cancel();
        }
    });
    let mut live_stream_task =
        tokio::spawn(live_stream_gate.run(live_stream_address, shutdown.child_token()));
    tokio::pin!(server);
    let (result, server_finished) = tokio::select! {
        _ = shutdown.cancelled() => (Ok(()), false),
        result = &mut server => (result, true),
        result = &mut live_stream_task => (match result {
            Ok(result) => result,
            Err(error) => Err(error.into()),
        }, false),
    };
    shutdown.cancel();
    let http_drain = if !server_finished {
        tokio::time::timeout(std::time::Duration::from_secs(30), &mut server)
            .await
            .map_err(anyhow::Error::from)
            .and_then(|result| result)
    } else {
        Ok(())
    };
    let observers = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        if !live_stream_task.is_finished() {
            live_stream_task.await??;
        }
        target_observer.await?;
        resource_observer.await?;
        if let Some(task) = runtime_event_task {
            task.await?;
        }
        Ok::<(), anyhow::Error>(())
    })
    .await;
    let sessions = live_views.shutdown().await;
    let drained = live_view_audit.shutdown().await;
    http_drain?;
    observers??;
    sessions?;
    drained?;
    result
}

/// The hosted UAV handler.
pub(in crate::server) type HostedUav =
    Hosted<UavSimMcp, DurableTasks<UavSimTaskExtension, WithResources<UavSimMcp>>>;

/// The hosted handler: the domain, its durable tasks, and itself as the
/// resource-change source.
pub(in crate::server) fn hosted(state: Arc<AppState>) -> HostedUav {
    Hosted::new(UavSimMcp::new(state.clone())).with_tasks(DurableTasks::with_resources(
        UavSimTaskExtension::new(state.clone()),
        UavSimMcp::new(state),
    ))
}

/// Ready while live-view auditing runs and the simulation has not failed.
async fn ready(state: &AppState) -> bool {
    if !state.live_view_audit.is_running() {
        return false;
    }
    match state.adapter.state().await {
        Ok(simulation) => simulation.lifecycle != SimulationLifecycle::Failed,
        Err(error) => {
            tracing::warn!(%error, "UAV simulation MCP readiness failed");
            false
        }
    }
}
