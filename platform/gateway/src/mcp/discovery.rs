use std::{collections::BTreeMap, time::Duration};

use rmcp::ErrorData as McpError;
use rmcp::model::{Prompt, Resource, ResourceTemplate, Tool};
use tokio::{
    sync::{Mutex, Notify, broadcast},
    time::Instant,
};
use uuid::Uuid;
use veoveo_mcp_contract::{
    DiscoveryFailureMode, GatewayDiscoveryDegradation, GatewayDiscoveryFailure,
    GatewayDiscoveryFailureCode, GatewayDiscoverySurface, ServerSlug,
};
use veoveo_types::{PrincipalId, ResourceUri};

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

/// Keep the upstream identity beside the projected descriptor. Server-owned
/// projection can change both the scheme and the UI authority.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DiscoveredResource {
    pub(super) resource: Resource,
    pub(super) upstream_uri: ResourceUri,
    pub(super) listed: bool,
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
    denied: u32,
}
impl DiscoveryFetch {
    pub(super) fn with_denied(mut self, denied: u32) -> Self {
        self.denied = denied;
        self
    }
}
#[derive(Debug, Clone, PartialEq)]
pub(super) struct AdmittedCatalog<T> {
    pub(super) items: Vec<T>,
    pub(super) denied: u32,
}

#[derive(Debug)]
struct CachedItems<T> {
    items: Vec<T>,
    denied: u32,
}

#[derive(Debug)]
struct SurfaceState<T> {
    generation: u64,
    entries: BTreeMap<DiscoveryCacheKey, CachedItems<T>>,
    in_flight: BTreeMap<DiscoveryCacheKey, Uuid>,
    failed: BTreeMap<DiscoveryCacheKey, Instant>,
}

#[derive(Debug)]
struct SurfaceCache<T>(Mutex<SurfaceState<T>>);
impl<T> Default for SurfaceCache<T> {
    fn default() -> Self {
        Self(Mutex::new(SurfaceState {
            generation: 0,
            entries: BTreeMap::new(),
            in_flight: BTreeMap::new(),
            failed: BTreeMap::new(),
        }))
    }
}
impl<T: Clone + PartialEq> SurfaceCache<T> {
    #[cfg(test)]
    async fn contains(&self, key: &DiscoveryCacheKey) -> bool {
        self.0.lock().await.entries.contains_key(key)
    }

    async fn pending(&self, keys: &[DiscoveryCacheKey]) -> bool {
        let state = self.0.lock().await;
        keys.iter().any(|key| state.in_flight.contains_key(key))
    }

    async fn awaiting_initial_result(&self, keys: &[DiscoveryCacheKey]) -> bool {
        let state = self.0.lock().await;
        keys.iter()
            .any(|key| state.in_flight.contains_key(key) && !state.failed.contains_key(key))
    }

    async fn get(&self, key: &DiscoveryCacheKey) -> Option<AdmittedCatalog<T>> {
        let state = self.0.lock().await;
        let entry = state.entries.get(key)?;
        Some(AdmittedCatalog {
            items: entry.items.clone(),
            denied: entry.denied,
        })
    }

    async fn begin(&self, key: DiscoveryCacheKey, coalesce: bool) -> Option<DiscoveryFetch> {
        let mut state = self.0.lock().await;
        if key.catalog_generation < state.generation {
            return None;
        }
        if key.catalog_generation > state.generation {
            state.entries.clear();
            state.in_flight.clear();
            state.failed.clear();
            state.generation = key.catalog_generation;
        }
        if coalesce
            && state
                .failed
                .get(&key)
                .is_some_and(|retry_at| *retry_at > Instant::now())
        {
            return None;
        }
        if (coalesce && state.in_flight.contains_key(&key))
            || state.in_flight.len() >= MAX_CACHE_ENTRIES_PER_SURFACE
        {
            return None;
        }
        let request = Uuid::now_v7();
        state.in_flight.insert(key.clone(), request);
        Some(DiscoveryFetch {
            key,
            request,
            denied: 0,
        })
    }

