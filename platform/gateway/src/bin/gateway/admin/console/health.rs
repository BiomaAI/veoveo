use std::{collections::BTreeMap, sync::Arc, time::Duration};

use parking_lot::RwLock;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::ServerSlug;
use veoveo_mcp_gateway::{
    GatewayServerHealth, GatewayServerHealthState, probe_gateway_server_health,
};

use crate::runtime::{SharedCatalog, current_catalog};

const SERVER_HEALTH_PROBE_INTERVAL: Duration = Duration::from_secs(15);

pub(crate) type ServerHealthCache = Arc<RwLock<BTreeMap<ServerSlug, GatewayServerHealth>>>;

/// Continuously probed MCP server health, decoupled from request handling.
/// `epoch` bumps only when some server's health *state* changes (not on
/// every probe), so stream consumers can wait on it without waking every
/// interval.
#[derive(Clone)]
pub(crate) struct ServerHealthMonitor {
    pub(crate) cache: ServerHealthCache,
    pub(crate) epoch: watch::Receiver<u64>,
}

impl ServerHealthMonitor {
    pub(crate) fn snapshot(&self) -> BTreeMap<ServerSlug, GatewayServerHealth> {
        self.cache.read().clone()
    }
}

pub(crate) fn spawn_server_health_prober(
    catalog: SharedCatalog,
    cancellation: CancellationToken,
) -> ServerHealthMonitor {
    let cache: ServerHealthCache = Arc::default();
    let (epoch_tx, epoch_rx) = watch::channel(0u64);
    let monitor = ServerHealthMonitor {
        cache: cache.clone(),
        epoch: epoch_rx,
    };
    tokio::spawn(async move {
        loop {
            let current = current_catalog(&catalog);
            let probed = probe_gateway_server_health(&current).await;
            let states_changed = {
                let mut cached = cache.write();
                let changed = health_states(&cached) != health_states(&probed);
                *cached = probed;
                changed
            };
            if states_changed {
                epoch_tx.send_modify(|epoch| *epoch += 1);
            }
            tokio::select! {
                () = cancellation.cancelled() => break,
                () = tokio::time::sleep(SERVER_HEALTH_PROBE_INTERVAL) => {}
            }
        }
    });
    monitor
}

fn health_states(
    health: &BTreeMap<ServerSlug, GatewayServerHealth>,
) -> BTreeMap<&ServerSlug, GatewayServerHealthState> {
    health
        .iter()
        .map(|(slug, health)| (slug, health.state))
        .collect()
}
