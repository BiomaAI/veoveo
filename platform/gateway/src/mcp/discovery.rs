use std::{collections::BTreeMap, time::Duration};

use rmcp::model::{Resource, ResourceTemplate, Tool};
use tokio::{
    sync::{Mutex, Notify, broadcast},
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
const DISCOVERY_SETTLE_BUDGET: Duration = Duration::from_secs(2);

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
    observed_expired: bool,
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
impl<T: Clone + PartialEq> SurfaceCache<T> {
    #[cfg(test)]
    async fn contains(&self, key: &DiscoveryCacheKey) -> bool {
        self.0
            .lock()
            .await
            .entries
            .get(key)
            .is_some_and(|entry| entry.expires > Instant::now())
    }

    async fn pending(&self, keys: &[DiscoveryCacheKey]) -> bool {
        let state = self.0.lock().await;
        keys.iter().any(|key| state.in_flight.contains_key(key))
    }

    async fn get(&self, key: &DiscoveryCacheKey) -> Option<Vec<T>> {
        // Retain the prior value only to compare refreshes. Expired authority
        // must never be returned to a caller.
        let mut state = self.0.lock().await;
        let entry = state.entries.get_mut(key)?;
        if entry.expires <= Instant::now() {
            entry.observed_expired = true;
            return None;
        }
        Some(entry.items.clone())
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
            let changed = state
                .entries
                .get(&fetch.key)
                .is_none_or(|previous| previous.items != items || previous.observed_expired);
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
                    observed_expired: false,
                },
            );
            changed
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
    settled: Notify,
}
impl Default for CatalogDiscoveryCache {
    fn default() -> Self {
        Self {
            resources: Default::default(),
            resource_templates: Default::default(),
            tools: Default::default(),
            changes: broadcast::channel(DISCOVERY_CHANGE_BUFFER).0,
            settled: Notify::new(),
        }
    }
}
impl CatalogDiscoveryCache {
    #[cfg(test)]
    pub(super) async fn contains(
        &self,
        surface: GatewayDiscoverySurface,
        key: &DiscoveryCacheKey,
    ) -> bool {
        match surface {
            GatewayDiscoverySurface::Resources => self.resources.contains(key).await,
            GatewayDiscoverySurface::ResourceTemplates => {
                self.resource_templates.contains(key).await
            }
            GatewayDiscoverySurface::Tools => self.tools.contains(key).await,
        }
    }

    /// Give parallel discovery one shared, bounded opportunity to produce a
    /// useful first snapshot. A slow optional server cannot extend this budget.
    pub(super) async fn settle(
        &self,
        surface: GatewayDiscoverySurface,
        keys: &[DiscoveryCacheKey],
    ) {
        let _ = tokio::time::timeout(DISCOVERY_SETTLE_BUDGET, async {
            loop {
                let changed = self.settled.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let pending = match surface {
                    GatewayDiscoverySurface::Resources => self.resources.pending(keys).await,
                    GatewayDiscoverySurface::ResourceTemplates => {
                        self.resource_templates.pending(keys).await
                    }
                    GatewayDiscoverySurface::Tools => self.tools.pending(keys).await,
                };
                if !pending {
                    return;
                }
                changed.await;
            }
        })
        .await;
    }

    pub(super) fn subscribe(&self) -> broadcast::Receiver<DiscoveryChange> {
        self.changes.subscribe()
    }

