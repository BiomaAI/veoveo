use std::{collections::BTreeMap, time::Duration};

use rmcp::model::{Resource, ResourceTemplate, Tool};
use tokio::{
    sync::{Mutex, broadcast},
    time::Instant,
};
use uuid::Uuid;
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

/// A discovery response may publish only while its exact request is current.
#[derive(Debug, Clone)]
pub(super) struct DiscoveryFetch {
    key: DiscoveryCacheKey,
    request: Uuid,
}

#[derive(Debug)]
struct CachedItems<T> {
    items: Vec<T>,
    expires: Instant,
}

#[derive(Debug)]
struct SurfaceState<T> {
    generation: u64,
    entries: BTreeMap<DiscoveryCacheKey, CachedItems<T>>,
    in_flight: BTreeMap<DiscoveryCacheKey, Uuid>,
}

#[derive(Debug)]
struct SurfaceCache<T>(Mutex<SurfaceState<T>>);
impl<T> Default for SurfaceCache<T> {
    fn default() -> Self {
        Self(Mutex::new(SurfaceState {
            generation: 0,
            entries: BTreeMap::new(),
            in_flight: BTreeMap::new(),
        }))
    }
}
impl<T: Clone> SurfaceCache<T> {
    async fn get(&self, key: &DiscoveryCacheKey) -> Option<Vec<T>> {
        let mut state = self.0.lock().await;
        if state
            .entries
            .get(key)
            .is_some_and(|entry| entry.expires <= Instant::now())
        {
            state.entries.remove(key);
        }
        state.entries.get(key).map(|entry| entry.items.clone())
    }

    async fn begin(&self, key: DiscoveryCacheKey, coalesce: bool) -> Option<DiscoveryFetch> {
        let mut state = self.0.lock().await;
        if key.catalog_generation < state.generation {
            return None;
        }
        if key.catalog_generation > state.generation {
            state.entries.clear();
            state.in_flight.clear();
            state.generation = key.catalog_generation;
        }
        if (coalesce && state.in_flight.contains_key(&key))
            || state.in_flight.len() >= MAX_CACHE_ENTRIES_PER_SURFACE
        {
            return None;
        }
        let request = Uuid::now_v7();
        state.in_flight.insert(key.clone(), request);
        Some(DiscoveryFetch { key, request })
    }

    async fn finish(&self, fetch: &DiscoveryFetch, items: Option<Vec<T>>) -> bool {
        let mut state = self.0.lock().await;
        if state.in_flight.get(&fetch.key) != Some(&fetch.request) {
            return false;
        }
        state.in_flight.remove(&fetch.key);
        if let Some(items) = items {
            if state.entries.len() >= MAX_CACHE_ENTRIES_PER_SURFACE
                && !state.entries.contains_key(&fetch.key)
            {
                state.entries.pop_first();
            }
            state.entries.insert(
                fetch.key.clone(),
                CachedItems {
                    items,
                    expires: Instant::now()
                        + Duration::from_millis(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
                },
            );
            true
        } else {
            false
        }
    }

    async fn invalidate(&self, server: &ServerSlug) {
        let mut state = self.0.lock().await;
        state.entries.retain(|key, _| &key.server != server);
        // A response captured before this event must not repopulate the cache,
        // even if a replacement request has already started for the same key.
        state.in_flight.retain(|key, _| &key.server != server);
    }
}

#[derive(Debug)]
pub(super) struct CatalogDiscoveryCache {
    resources: SurfaceCache<Resource>,
    resource_templates: SurfaceCache<ResourceTemplate>,
    tools: SurfaceCache<Tool>,
    changes: broadcast::Sender<DiscoveryChange>,
}
impl Default for CatalogDiscoveryCache {
    fn default() -> Self {
        Self {
            resources: Default::default(),
            resource_templates: Default::default(),
            tools: Default::default(),
            changes: broadcast::channel(DISCOVERY_CHANGE_BUFFER).0,
        }
    }
}
impl CatalogDiscoveryCache {
    pub(super) fn subscribe(&self) -> broadcast::Receiver<DiscoveryChange> {
        self.changes.subscribe()
    }

