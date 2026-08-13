use std::collections::BTreeMap;

use rmcp::model::{Resource, ResourceTemplate, Tool};
use tokio::sync::Mutex;
use veoveo_mcp_contract::{
    GatewayDiscoveryDegradation, GatewayDiscoveryFailure, GatewayDiscoveryFailureCode,
    GatewayDiscoverySurface, PrincipalId, ServerSlug,
};

pub(super) const MAX_CONCURRENT_DISCOVERY: usize = 8;
const MAX_CACHE_ENTRIES_PER_SURFACE: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct DiscoveryCacheKey {
    pub(super) catalog_generation: u64,
    pub(super) principal: PrincipalId,
    pub(super) authorization_fingerprint: [u8; 32],
    pub(super) server: ServerSlug,
}

#[derive(Debug, Default)]
pub(super) struct CatalogDiscoveryCache {
    resources: Mutex<BTreeMap<DiscoveryCacheKey, Vec<Resource>>>,
    resource_templates: Mutex<BTreeMap<DiscoveryCacheKey, Vec<ResourceTemplate>>>,
    tools: Mutex<BTreeMap<DiscoveryCacheKey, Vec<Tool>>>,
}

impl CatalogDiscoveryCache {
    pub(super) async fn resources(&self, key: &DiscoveryCacheKey) -> Option<Vec<Resource>> {
        self.resources.lock().await.get(key).cloned()
    }

    pub(super) async fn store_resources(&self, key: DiscoveryCacheKey, resources: Vec<Resource>) {
        store(&self.resources, key, resources).await;
    }

    pub(super) async fn resource_templates(
        &self,
        key: &DiscoveryCacheKey,
    ) -> Option<Vec<ResourceTemplate>> {
        self.resource_templates.lock().await.get(key).cloned()
    }

    pub(super) async fn store_resource_templates(
        &self,
        key: DiscoveryCacheKey,
        templates: Vec<ResourceTemplate>,
    ) {
        store(&self.resource_templates, key, templates).await;
    }

    pub(super) async fn tools(&self, key: &DiscoveryCacheKey) -> Option<Vec<Tool>> {
        self.tools.lock().await.get(key).cloned()
    }

    pub(super) async fn store_tools(&self, key: DiscoveryCacheKey, tools: Vec<Tool>) {
        store(&self.tools, key, tools).await;
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
        cache.store_resources(key(1, "one"), Vec::new()).await;
        cache.store_resources(key(2, "two"), Vec::new()).await;
        assert!(cache.resources(&key(1, "one")).await.is_none());
        assert!(cache.resources(&key(2, "two")).await.is_some());
    }

    #[tokio::test]
    async fn list_change_invalidation_is_scoped_to_one_server() {
        let cache = CatalogDiscoveryCache::default();
        cache.store_resources(key(1, "one"), Vec::new()).await;
        cache.store_resources(key(1, "two"), Vec::new()).await;
        cache
            .invalidate_resource_surfaces(&ServerSlug::new("one").unwrap())
            .await;
        assert!(cache.resources(&key(1, "one")).await.is_none());
        assert!(cache.resources(&key(1, "two")).await.is_some());
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
