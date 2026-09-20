mod config;
mod credentials;
mod kubernetes;
mod kubernetes_types;
mod reconcile;
mod resources;
#[cfg(test)]
mod tests;

use anyhow::{Result, ensure};
use futures::{StreamExt, stream};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::watch;
use veoveo_platform_store::{
    OutboxEventRecord, PlatformStore, PlatformTable, StoreConfig, StoreCredentials,
};

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
    tokio::spawn(kube.watch_pods(changed.clone()));
    tokio::spawn(observe_intents(store, changed));
    let mut recovery = tokio::time::interval(Duration::from_secs(5));
    recovery.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut audit = tokio::time::interval(Duration::from_secs(30));
    audit.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut include_settled = true;
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    tracing::info!(
        namespace = manager.config.namespace,
        "agent lifecycle manager ready"
    );
    loop {
        match manager
            .store
            .pending_managed_agent_operations(&manager.config.namespace, 200, include_settled)
            .await
        {
            Ok(operations) => {
                let work = stream::iter(operations)
                    .for_each_concurrent(4, |operation| manager.process(operation));
                tokio::select! { _ = work => {}, _ = &mut shutdown => return Ok(()) }
            }
            Err(error) => tracing::warn!(%error, "managed intent inventory unavailable"),
        }
        include_settled = tokio::select! {
            _ = &mut shutdown => return Ok(()),
            _ = recovery.tick() => false,
            _ = changes.changed() => true,
            _ = audit.tick() => true,
        };
    }
}

async fn observe_intents(store: PlatformStore, changed: watch::Sender<u64>) {
    loop {
        match store
            .live::<OutboxEventRecord>(PlatformTable::OutboxEvent)
            .await
        {
            Ok(mut live) => {
                changed.send_modify(|version| *version = version.wrapping_add(1));
                while let Some(event) = live.next().await {
                    match event {
                        Ok(event)
                            if matches!(
                                event.data.aggregate_type.as_str(),
                                "managed_agent" | "agent_definition" | "agent"
                            ) =>
                        {
                            changed.send_modify(|version| *version = version.wrapping_add(1))
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(%error, "managed intent stream requires recovery");
                            break;
                        }
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "managed intent stream unavailable"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
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