    pub(super) async fn begin(
        &self,
        surface: GatewayDiscoverySurface,
        key: DiscoveryCacheKey,
    ) -> Option<DiscoveryFetch> {
        match surface {
            GatewayDiscoverySurface::Resources => self.resources.begin(key, true).await,
            GatewayDiscoverySurface::ResourceTemplates => {
                self.resource_templates.begin(key, true).await
            }
            GatewayDiscoverySurface::Tools => self.tools.begin(key, true).await,
        }
    }
    pub(super) async fn finish_failure(
        &self,
        surface: GatewayDiscoverySurface,
        fetch: DiscoveryFetch,
    ) {
        match surface {
            GatewayDiscoverySurface::Resources => self.resources.finish(&fetch, None).await,
            GatewayDiscoverySurface::ResourceTemplates => {
                self.resource_templates.finish(&fetch, None).await
            }
            GatewayDiscoverySurface::Tools => self.tools.finish(&fetch, None).await,
        };
    }
    pub(super) async fn resources(&self, key: &DiscoveryCacheKey) -> Option<Vec<Resource>> {
        self.resources.get(key).await
    }
    pub(super) async fn resource_templates(
        &self,
        key: &DiscoveryCacheKey,
    ) -> Option<Vec<ResourceTemplate>> {
        self.resource_templates.get(key).await
    }
    pub(super) async fn tools(&self, key: &DiscoveryCacheKey) -> Option<Vec<Tool>> {
        self.tools.get(key).await
    }
    pub(super) async fn start_tools(&self, key: DiscoveryCacheKey) -> Option<DiscoveryFetch> {
        self.tools.begin(key, false).await
    }
    pub(super) async fn store_tools(&self, fetch: Option<DiscoveryFetch>, tools: Vec<Tool>) {
        if let Some(fetch) = fetch {
            self.tools.finish(&fetch, Some(tools)).await;
        }
    }
    pub(super) async fn finish_resources(&self, fetch: DiscoveryFetch, items: Vec<Resource>) {
        if self.resources.finish(&fetch, Some(items)).await {
            self.publish(GatewayDiscoverySurface::Resources, fetch);
        }
    }
    pub(super) async fn finish_resource_templates(
        &self,
        fetch: DiscoveryFetch,
        items: Vec<ResourceTemplate>,
    ) {
        if self.resource_templates.finish(&fetch, Some(items)).await {
            self.publish(GatewayDiscoverySurface::ResourceTemplates, fetch);
        }
    }
    pub(super) async fn finish_tools(&self, fetch: DiscoveryFetch, items: Vec<Tool>) {
        if self.tools.finish(&fetch, Some(items)).await {
            self.publish(GatewayDiscoverySurface::Tools, fetch);
        }
    }
    fn publish(&self, surface: GatewayDiscoverySurface, fetch: DiscoveryFetch) {
        let _ = self.changes.send(DiscoveryChange {
            surface,
            key: fetch.key,
        });
    }
    pub(super) async fn invalidate_resource_surfaces(&self, server: &ServerSlug) {
        self.resources.invalidate(server).await;
        self.resource_templates.invalidate(server).await;
    }
    pub(super) async fn invalidate_tools(&self, server: &ServerSlug) {
        self.tools.invalidate(server).await;
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

    async fn begin(cache: &CatalogDiscoveryCache, key: DiscoveryCacheKey) -> DiscoveryFetch {
        cache
            .begin(GatewayDiscoverySurface::Resources, key)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn discovery_is_coalesced_and_catalog_generation_is_monotonic() {
        let cache = CatalogDiscoveryCache::default();
        let stale = begin(&cache, key(1, "one")).await;
        assert!(
            cache
                .begin(GatewayDiscoverySurface::Resources, key(1, "one"))
                .await
                .is_none()
        );
        let current = begin(&cache, key(2, "two")).await;
        cache
            .finish_resources(stale, vec![Resource::new("one://old", "old")])
            .await;
        assert!(cache.resources(&key(1, "one")).await.is_none());
        assert!(
            cache
                .begin(GatewayDiscoverySurface::Resources, key(1, "one"))
                .await
                .is_none()
        );
        cache
            .finish_resources(current, vec![Resource::new("two://current", "current")])
            .await;
        assert_eq!(
            cache.resources(&key(2, "two")).await.unwrap()[0].uri,
            "two://current"
        );
    }

    #[tokio::test]
    async fn invalidated_in_flight_response_cannot_replace_new_contents_or_finish_new_request() {
        let cache = CatalogDiscoveryCache::default();
        let stale = begin(&cache, key(1, "one")).await;
        let other = begin(&cache, key(1, "two")).await;
        cache
            .finish_resources(other, vec![Resource::new("two://kept", "kept")])
            .await;
        cache
            .invalidate_resource_surfaces(&ServerSlug::new("one").unwrap())
            .await;
        let current = begin(&cache, key(1, "one")).await;
        cache
            .finish_resources(stale.clone(), vec![Resource::new("one://stale", "stale")])
            .await;
        cache
            .finish_failure(GatewayDiscoverySurface::Resources, stale)
            .await;
        assert!(cache.resources(&key(1, "one")).await.is_none());
        assert!(cache.resources(&key(1, "two")).await.is_some());
        cache
            .finish_resources(current, vec![Resource::new("one://new", "new")])
            .await;
        assert_eq!(
            cache.resources(&key(1, "one")).await.unwrap()[0].uri,
            "one://new"
        );
    }

    #[tokio::test]
    async fn synchronous_tool_discovery_is_fenced_against_notification_races() {
        let cache = CatalogDiscoveryCache::default();
        let stale = cache.start_tools(key(1, "one")).await;
        cache
            .invalidate_tools(&ServerSlug::new("one").unwrap())
            .await;
        let current = cache.start_tools(key(1, "one")).await;
        cache
            .store_tools(
                stale,
                vec![Tool::new("stale", "old", std::sync::Arc::default())],
            )
            .await;
        assert!(cache.tools(&key(1, "one")).await.is_none());
        cache
            .store_tools(
                current,
                vec![Tool::new("current", "new", std::sync::Arc::default())],
            )
            .await;
        assert_eq!(
            cache.tools(&key(1, "one")).await.unwrap()[0].name,
            "current"
        );
    }

    #[tokio::test]
    async fn expired_entries_require_fresh_discovery_after_missed_notifications() {
        let cache = CatalogDiscoveryCache::default();
        let fetch = begin(&cache, key(1, "one")).await;
        cache
            .finish_resources(fetch, vec![Resource::new("one://old", "old")])
            .await;
        // Set the actual deadline directly; no wall-clock delay in this unit test.
        cache
            .resources
            .0
            .lock()
            .await
            .entries
            .get_mut(&key(1, "one"))
            .unwrap()
            .expires = Instant::now();
        assert!(cache.resources(&key(1, "one")).await.is_none());
        let fetch = begin(&cache, key(1, "one")).await;
        cache
            .finish_resources(fetch, vec![Resource::new("one://new", "new")])
            .await;
        assert_eq!(
            cache.resources(&key(1, "one")).await.unwrap()[0].uri,
            "one://new"
        );
    }

    #[tokio::test]
    async fn one_server_completes_while_another_remains_in_flight() {
        let cache = CatalogDiscoveryCache::default();
        let _hung = begin(&cache, key(1, "hung")).await;
        let healthy = begin(&cache, key(1, "healthy")).await;
        let mut changes = cache.subscribe();
        cache.finish_resources(healthy, Vec::new()).await;
        assert_eq!(changes.recv().await.unwrap().key, key(1, "healthy"));
        assert!(cache.resources(&key(1, "hung")).await.is_none());
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
