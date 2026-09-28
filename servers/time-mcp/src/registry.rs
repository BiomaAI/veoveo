use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use tokio::sync::RwLock;
use veoveo_platform_store::TenantId;

use crate::{
    authority::{AuthorityContext, LeapSecondTable},
    catalog::{TimeAccessContext, TimeCatalog},
    contract::{
        AuthorityDatasetKind, AuthorityRelease, AuthorityReleaseId, EffectiveTimeAuthority,
        TimeAuthorityReference,
    },
    engine::TemporalEngine,
};

#[cfg(test)]
mod tests;

const AUTHORITY_LOAD_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct AuthorityRegistry {
    bootstrap: AuthorityContext,
    bootstrap_tzdb: PathBuf,
    bootstrap_leaps: PathBuf,
    cache: Arc<RwLock<AuthorityCache>>,
}

#[derive(Default)]
struct AuthorityCache {
    // An invalidation replaces the token. A loader started before it may finish
    // its request, but cannot repopulate the invalidated cache.
    generation: Arc<()>,
    tenants: BTreeMap<TenantId, CachedAuthority>,
}

struct CachedAuthority {
    selection: AuthoritySelection,
    authority: AuthorityContext,
}

#[derive(Default)]
struct AuthorityPair {
    tzdb: Option<AuthorityRelease>,
    leap_seconds: Option<AuthorityRelease>,
}

impl AuthorityPair {
    fn from_releases(releases: Vec<AuthorityRelease>) -> Result<Self> {
        let mut pair = Self::default();
        for release in releases {
            if pair.replace(release).is_some() {
                bail!("active authority contains more than one release for a family");
            }
        }
        Ok(pair)
    }

    fn replace(&mut self, release: AuthorityRelease) -> Option<AuthorityRelease> {
        match release.dataset_kind {
            AuthorityDatasetKind::Tzdb => self.tzdb.replace(release),
            AuthorityDatasetKind::LeapSeconds => self.leap_seconds.replace(release),
        }
    }
}

/// All inputs that affect the loaded context, including acquisition provenance.
#[derive(Clone, PartialEq, Eq)]
struct AuthoritySelection {
    effective: EffectiveTimeAuthority,
    tzdb_path: PathBuf,
    leap_path: PathBuf,
}

impl AuthoritySelection {
    async fn load(&self) -> Result<AuthorityContext> {
        let leaps = LeapSecondTable::from_path(&self.leap_path).await?;
        let effective = self.effective.clone();
        let tzdb = self.tzdb_path.clone();
        tokio::task::spawn_blocking(move || AuthorityContext::from_paths(effective, tzdb, leaps))
            .await
            .context("loading temporal authority files")?
    }
}

impl AuthorityRegistry {
    pub fn new(
        bootstrap: AuthorityContext,
        bootstrap_tzdb: PathBuf,
        bootstrap_leaps: PathBuf,
    ) -> Self {
        Self {
            bootstrap,
            bootstrap_tzdb,
            bootstrap_leaps,
            cache: Arc::default(),
        }
    }

    /// Each request validates the current catalog selection before cache reuse.
    /// Returned engines have separate mission-epoch state.
    pub async fn authority_engine(
        &self,
        catalog: &TimeCatalog,
        scope: &TimeAccessContext,
    ) -> Result<TemporalEngine> {
        let result =
            tokio::time::timeout(AUTHORITY_LOAD_TIMEOUT, self.engine_inner(catalog, scope))
                .await
                .map_err(|_| anyhow::anyhow!("loading temporal authority exceeded 30 seconds"))
                .and_then(|result| result);
        if result.is_err() {
            self.invalidate_tenant(scope.identity.tenant_id).await;
        }
        result
    }

    async fn engine_inner(
        &self,
        catalog: &TimeCatalog,
        scope: &TimeAccessContext,
    ) -> Result<TemporalEngine> {
        let generation = self.cache.read().await.generation.clone();
        let pair = AuthorityPair::from_releases(catalog.active_releases(scope).await?)?;
        let selection = self.selection(catalog, scope, pair).await?;
        let tenant = scope.identity.tenant_id;
        if let Some(cached) = self.cache.read().await.tenants.get(&tenant)
            && cached.selection == selection
        {
            return Ok(TemporalEngine::new(cached.authority.clone()));
        }
        let authority = selection
            .load()
            .await
            .context("loading activated temporal authority")?;
        let engine = TemporalEngine::new(authority.clone());
        let mut cache = self.cache.write().await;
        if Arc::ptr_eq(&generation, &cache.generation) {
            cache.tenants.insert(
                tenant,
                CachedAuthority {
                    selection,
                    authority,
                },
            );
        }
        Ok(engine)
    }

    /// Shared LIVE/reconciliation signals carry no tenant identity; evict the
    /// process cache. Requests still validate catalog state while delivery lags.
    pub async fn invalidate(&self) {
        *self.cache.write().await = AuthorityCache::default();
    }

    async fn invalidate_tenant(&self, tenant: TenantId) {
        let mut cache = self.cache.write().await;
        cache.tenants.remove(&tenant);
        cache.generation = Arc::default();
    }

    pub async fn reload(
        &self,
        catalog: &TimeCatalog,
        scope: &TimeAccessContext,
    ) -> Result<TemporalEngine> {
        self.invalidate_tenant(scope.identity.tenant_id).await;
        self.authority_engine(catalog, scope).await
    }

    async fn selection(
        &self,
        catalog: &TimeCatalog,
        scope: &TimeAccessContext,
        pair: AuthorityPair,
    ) -> Result<AuthoritySelection> {
        Ok(AuthoritySelection {
            tzdb_path: pair.tzdb.as_ref().map_or_else(
                || self.bootstrap_tzdb.clone(),
                |release| release.artifact_path.clone().into(),
            ),
            leap_path: pair.leap_seconds.as_ref().map_or_else(
                || self.bootstrap_leaps.clone(),
                |release| release.artifact_path.clone().into(),
            ),
            effective: EffectiveTimeAuthority {
                tzdb: match pair.tzdb {
                    Some(release) => catalog.authority_reference(scope, &release).await?,
                    None => self.bootstrap.effective.tzdb.clone(),
                },
                leap_seconds: match pair.leap_seconds {
                    Some(release) => catalog.authority_reference(scope, &release).await?,
                    None => self.bootstrap.effective.leap_seconds.clone(),
                },
            },
        })
    }

    pub async fn preflight_activation(
        &self,
        catalog: &TimeCatalog,
        scope: &TimeAccessContext,
        candidate: &AuthorityRelease,
    ) -> Result<()> {
        tokio::time::timeout(AUTHORITY_LOAD_TIMEOUT, async {
            let mut pair = AuthorityPair::from_releases(catalog.active_releases(scope).await?)?;
            pair.replace(candidate.clone());
            self.selection(catalog, scope, pair)
                .await?
                .load()
                .await
                .context("preflighting temporal authority activation")?;
            Ok(())
        })
        .await
        .map_err(|_| anyhow::anyhow!("temporal authority preflight exceeded 30 seconds"))?
    }

    /// Immutable packaged metadata does not require loading the tenant engine.
    pub fn bootstrap_reference(&self, id: &AuthorityReleaseId) -> Option<TimeAuthorityReference> {
        [
            &self.bootstrap.effective.tzdb,
            &self.bootstrap.effective.leap_seconds,
        ]
        .into_iter()
        .find(|reference| &reference.release_id == id)
        .cloned()
    }
}
