use crate::{Application, CapacityHealth, config::PreparedConfiguration};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use veoveo_computers::{ComputersStore, api::CapacityAvailability};
use veoveo_mcp_contract::GatewayInternalTokenVerifier;
use veoveo_task_runtime::TaskRuntime;

/// Serves an already validated installation profile. The same path is used by
/// the executable and isolated service fixtures with their own platform store.
pub async fn serve(
    config: PreparedConfiguration,
    tasks: TaskRuntime,
    verifier: GatewayInternalTokenVerifier,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let _cancel_on_drop = shutdown.clone().drop_guard();
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let store = ComputersStore::new(tasks.platform_store().clone(), config.provider_instance_id)?;
    tokio::time::timeout(
        Duration::from_secs(5),
        store.install_session_grant_policy(None, config.access),
    )
    .await??;
    if let Some(provider) = &config.provider {
        // First install or an exact retry only. A changed quota requires an
        // explicit compare-and-set transition; replicas cannot overwrite it.
        tokio::time::timeout(
            Duration::from_secs(5),
            store.install_capacity(None, provider.limits),
        )
        .await??;
        tokio::time::timeout(
            Duration::from_secs(5),
            store.install_automation_grant_policy(None, provider.execution.policy),
        )
        .await??;
    }
    let (health, receiver) = watch::channel(CapacityHealth {
        availability: if config.provider.is_some() {
            CapacityAvailability::ComputeUnavailable
        } else {
            CapacityAvailability::SetupRequired
        },
        observed_at: Instant::now(),
    });
    let templates = config.templates.runtimes();
    let (runtime_sender, runtime_access) = crate::RuntimeAccess::channel();
    let mut app = Application::new(
        store.clone(),
        tasks.clone(),
        config.templates,
        receiver,
        runtime_access,
    )?;
    if let Some(provider) = &config.provider {
        app = app.with_maintenance(provider.maintenance.clone())?;
        app = app.with_execution(
            provider.execution.keys.clone(),
            provider.execution.artifacts.clone(),
            provider.execution.templates.clone(),
        )?;
    }
    let app = Arc::new(app);
    let router = super::router(
        app,
        verifier,
        config.allowed_hosts,
        config.allowed_origins,
        shutdown.clone(),
    )?;
    let background = config.provider.map(|provider| {
        tokio::spawn(super::provider::maintain(
            provider,
            store,
            tasks,
            templates,
            health,
            runtime_sender,
            shutdown.clone(),
        ))
    });
    let result = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown.clone().cancelled_owned())
        .await;
    shutdown.cancel();
    if let Some(mut background) = background
        && tokio::time::timeout(Duration::from_secs(10), &mut background)
            .await
            .is_err()
    {
        background.abort();
        let _ = background.await;
    }
    result.map_err(Into::into)
}
