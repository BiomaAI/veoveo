use std::collections::{BTreeMap, BTreeSet};

use rmcp::model::{Resource, ResourceTemplate, Tool};
use tokio::sync::{Mutex, broadcast};
use veoveo_mcp_contract::{
    GatewayDiscoveryDegradation, GatewayDiscoveryFailure, GatewayDiscoveryFailureCode,
    GatewayDiscoverySurface, PrincipalId, ServerSlug,
};

pub(super) const MAX_CONCURRENT_DISCOVERY: usize = 8;
const MAX_CACHE_ENTRIES_PER_SURFACE: usize = 4_096;
const DISCOVERY_CHANGE_BUFFER: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct DiscoveryCacheKey {
    pub(super) catalog_generation: u64,
    pub(super) principal: PrincipalId,
    pub(super) authorization_fingerprint: [u8; 32],
    pub(super) server: ServerSlug,
}

#[derive(Debug, Clone)]
pub(super) struct DiscoveryChange {
    pub(super) surface: GatewayDiscoverySurface,
    pub(super) key: DiscoveryCacheKey,
}

impl DiscoveryChange {
    pub(super) fn belongs_to(
        &self,
        catalog_generation: u64,
        principal: &PrincipalId,
        authorization_fingerprint: &[u8; 32],
    ) -> bool {
        self.key.catalog_generation == catalog_generation
            && &self.key.principal == principal
            && &self.key.authorization_fingerprint == authorization_fingerprint
    }
}

#[derive(Debug)]
pub(super) struct CatalogDiscoveryCache {
    resources: Mutex<BTreeMap<DiscoveryCacheKey, Vec<Resource>>>,
    resource_templates: Mutex<BTreeMap<DiscoveryCacheKey, Vec<ResourceTemplate>>>,
    tools: Mutex<BTreeMap<DiscoveryCacheKey, Vec<Tool>>>,
    in_flight: Mutex<BTreeSet<(GatewayDiscoverySurface, DiscoveryCacheKey)>>,
    changes: broadcast::Sender<DiscoveryChange>,
}

impl Default for CatalogDiscoveryCache {
    fn default() -> Self {
        let (changes, _) = broadcast::channel(DISCOVERY_CHANGE_BUFFER);
        Self {
            resources: Mutex::new(BTreeMap::new()),
            resource_templates: Mutex::new(BTreeMap::new()),
            tools: Mutex::new(BTreeMap::new()),
            in_flight: Mutex::new(BTreeSet::new()),
            changes,
        }
    }
}

impl CatalogDiscoveryCache {
    pub(super) fn subscribe(&self) -> broadcast::Receiver<DiscoveryChange> {
        self.changes.subscribe()
    }

    /// Claim one missing per-server discovery operation. The claim is shared by every
    /// stateless handler for this profile, so repeated list calls never multiply a hung
    /// upstream request.
    pub(super) async fn begin(
        &self,
        surface: GatewayDiscoverySurface,
        key: DiscoveryCacheKey,
    ) -> bool {
        let mut in_flight = self.in_flight.lock().await;
        in_flight.retain(|(_, candidate)| candidate.catalog_generation == key.catalog_generation);
        if in_flight.len() >= MAX_CACHE_ENTRIES_PER_SURFACE {
            return false;
        }
        in_flight.insert((surface, key))
    }

    pub(super) async fn finish_failure(
        &self,
        surface: GatewayDiscoverySurface,
        key: &DiscoveryCacheKey,
    ) {
        self.in_flight.lock().await.remove(&(surface, key.clone()));
    }

    pub(super) async fn resources(&self, key: &DiscoveryCacheKey) -> Option<Vec<Resource>> {
        self.resources.lock().await.get(key).cloned()
    }

    pub(super) async fn finish_resources(&self, key: DiscoveryCacheKey, resources: Vec<Resource>) {
        self.finish_completed(
            GatewayDiscoverySurface::Resources,
            &self.resources,
            key,
            resources,
        )
        .await;
    }

    pub(super) async fn resource_templates(
        &self,
        key: &DiscoveryCacheKey,
    ) -> Option<Vec<ResourceTemplate>> {
        self.resource_templates.lock().await.get(key).cloned()
    }

    pub(super) async fn finish_resource_templates(
        &self,
        key: DiscoveryCacheKey,
        templates: Vec<ResourceTemplate>,
    ) {
        self.finish_completed(
            GatewayDiscoverySurface::ResourceTemplates,
            &self.resource_templates,
            key,
            templates,
        )
        .await;
    }

    pub(super) async fn tools(&self, key: &DiscoveryCacheKey) -> Option<Vec<Tool>> {
        self.tools.lock().await.get(key).cloned()
    }

    pub(super) async fn store_tools(&self, key: DiscoveryCacheKey, tools: Vec<Tool>) {
        store(&self.tools, key, tools).await;
    }

    pub(super) async fn finish_tools(&self, key: DiscoveryCacheKey, tools: Vec<Tool>) {
        self.finish_completed(GatewayDiscoverySurface::Tools, &self.tools, key, tools)
            .await;
    }

    pub(super) async fn invalidate_resource_surfaces(&self, server: &ServerSlug) {
        self.resources
            .lock()
            .await
            .retain(|key, _| &key.server != server);
        self.resource_templates
            .lock()
            .await
            .retain(|key, _| &key.server != server);
    }

    pub(super) async fn invalidate_tools(&self, server: &ServerSlug) {
        self.tools
            .lock()
            .await
            .retain(|key, _| &key.server != server);
    }

