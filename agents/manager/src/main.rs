mod config;
mod credentials;
mod kubernetes;
mod kubernetes_types;
mod observation;
mod reconcile;
mod resources;
#[cfg(test)]
mod tests;

use anyhow::{Result, ensure};
use futures::{StreamExt, stream};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::watch;
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};

use config::Config;
use kubernetes::Kubernetes;
use reconcile::Manager;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();
    let config_path = std::env::var("VEOVEO_AGENT_MANAGER_CONFIG")?;
    let control_path = std::env::var("VEOVEO_GATEWAY_CONFIG")?;
    let config = Arc::new(Config::load(
        Path::new(&config_path),
        Path::new(&control_path),
    )?);
    ensure!(
        std::env::var("VEOVEO_SURREAL_AUTH_LEVEL").as_deref() == Ok("database"),
        "manager requires database-scoped credentials"
    );
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &config.store_endpoint,
            &config.store_namespace,
            &config.store_database,
            StoreCredentials::database(
                std::env::var("VEOVEO_SURREAL_USERNAME")?,
                std::env::var("VEOVEO_SURREAL_PASSWORD")?,
            ),
        )
        .build()?,
    )
    .await?;
    let kube = Kubernetes::in_cluster(config.namespace.clone())?;
    let manager = Manager {
        store: store.clone(),
        kube: kube.clone(),
        config,
    };
    let (changed, mut changes) = watch::channel(0_u64);
    for resource in [
        kubernetes::Resource::Pods,
        kubernetes::Resource::Deployments,
        kubernetes::Resource::Claims,
        kubernetes::Resource::Secrets,
        kubernetes::Resource::ConfigMaps,
    ] {
        tokio::spawn(kube.clone().watch_resources(resource, changed.clone()));
    }
    let mut intents = tokio::spawn(observation::observe_intents(store, changed));
    let mut retry_delay = Duration::from_millis(250);
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    tracing::info!(
        namespace = manager.config.namespace,
        "agent lifecycle manager ready"
    );
    loop {
        // Consume hints before inventory; writes during reconciliation remain
        // pending and immediately trigger the next pass.
        changes.borrow_and_update();
        let work = reconcile_inventory(&manager);
        let mut retry = tokio::select! {
            retry = work => retry,
            _ = &mut shutdown => return Ok(()),
            _ = &mut intents => anyhow::bail!("manager intent observer ended"),
        };
        let due = match manager
            .store
            .next_managed_agent_delay(&manager.config.namespace)
            .await
        {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "managed deadlines unavailable");
                retry = true;
                None
            }
        };
        let delay = if retry {
            let delay = retry_delay;
            retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
            Some(delay)
        } else {
            retry_delay = Duration::from_millis(250);
            due
        };
        let deadline = async {
            match delay {
                Some(delay) => tokio::time::sleep(delay).await,
                None => futures::future::pending().await,
            }
        };
        tokio::select! {
            _ = &mut shutdown => return Ok(()),
            _ = &mut intents => anyhow::bail!("manager intent observer ended"),
            changed = changes.changed() => { if changed.is_err() { anyhow::bail!("manager observation sources ended"); } },
            _ = deadline => {},
        }
    }
}

async fn reconcile_inventory(manager: &Manager) -> bool {
    let mut after = None;
    let mut retry = false;
    loop {
        let operations = match manager
            .store
            .pending_managed_agent_operations(&manager.config.namespace, 200, true, after)
            .await
        {
            Ok(operations) => operations,
            Err(error) => {
                tracing::warn!(%error, "managed intent inventory unavailable");
                return true;
            }
        };
        let full = operations.len() == 200;
        after = operations.last().map(|operation| operation.cursor());
        let mut work = stream::iter(operations)
            .map(|operation| manager.process(operation))
            .buffer_unordered(4);
        while let Some(failed) = work.next().await {
            retry |= failed;
        }
        if !full {
            return retry;
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
