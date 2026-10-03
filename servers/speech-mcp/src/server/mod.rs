mod config;
mod dictation;
mod mcp;
mod setup;
mod tasks;

use crate::{application::SpeechService, process::WorkerProcess};
use clap::Parser;
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, PublicDeployment,
    docs::ServerDocs,
    hosting::{Hosted, HostedServer},
};
use veoveo_task_runtime::{DurableTasks, TaskRuntime, TaskRuntimeConfig};

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("speech"));

pub async fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _telemetry = veoveo_mcp_contract::init_server_telemetry("veoveo-speech-mcp", "info")?;
    let args = config::Args::parse();
    LazyLock::force(&setup::SERVER_SETUP);
    anyhow::ensure!(
        args.concurrent_recordings > 0
            && args.concurrent_recordings < usize::from(args.inference_capacity),
        "recording capacity must reserve at least one inference slot for dictation"
    );
    let deployment = PublicDeployment::new(&args.public_base_url)?;
    let worker = Arc::new(WorkerProcess::start(&args.python, args.inference_capacity).await?);
    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            args.surreal_endpoint,
            args.surreal_namespace,
            args.surreal_database,
            args.surreal_auth_level,
            args.surreal_username,
            args.surreal_password,
        ),
        "speech",
        format!("speech-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let service = Arc::new(SpeechService::new(
        tasks,
        HttpArtifactPlane::new(args.artifact_service_url),
        worker,
        args.concurrent_recordings,
        usize::from(args.inference_capacity) - args.concurrent_recordings,
    ));
    let stop = CancellationToken::new();
    let recovery = {
        let service = service.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            // Lease recovery is runtime maintenance and never initiates new model work.
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { () = stop.cancelled() => break, _ = tick.tick() => {} }
                match service.tasks.recover().await {
                    Ok(report) => {
                        for snapshot in report.resumable {
                            if let Err(error) = service.resume(snapshot).await {
                                tracing::warn!(%error, "Speech recovery could not claim work");
                            }
                        }
                    }
                    Err(error) => tracing::warn!(%error, "Speech recovery store unavailable"),
                }
                service.tasks.reap_workers().await;
            }
        })
    };
    for host in &args.allowed_hosts {
        anyhow::ensure!(
            veoveo_mcp_contract::parse_allowed_host_authority(host).is_some(),
            "invalid allowed host"
        );
    }
    let server = hosted_server(
        service.clone(),
        &deployment,
        args.allow_loopback_hosts,
        args.allowed_hosts,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    )?;
    let address = std::net::SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, args.port));
    // Audit closure stops the server as SIGTERM and Ctrl-C do.
    let serving = server.serve_with_shutdown(address, {
        let stop = stop.clone();
        let audit = service.audit.clone();
        async move {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("install SIGTERM handler");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {}, _ = audit.closed() => {} }
            stop.cancel();
        }
    });
    tokio::pin!(serving);
    let result: anyhow::Result<()> = tokio::select! {
        result = &mut serving => result,
        _ = stop.cancelled() => tokio::time::timeout(Duration::from_secs(30), &mut serving)
            .await.map_err(|_| anyhow::anyhow!("Speech HTTP shutdown deadline exceeded"))
            .and_then(|result| result),
    };
    stop.cancel();
    recovery.abort();
    let _ = recovery.await;
    let sessions = service.dictations.shutdown().await;
    let drained = service.audit.shutdown(Duration::from_secs(30)).await;
    result?;
    sessions?;
    drained?;
    Ok(())
}

/// Builds the hosted Speech server for the executable and native qualification.
pub fn hosted_server(
    service: Arc<SpeechService>,
    deployment: &PublicDeployment,
    allow_loopback_hosts: bool,
    allowed_hosts: Vec<String>,
    trust: GatewayInternalTrustBundle,
) -> anyhow::Result<HostedServer> {
    let (alive, ready_state) = (service.clone(), service.clone());
    let dictation = dictation::router().with_state(service.clone());
    Ok(HostedServer::for_domain::<mcp::SpeechMcp>()
        .deployment(deployment, allow_loopback_hosts)?
        .allowed_hosts(allowed_hosts)
        .internal_trust(trust)?
        .handler(move || {
            Hosted::new(mcp::SpeechMcp::new(service.clone())).with_tasks(
                DurableTasks::with_listener(
                    tasks::SpeechTasks(service.clone()),
                    mcp::SpeechListener {
                        state: service.clone(),
                    },
                ),
            )
        })
        .authenticated_routes(dictation)
        // A dead inference worker needs a restart.
        .liveness(move || {
            let state = alive.clone();
            async move { state.worker.ready().await.is_ok() }
        })
        .readiness(move || {
            let state = ready_state.clone();
            async move { ready(&state).await }
        })
        .build())
}

/// Ready while audit runs, the worker answers and the Store responds.
async fn ready(state: &SpeechService) -> bool {
    state.audit.is_running()
        && state.worker.ready().await.is_ok()
        && matches!(
            tokio::time::timeout(
                Duration::from_secs(2),
                state.tasks.platform_store().healthcheck()
            )
            .await,
            Ok(Ok(()))
        )
}