    async fn finish_completed<T: Clone>(
        &self,
        surface: GatewayDiscoverySurface,
        cache: &Mutex<BTreeMap<DiscoveryCacheKey, Vec<T>>>,
        key: DiscoveryCacheKey,
        value: Vec<T>,
    ) {
        let mut in_flight = self.in_flight.lock().await;
        if !in_flight.contains(&(surface, key.clone())) {
            return;
        }
        store(cache, key.clone(), value).await;
        in_flight.remove(&(surface, key.clone()));
        drop(in_flight);
        let _ = self.changes.send(DiscoveryChange { surface, key });
    }
}

pub(super) fn isolate_discovery_failures<T, E>(
    surface: GatewayDiscoverySurface,
    results: Vec<(ServerSlug, Result<Vec<T>, E>)>,
) -> (Vec<T>, GatewayDiscoveryDegradation, Vec<(ServerSlug, E)>) {
    let mut values = Vec::new();
    let mut failures = Vec::new();
    let mut errors = Vec::new();
    for (server, result) in results {
        match result {
            Ok(mut discovered) => values.append(&mut discovered),
            Err(error) => {
                failures.push(GatewayDiscoveryFailure {
                    server: server.clone(),
                    surface,
                    code: GatewayDiscoveryFailureCode::UpstreamUnavailable,
                });
                errors.push((server, error));
            }
        }
    }
    (values, GatewayDiscoveryDegradation::new(failures), errors)
}

async fn store<T: Clone>(
    cache: &Mutex<BTreeMap<DiscoveryCacheKey, Vec<T>>>,
    key: DiscoveryCacheKey,
    value: Vec<T>,
) {
    let mut cache = cache.lock().await;
    cache.retain(|candidate, _| candidate.catalog_generation == key.catalog_generation);
    if cache.len() >= MAX_CACHE_ENTRIES_PER_SURFACE && !cache.contains_key(&key) {
        cache.pop_first();
    }
    cache.insert(key, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(generation: u64, server: &str) -> DiscoveryCacheKey {
        DiscoveryCacheKey {
            catalog_generation: generation,
            principal: PrincipalId::new("principal").unwrap(),
            authorization_fingerprint: [7; 32],
            server: ServerSlug::new(server).unwrap(),
        }
    }

    #[tokio::test]
    async fn a_new_catalog_generation_evicts_old_discovery() {
        let cache = CatalogDiscoveryCache::default();
        let first = key(1, "one");
        assert!(
            cache
                .begin(GatewayDiscoverySurface::Resources, first.clone())
                .await
        );
        cache.finish_resources(first, Vec::new()).await;
        let second = key(2, "two");
        assert!(
            cache
                .begin(GatewayDiscoverySurface::Resources, second.clone())
                .await
        );
        cache.finish_resources(second, Vec::new()).await;
        assert!(cache.resources(&key(1, "one")).await.is_none());
        assert!(cache.resources(&key(2, "two")).await.is_some());
    }

    #[tokio::test]
    async fn list_change_invalidation_is_scoped_to_one_server() {
        let cache = CatalogDiscoveryCache::default();
        for server in ["one", "two"] {
            let key = key(1, server);
            assert!(
                cache
                    .begin(GatewayDiscoverySurface::Resources, key.clone())
                    .await
            );
            cache.finish_resources(key, Vec::new()).await;
        }
        cache
            .invalidate_resource_surfaces(&ServerSlug::new("one").unwrap())
            .await;
        assert!(cache.resources(&key(1, "one")).await.is_none());
        assert!(cache.resources(&key(1, "two")).await.is_some());
    }

    #[tokio::test]
    async fn one_server_completes_while_another_remains_in_flight() {
        let cache = CatalogDiscoveryCache::default();
        let hung = key(1, "hung");
        let healthy = key(1, "healthy");
        let mut changes = cache.subscribe();

        assert!(
            cache
                .begin(GatewayDiscoverySurface::Resources, hung.clone())
                .await
        );
        assert!(
            cache
                .begin(GatewayDiscoverySurface::Resources, healthy.clone())
                .await
        );
        assert!(
            !cache
                .begin(GatewayDiscoverySurface::Resources, hung.clone())
                .await,
            "a repeated list must not multiply an unresponsive request"
        );

        cache.finish_resources(healthy.clone(), Vec::new()).await;
        let change = changes
            .recv()
            .await
            .expect("healthy completion is published");
        assert_eq!(change.surface, GatewayDiscoverySurface::Resources);
        assert_eq!(change.key, healthy);
        assert!(cache.resources(&hung).await.is_none());
    }

    #[tokio::test]
    async fn stale_completion_cannot_repopulate_a_new_catalog_generation() {
        let cache = CatalogDiscoveryCache::default();
        let stale = key(1, "server");
        assert!(
            cache
                .begin(GatewayDiscoverySurface::Resources, stale.clone())
                .await
        );
        let current = key(2, "server");
        assert!(
            cache
                .begin(GatewayDiscoverySurface::Resources, current.clone())
                .await
        );

        cache.finish_resources(stale.clone(), Vec::new()).await;
        assert!(cache.resources(&stale).await.is_none());
        cache.finish_resources(current.clone(), Vec::new()).await;
        assert!(cache.resources(&current).await.is_some());
    }

    #[test]
    fn one_failed_server_does_not_discard_healthy_discovery() {
        let healthy = ServerSlug::new("healthy").unwrap();
        let failed = ServerSlug::new("failed").unwrap();
        let (values, degradation, errors) = isolate_discovery_failures(
            GatewayDiscoverySurface::Resources,
            vec![
                (healthy, Ok::<_, &str>(vec!["app"])),
                (failed.clone(), Err("unavailable")),
            ],
        );
        assert_eq!(values, vec!["app"]);
        assert_eq!(errors, vec![(failed.clone(), "unavailable")]);
        assert_eq!(degradation.failures.len(), 1);
        assert_eq!(degradation.failures[0].server, failed);
    }
}