    async fn finish(&self, fetch: &DiscoveryFetch, items: Option<Vec<T>>) -> bool {
        let mut state = self.0.lock().await;
        if state.in_flight.get(&fetch.key) != Some(&fetch.request) {
            return false;
        }
        state.in_flight.remove(&fetch.key);
        if let Some(items) = items {
            state.failed.remove(&fetch.key);
            let changed = state
                .entries
                .get(&fetch.key)
                .is_none_or(|previous| previous.items != items || previous.denied != fetch.denied);
            if state.entries.len() >= MAX_CACHE_ENTRIES_PER_SURFACE
                && !state.entries.contains_key(&fetch.key)
            {
                state.entries.pop_first();
            }
            state.entries.insert(
                fetch.key.clone(),
                CachedItems {
                    items,
                    denied: fetch.denied,
                },
            );
            changed
        } else {
            if state.failed.len() >= MAX_CACHE_ENTRIES_PER_SURFACE
                && !state.failed.contains_key(&fetch.key)
            {
                state.failed.pop_first();
            }
            state.failed.insert(
                fetch.key.clone(),
                Instant::now() + Duration::from_millis(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            );
            false
        }
    }

    async fn invalidate_cached_key(&self, key: &DiscoveryCacheKey) {
        self.0.lock().await.entries.remove(key);
    }
    async fn invalidate_key(&self, key: &DiscoveryCacheKey) {
        let mut state = self.0.lock().await;
        state.entries.remove(key);
        state.in_flight.remove(key);
        state.failed.remove(key);
    }
    async fn invalidate(&self, server: &ServerSlug) {
        let mut state = self.0.lock().await;
        state.entries.retain(|key, _| &key.server != server);
        // A response captured before this event must not repopulate the cache,
        // even if a replacement request has already started for the same key.
        state.in_flight.retain(|key, _| &key.server != server);
        state.failed.retain(|key, _| &key.server != server);
    }
}

#[derive(Debug)]
pub(super) struct CatalogDiscoveryCache {
    resources: SurfaceCache<DiscoveredResource>,
    resource_templates: SurfaceCache<ResourceTemplate>,
    tools: SurfaceCache<Tool>,
    prompts: SurfaceCache<Prompt>,
    changes: broadcast::Sender<DiscoveryChange>,
    settled: Notify,
}
impl Default for CatalogDiscoveryCache {
    fn default() -> Self {
        Self {
            resources: Default::default(),
            resource_templates: Default::default(),
            tools: Default::default(),
            prompts: Default::default(),
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
            GatewayDiscoverySurface::Prompts => self.prompts.contains(key).await,
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
                    GatewayDiscoverySurface::Resources => {
                        self.resources.awaiting_initial_result(keys).await
                    }
                    GatewayDiscoverySurface::ResourceTemplates => {
                        self.resource_templates.awaiting_initial_result(keys).await
                    }
                    GatewayDiscoverySurface::Tools => {
                        self.tools.awaiting_initial_result(keys).await
                    }
                    GatewayDiscoverySurface::Prompts => {
                        self.prompts.awaiting_initial_result(keys).await
                    }
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
            GatewayDiscoverySurface::Prompts => self.prompts.pending(keys).await,
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
            GatewayDiscoverySurface::Prompts => self.prompts.begin(key, true).await,
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
            GatewayDiscoverySurface::Prompts => self.prompts.finish(&fetch, None).await,
        };
        self.settled.notify_waiters();
    }
    pub(super) async fn resources(
        &self,
        key: &DiscoveryCacheKey,
    ) -> Option<AdmittedCatalog<Resource>> {
        let admitted = self.resources.get(key).await?;
        Some(AdmittedCatalog {
            items: admitted
                .items
                .into_iter()
                .filter(|item| item.listed)
                .map(|item| item.resource)
                .collect(),
            denied: admitted.denied,
        })
    }
    pub(super) async fn resource_routes(
        &self,
        key: &DiscoveryCacheKey,
    ) -> Option<Vec<DiscoveredResource>> {
        self.resources.get(key).await.map(|admitted| admitted.items)
    }
    pub(super) async fn resource_templates(
        &self,
        key: &DiscoveryCacheKey,
    ) -> Option<AdmittedCatalog<ResourceTemplate>> {
        self.resource_templates.get(key).await
    }
    pub(super) async fn tools(&self, key: &DiscoveryCacheKey) -> Option<AdmittedCatalog<Tool>> {
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
    pub(super) async fn finish_resource_routes(
        &self,
        fetch: DiscoveryFetch,
        items: Vec<DiscoveredResource>,
    ) {
        if self.resources.finish(&fetch, Some(items)).await {
            self.publish(GatewayDiscoverySurface::Resources, fetch);
        }
        self.settled.notify_waiters();
    }
    #[cfg(test)]
    pub(super) async fn finish_resources(&self, fetch: DiscoveryFetch, items: Vec<Resource>) {
        let items = items
            .into_iter()
            .map(|resource| DiscoveredResource {
                upstream_uri: ResourceUri::new(resource.uri.clone()).unwrap(),
                resource,
                listed: true,
            })
            .collect();
        self.finish_resource_routes(fetch, items).await;
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
    pub(super) async fn prompts(&self, key: &DiscoveryCacheKey) -> Option<AdmittedCatalog<Prompt>> {
        self.prompts.get(key).await
    }
    pub(super) async fn start_prompts(&self, key: DiscoveryCacheKey) -> Option<DiscoveryFetch> {
        self.prompts.begin(key, false).await
    }
    pub(super) async fn store_prompts(
        &self,
        fetch: Option<DiscoveryFetch>,
        items: Vec<Prompt>,
        denied: u32,
    ) {
        if let Some(fetch) = fetch {
            self.prompts
                .finish(&fetch.with_denied(denied), Some(items))
                .await;
        }
    }
    pub(super) async fn invalidate_prompts(&self, server: &ServerSlug) {
        self.prompts.invalidate(server).await;
    }
    fn publish(&self, surface: GatewayDiscoverySurface, fetch: DiscoveryFetch) {
        self.settled.notify_waiters();
        let _ = self.changes.send(DiscoveryChange {
            surface,
            key: fetch.key,
        });
    }
    pub(super) async fn invalidate_cached_key(&self, key: &DiscoveryCacheKey) {
        self.resources.invalidate_cached_key(key).await;
        self.resource_templates.invalidate_cached_key(key).await;
        self.tools.invalidate_cached_key(key).await;
        self.prompts.invalidate_cached_key(key).await;
    }
    pub(super) async fn invalidate_key(&self, key: &DiscoveryCacheKey) {
        self.resources.invalidate_key(key).await;
        self.resource_templates.invalidate_key(key).await;
        self.tools.invalidate_key(key).await;
        self.prompts.invalidate_key(key).await;
        self.settled.notify_waiters();
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

/// Fails a list whose profile requires complete discovery when any server failed.
pub(super) fn enforce_complete_discovery<E>(
    surface: GatewayDiscoverySurface,
    mode: DiscoveryFailureMode,
    errors: &[(ServerSlug, E)],
) -> Result<(), McpError> {
    if mode != DiscoveryFailureMode::FailClosed || errors.is_empty() {
        return Ok(());
    }
    let mut servers = errors
        .iter()
        .map(|(server, _)| server.to_string())
        .collect::<Vec<_>>();
    servers.sort();
    let surface = match surface {
        GatewayDiscoverySurface::Resources => "resource",
        GatewayDiscoverySurface::ResourceTemplates => "resource template",
        GatewayDiscoverySurface::Tools => "tool",
        GatewayDiscoverySurface::Prompts => "prompt",
    };
    Err(crate::mcp_support::mcp_internal(format!(
        "profile requires complete {surface} discovery; unavailable servers: {}",
        servers.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail_closed_profiles_reject_incomplete_tool_and_prompt_catalogs() {
        let errors = [
            (ServerSlug::new("uav-sim").unwrap(), ()),
            (ServerSlug::new("map").unwrap(), ()),
        ];
        for surface in [
            GatewayDiscoverySurface::Tools,
            GatewayDiscoverySurface::Prompts,
        ] {
            assert!(
                enforce_complete_discovery(surface, DiscoveryFailureMode::Isolate, &errors).is_ok()
            );
        }
        let error = enforce_complete_discovery(
            GatewayDiscoverySurface::Prompts,
            DiscoveryFailureMode::FailClosed,
            &errors,
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            "profile requires complete prompt discovery; unavailable servers: map, uav-sim"
        );
    }

    #[test]
    fn isolated_prompt_failures_name_the_missing_server() {
        let results: Vec<(ServerSlug, Result<Vec<&str>, ()>)> = vec![
            (ServerSlug::new("frames").unwrap(), Ok(vec!["frame_audit"])),
            (ServerSlug::new("uav-sim").unwrap(), Err(())),
        ];
        let (prompts, degradation, errors) =
            isolate_discovery_failures(GatewayDiscoverySurface::Prompts, results);
        assert_eq!(prompts, ["frame_audit"]);
        assert_eq!(errors.len(), 1);
        assert_eq!(
            degradation.failures,
            [GatewayDiscoveryFailure {
                server: ServerSlug::new("uav-sim").unwrap(),
                surface: GatewayDiscoverySurface::Prompts,
                code: GatewayDiscoveryFailureCode::UpstreamUnavailable,
            }]
        );
    }

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
    async fn read_routes_reuse_discovery_without_crossing_authority_or_invalidation() {
        let cache = CatalogDiscoveryCache::default();
        let owner = key(1, "charts");
        let route = DiscoveredResource {
            resource: Resource::new("ui://charts/chart.html", "chart"),
            upstream_uri: ResourceUri::new("ui://vendor/chart.html").unwrap(),
            listed: true,
        };
        let fetch = begin(&cache, owner.clone()).await;
        cache
            .finish_resource_routes(fetch, vec![route.clone()])
            .await;
        assert_eq!(
            cache
                .resources(&owner)
                .await
                .map(|catalog| catalog.items)
                .unwrap(),
            vec![route.resource.clone()]
        );
        for _ in 0..2 {
            assert_eq!(
                cache.resource_routes(&owner).await.unwrap(),
                vec![route.clone()]
            );
        }
        let mut other = owner.clone();
        other.principal = PrincipalId::new("other").unwrap();
        assert!(cache.resource_routes(&other).await.is_none());
        other = owner.clone();
        other.authorization_fingerprint = [8; 32];
        assert!(cache.resource_routes(&other).await.is_none());
        other = owner.clone();
        other.catalog_generation += 1;
        assert!(cache.resource_routes(&other).await.is_none());
        cache.invalidate_resource_surfaces(&owner.server).await;
        assert!(cache.resource_routes(&owner).await.is_none());

        // Listing and reading have distinct policy actions. Retain the private
        // route for a separately authorized read without exposing its descriptor.
        let fetch = begin(&cache, owner.clone()).await;
        let mut hidden = route;
        hidden.listed = false;
        cache
            .finish_resource_routes(fetch, vec![hidden.clone()])
            .await;
        assert!(
            cache
                .resources(&owner)
                .await
                .map(|catalog| catalog.items)
                .unwrap()
                .is_empty()
        );
        assert_eq!(cache.resource_routes(&owner).await.unwrap(), vec![hidden]);
    }

    #[tokio::test]
    async fn unavailable_sources_retry_without_delaying_healthy_catalogs() {
        let cache = CatalogDiscoveryCache::default();
        let failed = key(1, "offline");
        let fetch = begin(&cache, failed.clone()).await;
        cache
            .finish_failure(GatewayDiscoverySurface::Resources, fetch)
            .await;
        assert!(
            cache
                .begin(GatewayDiscoverySurface::Resources, failed.clone())
                .await
                .is_none()
        );
        assert_eq!(
            cache
                .missing_code(GatewayDiscoverySurface::Resources, &failed)
                .await,
            GatewayDiscoveryFailureCode::UpstreamUnavailable
        );
        *cache
            .resources
            .0
            .lock()
            .await
            .failed
            .get_mut(&failed)
            .unwrap() = Instant::now();
        let retry = begin(&cache, failed.clone()).await;
        tokio::time::timeout(
            Duration::from_millis(50),
            cache.settle(
                GatewayDiscoverySurface::Resources,
                std::slice::from_ref(&failed),
            ),
        )
        .await
        .unwrap();
        assert!(cache.resources.pending(std::slice::from_ref(&failed)).await);
        let mut changes = cache.subscribe();
        let items = vec![Resource::new("offline://recovered", "recovered")];
        cache.finish_resources(retry, items.clone()).await;
        assert_eq!(
            cache.resources(&failed).await.map(|catalog| catalog.items),
            Some(items)
        );
        assert!(changes.try_recv().is_ok());
        assert!(!cache.resources.0.lock().await.failed.contains_key(&failed));
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

        let refresh = begin(&cache, key.clone()).await;
        cache.finish_resources(refresh, items.clone()).await;
        assert!(matches!(
            updates.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert_eq!(
            cache.resources(&key).await.map(|catalog| catalog.items),
            Some(items.clone())
        );

        cache.invalidate_resource_surfaces(&key.server).await;
        assert!(
            cache
                .resources(&key)
                .await
                .map(|catalog| catalog.items)
                .is_none(),
            "invalidated permissions are never served"
        );
        let recovery = begin(&cache, key.clone()).await;
        cache.finish_resources(recovery, items.clone()).await;
        assert!(
            updates.try_recv().is_ok(),
            "an observed unavailable source must recover"
        );

        cache.invalidate_resource_surfaces(&key.server).await;
        assert!(
            cache
                .resources(&key)
                .await
                .map(|catalog| catalog.items)
                .is_none()
        );
        let refreshed = begin(&cache, key.clone()).await;
        cache.finish_resources(refreshed, items).await;
        assert!(updates.try_recv().is_ok());
    }

    #[tokio::test]
    async fn cold_and_invalidated_catalogs_settle_before_the_first_snapshot() {
        let cache = std::sync::Arc::new(CatalogDiscoveryCache::default());
        let cache_key = key(1, "ready");
        for version in 0..2 {
            cache.invalidate_resource_surfaces(&cache_key.server).await;
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
                cache
                    .resources(&cache_key)
                    .await
                    .map(|catalog| catalog.items)
                    .unwrap()[0]
                    .uri,
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
        assert!(
            cache
                .resources(&key(1, "one"))
                .await
                .map(|catalog| catalog.items)
                .is_none()
        );
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
            cache
                .resources(&key(2, "two"))
                .await
                .map(|catalog| catalog.items)
                .unwrap()[0]
                .uri,
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
        assert!(
            cache
                .resources(&key(1, "one"))
                .await
                .map(|catalog| catalog.items)
                .is_none()
        );
        assert!(
            cache
                .resources(&key(1, "two"))
                .await
                .map(|catalog| catalog.items)
                .is_some()
        );
        cache
            .finish_resources(current, vec![Resource::new("one://new", "new")])
            .await;
        assert_eq!(
            cache
                .resources(&key(1, "one"))
                .await
                .map(|catalog| catalog.items)
                .unwrap()[0]
                .uri,
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
        assert!(
            cache
                .tools(&key(1, "one"))
                .await
                .map(|catalog| catalog.items)
                .is_none()
        );
        cache
            .store_tools(
                current,
                vec![Tool::new("current", "new", std::sync::Arc::default())],
            )
            .await;
        assert_eq!(
            cache
                .tools(&key(1, "one"))
                .await
                .map(|catalog| catalog.items)
                .unwrap()[0]
                .name,
            "current"
        );
    }

    #[tokio::test]
    async fn disconnected_watch_requires_fresh_discovery_after_missed_notifications() {
        let cache = CatalogDiscoveryCache::default();
        let fetch = begin(&cache, key(1, "one")).await;
        cache
            .finish_resources(fetch, vec![Resource::new("one://old", "old")])
            .await;
        // A disconnected watcher invalidates every surface for its authority key.
        cache.invalidate_key(&key(1, "one")).await;
        assert!(
            cache
                .resources(&key(1, "one"))
                .await
                .map(|catalog| catalog.items)
                .is_none()
        );
        let fetch = begin(&cache, key(1, "one")).await;
        cache
            .finish_resources(fetch, vec![Resource::new("one://new", "new")])
            .await;
        assert_eq!(
            cache
                .resources(&key(1, "one"))
                .await
                .map(|catalog| catalog.items)
                .unwrap()[0]
                .uri,
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
        assert!(
            cache
                .resources(&key(1, "hung"))
                .await
                .map(|catalog| catalog.items)
                .is_none()
        );
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
