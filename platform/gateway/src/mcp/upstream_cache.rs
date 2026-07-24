use std::{collections::BTreeMap, time::Duration};

use rmcp::service::{Peer, RoleClient, RunningService};
use tokio::sync::RwLock;
use veoveo_mcp_contract::{PrincipalId, ServerSlug};

use super::upstream::GatewayUpstreamHandler;

const UPSTREAM_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(super) struct UpstreamConnectionCache {
    connections: RwLock<BTreeMap<UpstreamCacheKey, UpstreamConnection>>,
}

impl UpstreamConnectionCache {
    pub(super) fn new() -> Self {
        Self {
            connections: RwLock::new(BTreeMap::new()),
        }
    }

    pub(super) async fn reusable_peer(&self, key: &UpstreamCacheKey) -> Option<Peer<RoleClient>> {
        let connections = self.connections.read().await;
        connections
            .get(key)
            .filter(|connection| connection.is_reusable())
            .map(|connection| connection.running.peer().clone())
    }

    pub(super) async fn close_stale(&self, current_generation: u64) {
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
