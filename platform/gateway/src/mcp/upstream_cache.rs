use std::{collections::BTreeMap, sync::Arc, time::Duration};

use rmcp::service::{Peer, RoleClient, RunningService};
use tokio::sync::{Mutex, RwLock};
use veoveo_mcp_contract::{PrincipalId, ServerSlug};

use super::upstream::GatewayUpstreamHandler;

const UPSTREAM_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(super) struct UpstreamConnectionCache {
    connections: RwLock<BTreeMap<UpstreamCacheKey, UpstreamConnection>>,
    initialization_locks: Mutex<BTreeMap<UpstreamCacheKey, Arc<Mutex<()>>>>,
}

impl UpstreamConnectionCache {
    pub(super) fn new() -> Self {
        Self {
            connections: RwLock::new(BTreeMap::new()),
            initialization_locks: Mutex::new(BTreeMap::new()),
        }
    }

    /// Coalesce concurrent connection attempts for one exact authority while
    /// allowing unrelated upstreams to initialize independently.
    pub(super) async fn initialization_lock(&self, key: &UpstreamCacheKey) -> Arc<Mutex<()>> {
        self.initialization_locks
            .lock()
            .await
            .entry(key.clone())
            .or_default()
            .clone()
    }

    pub(super) async fn reusable_peer(&self, key: &UpstreamCacheKey) -> Option<Peer<RoleClient>> {
        let connections = self.connections.read().await;
        connections
            .get(key)
            .filter(|connection| connection.is_reusable())
            .map(|connection| connection.running.peer().clone())
    }

    pub(super) async fn close_stale(&self, current_generation: u64) {
        self.initialization_locks
            .lock()
            .await
            .retain(|key, _| key.catalog_generation == current_generation);
        let stale_connections = {
            let mut connections = self.connections.write().await;
            let stale_keys = connections
                .iter()
                .filter_map(|(key, connection)| {
                    if key.catalog_generation != current_generation
                        || connection.running.is_closed()
                    {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            stale_keys
                .into_iter()
                .filter_map(|key| connections.remove(&key).map(|connection| (key, connection)))
                .collect::<Vec<_>>()
        };
        for (key, connection) in stale_connections {
            close_upstream_connection(key, connection, "stale upstream connection").await;
        }
    }

    pub(super) async fn insert_or_reuse(
        &self,
        key: UpstreamCacheKey,
        connection: UpstreamConnection,
    ) -> Peer<RoleClient> {
        let peer = connection.running.peer().clone();
        let mut connections = self.connections.write().await;
        if let Some(existing) = connections.get(&key)
            && existing.is_reusable()
        {
            let existing_peer = existing.running.peer().clone();
            drop(connections);
            close_upstream_connection(key, connection, "superseded upstream connection").await;
            return existing_peer;
        }
        let replaced = connections.insert(key.clone(), connection);
        drop(connections);
        if let Some(replaced) = replaced {
            close_upstream_connection(key, replaced, "replaced upstream connection").await;
        }
        peer
    }

    pub(super) async fn invalidate(&self, key: &UpstreamCacheKey, reason: &'static str) {
        let connection = self.connections.write().await.remove(key);
        if let Some(connection) = connection {
            close_upstream_connection(key.clone(), connection, reason).await;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct UpstreamCacheKey {
    pub(super) server: ServerSlug,
    pub(super) principal: PrincipalId,
    /// SHA-256 over the actor and resolved invocation authority.
    pub(super) authorization_fingerprint: [u8; 32],
    pub(super) catalog_generation: u64,
}

#[derive(Debug)]
pub(super) struct UpstreamConnection {
    pub(super) running: RunningService<RoleClient, GatewayUpstreamHandler>,
}

impl UpstreamConnection {
    fn is_reusable(&self) -> bool {
        !self.running.is_closed()
    }
}

async fn close_upstream_connection(
    key: UpstreamCacheKey,
    mut connection: UpstreamConnection,
    reason: &'static str,
) {
    if connection.running.is_closed() {
        return;
    }
    match connection
        .running
        .close_with_timeout(UPSTREAM_CLOSE_TIMEOUT)
        .await
    {
        Ok(Some(_)) => {
            tracing::debug!(
                server = %key.server,
                principal = %key.principal,
                catalog_generation = key.catalog_generation,
                reason,
                "closed gateway upstream MCP connection"
            );
        }
        Ok(None) => {
            tracing::warn!(
                server = %key.server,
                principal = %key.principal,
                catalog_generation = key.catalog_generation,
                reason,
                "timed out closing gateway upstream MCP connection"
            );
        }
        Err(err) => {
            tracing::warn!(
                server = %key.server,
                principal = %key.principal,
                catalog_generation = key.catalog_generation,
                reason,
                "failed to close gateway upstream MCP connection: {err}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(server: &str, catalog_generation: u64) -> UpstreamCacheKey {
        UpstreamCacheKey {
            server: ServerSlug::new(server).unwrap(),
            principal: PrincipalId::new("issuer#operator").unwrap(),
            authorization_fingerprint: [7; 32],
            catalog_generation,
        }
    }

    #[tokio::test]
    async fn initialization_is_single_flight_per_exact_authority() {
        let cache = UpstreamConnectionCache::new();
        let authority = key("fleet", 4);
        let first = cache.initialization_lock(&authority).await;
        let second = cache.initialization_lock(&authority).await;
        assert!(Arc::ptr_eq(&first, &second));

        let held = first.lock().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), second.lock())
                .await
                .is_err()
        );
        drop(held);
        let _next = tokio::time::timeout(Duration::from_millis(20), second.lock())
            .await
            .expect("the next exact-authority initializer proceeds after release");
    }

    #[tokio::test]
    async fn unrelated_upstream_authorities_initialize_independently() {
        let cache = UpstreamConnectionCache::new();
        let fleet = cache.initialization_lock(&key("fleet", 4)).await;
        let map = cache.initialization_lock(&key("map", 4)).await;
        assert!(!Arc::ptr_eq(&fleet, &map));

        let _fleet_held = fleet.lock().await;
        let _map = tokio::time::timeout(Duration::from_millis(20), map.lock())
            .await
            .expect("a different authority is not serialized");
    }

    #[tokio::test]
    async fn catalog_generation_cleanup_releases_old_initialization_keys() {
        let cache = UpstreamConnectionCache::new();
        let old = cache.initialization_lock(&key("fleet", 3)).await;
        cache.close_stale(4).await;
        let replacement = cache.initialization_lock(&key("fleet", 3)).await;
        assert!(!Arc::ptr_eq(&old, &replacement));
    }
}