    pub(super) async fn missing_code(
        &self,
        surface: GatewayDiscoverySurface,
        key: &DiscoveryCacheKey,
    ) -> GatewayDiscoveryFailureCode {
        let keys = std::slice::from_ref(key);
        let pending = match surface {
            GatewayDiscoverySurface::Resources => self.resources.pending(keys).await,
            GatewayDiscoverySurface::ResourceTemplates => {
                self.resource_templates.pending(keys).await
            }
            GatewayDiscoverySurface::Tools => self.tools.pending(keys).await,
        };
        if pending {
            GatewayDiscoveryFailureCode::DiscoveryPending
        } else {
            GatewayDiscoveryFailureCode::UpstreamUnavailable
        }
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
        self.settled.notify_waiters();
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
            self.settled.notify_waiters();
        }
    }
    pub(super) async fn finish_resources(&self, fetch: DiscoveryFetch, items: Vec<Resource>) {
        if self.resources.finish(&fetch, Some(items)).await {
            self.publish(GatewayDiscoverySurface::Resources, fetch);
        }
        self.settled.notify_waiters();
    }
    pub(super) async fn finish_resource_templates(
        &self,
        fetch: DiscoveryFetch,
        items: Vec<ResourceTemplate>,
    ) {
        if self.resource_templates.finish(&fetch, Some(items)).await {
            self.publish(GatewayDiscoverySurface::ResourceTemplates, fetch);
        }
        self.settled.notify_waiters();
    }
    pub(super) async fn finish_tools(&self, fetch: DiscoveryFetch, items: Vec<Tool>) {
        if self.tools.finish(&fetch, Some(items)).await {
            self.publish(GatewayDiscoverySurface::Tools, fetch);
        }
        self.settled.notify_waiters();
    }
    fn publish(&self, surface: GatewayDiscoverySurface, fetch: DiscoveryFetch) {
        self.settled.notify_waiters();
        let _ = self.changes.send(DiscoveryChange {
            surface,
            key: fetch.key,
        });
    }
    pub(super) async fn invalidate_resource_surfaces(&self, server: &ServerSlug) {
        self.resources.invalidate(server).await;
        self.resource_templates.invalidate(server).await;
        self.settled.notify_waiters();
    }
    pub(super) async fn invalidate_tools(&self, server: &ServerSlug) {
        self.tools.invalidate(server).await;
        self.settled.notify_waiters();
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
    async fn unchanged_refresh_does_not_feed_back_but_recovery_and_invalidation_notify() {
        let cache = CatalogDiscoveryCache::default();
        let key = key(1, "stable");
        let items = vec![Resource::new("stable://app", "app")];
        let mut updates = cache.subscribe();
        let first = begin(&cache, key.clone()).await;
        cache.finish_resources(first, items.clone()).await;
        assert!(updates.try_recv().is_ok());

        cache
            .resources
            .0
            .lock()
            .await
            .entries
            .get_mut(&key)
            .unwrap()
            .expires = Instant::now();
        let refresh = begin(&cache, key.clone()).await;
        cache.finish_resources(refresh, items.clone()).await;
        assert!(matches!(
            updates.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert_eq!(cache.resources(&key).await, Some(items.clone()));

        cache
            .resources
            .0
            .lock()
            .await
            .entries
            .get_mut(&key)
            .unwrap()
            .expires = Instant::now();
        assert!(
            cache.resources(&key).await.is_none(),
            "expired permissions are never served"
        );
        let recovery = begin(&cache, key.clone()).await;
        cache.finish_resources(recovery, items.clone()).await;
        assert!(
            updates.try_recv().is_ok(),
            "an observed unavailable source must recover"
        );

        cache.invalidate_resource_surfaces(&key.server).await;
        assert!(cache.resources(&key).await.is_none());
        let refreshed = begin(&cache, key.clone()).await;
        cache.finish_resources(refreshed, items).await;
        assert!(updates.try_recv().is_ok());
    }

    #[tokio::test]
    async fn cold_and_expired_catalogs_settle_before_the_first_snapshot() {
        let cache = std::sync::Arc::new(CatalogDiscoveryCache::default());
        let cache_key = key(1, "ready");
        for version in 0..2 {
            if let Some(entry) = cache.resources.0.lock().await.entries.get_mut(&cache_key) {
                entry.expires = Instant::now();
            }
            assert!(
                !cache
                    .contains(GatewayDiscoverySurface::Resources, &cache_key)
                    .await
            );
            let fetch = begin(&cache, cache_key.clone()).await;
            let writer = cache.clone();
            let work = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(650)).await;
                writer
                    .finish_resources(
                        fetch,
                        vec![Resource::new(format!("ready://{version}"), "ready")],
                    )
                    .await;
            });
            tokio::time::timeout(
                Duration::from_secs(2),
                cache.settle(
                    GatewayDiscoverySurface::Resources,
                    std::slice::from_ref(&cache_key),
                ),
            )
            .await
            .unwrap();
            assert_eq!(
                cache.resources(&cache_key).await.unwrap()[0].uri,
                format!("ready://{version}")
            );
            work.await.unwrap();
        }
    }

    #[tokio::test]
    async fn settlement_shares_one_deadline_and_failed_sources_release_waiters() {
        let cache = std::sync::Arc::new(CatalogDiscoveryCache::default());
        let failed = key(1, "failed");
        let fetch = begin(&cache, failed.clone()).await;
        let writer = cache.clone();
        let work = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            writer
                .finish_failure(GatewayDiscoverySurface::Resources, fetch)
                .await;
        });
        tokio::time::timeout(
            Duration::from_millis(250),
            cache.settle(GatewayDiscoverySurface::Resources, &[failed]),
        )
        .await
        .unwrap();
        work.await.unwrap();

        let keys = (0..8)
            .map(|i| key(1, &format!("slow-{i}")))
            .collect::<Vec<_>>();
        for key in &keys {
            begin(&cache, key.clone()).await;
        }
        tokio::time::timeout(
            Duration::from_secs(3),
            cache.settle(GatewayDiscoverySurface::Resources, &keys),
        )
        .await
        .unwrap();
        assert!(
            cache.resources.pending(&keys).await,
            "the bounded response does not cancel independent discovery"
        );
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
