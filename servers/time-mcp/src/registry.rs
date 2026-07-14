use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use tokio::sync::RwLock;

use crate::{
    authority::{AuthorityContext, LeapSecondTable},
    catalog::{TimeCatalog, TimeScope},
    contract::{AuthorityDatasetKind, AuthorityReleaseId},
    engine::TemporalEngine,
};

#[derive(Clone)]
pub struct AuthorityRegistry {
    bootstrap: AuthorityContext,
    bootstrap_tzdb: PathBuf,
    bootstrap_leaps: PathBuf,
    tenants: Arc<RwLock<BTreeMap<String, TemporalEngine>>>,
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
            tenants: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub async fn engine(&self, catalog: &TimeCatalog, scope: &TimeScope) -> Result<TemporalEngine> {
        let key = scope.tenant_key();
        let engine = if let Some(engine) = self.tenants.read().await.get(&key).cloned() {
            engine
        } else {
            let engine = TemporalEngine::new(self.bootstrap.clone());
            self.tenants
                .write()
                .await
                .insert(key.clone(), engine.clone());
            engine
        };
        engine.replace_epochs(catalog.list_epochs(scope).await?);
        Ok(engine)
    }

    pub async fn reload(&self, catalog: &TimeCatalog, scope: &TimeScope) -> Result<TemporalEngine> {
        let active = catalog.active_releases(scope).await?;
        let tzdb_release = active
            .iter()
            .find(|release| release.dataset_kind == AuthorityDatasetKind::Tzdb);
        let leap_release = active
            .iter()
            .find(|release| release.dataset_kind == AuthorityDatasetKind::LeapSeconds);
        let tzdb_path = tzdb_release.map_or(self.bootstrap_tzdb.as_path(), |release| {
            std::path::Path::new(&release.artifact_path)
        });
        let leap_path = leap_release.map_or(self.bootstrap_leaps.as_path(), |release| {
            std::path::Path::new(&release.artifact_path)
        });
        let leaps = LeapSecondTable::from_path(leap_path).await?;
        let authority = AuthorityContext::from_paths(
            tzdb_release.map_or_else(
                || self.bootstrap.binding.tzdb_release_id.clone(),
                |release| release.release_id.clone(),
            ),
            leap_release.map_or_else(
                || self.bootstrap.binding.leap_seconds_release_id.clone(),
                |release| release.release_id.clone(),
            ),
            tzdb_path,
            leaps,
        )
        .context("loading activated temporal authority")?;
        let engine = TemporalEngine::new(authority);
        engine.replace_epochs(catalog.list_epochs(scope).await?);
        self.tenants
            .write()
            .await
            .insert(scope.tenant_key(), engine.clone());
        Ok(engine)
    }

    pub async fn preflight_activation(
        &self,
        catalog: &TimeCatalog,
        scope: &TimeScope,
        candidate: &crate::contract::AuthorityRelease,
    ) -> Result<()> {
        let active = catalog.active_releases(scope).await?;
        let active_tzdb = active
            .iter()
            .find(|release| release.dataset_kind == AuthorityDatasetKind::Tzdb);
        let active_leaps = active
            .iter()
            .find(|release| release.dataset_kind == AuthorityDatasetKind::LeapSeconds);
        let tzdb = if candidate.dataset_kind == AuthorityDatasetKind::Tzdb {
            candidate
        } else {
            active_tzdb.unwrap_or(candidate)
        };
        let leaps = if candidate.dataset_kind == AuthorityDatasetKind::LeapSeconds {
            candidate
        } else {
            active_leaps.unwrap_or(candidate)
        };
        let tzdb_path =
            if candidate.dataset_kind == AuthorityDatasetKind::Tzdb || active_tzdb.is_some() {
                std::path::Path::new(&tzdb.artifact_path)
            } else {
                self.bootstrap_tzdb.as_path()
            };
        let leap_path = if candidate.dataset_kind == AuthorityDatasetKind::LeapSeconds
            || active_leaps.is_some()
        {
            std::path::Path::new(&leaps.artifact_path)
        } else {
            self.bootstrap_leaps.as_path()
        };
        let leap_table = LeapSecondTable::from_path(leap_path).await?;
        AuthorityContext::from_paths(
            if candidate.dataset_kind == AuthorityDatasetKind::Tzdb {
                candidate.release_id.clone()
            } else {
                active_tzdb.map_or_else(
                    || self.bootstrap.binding.tzdb_release_id.clone(),
                    |release| release.release_id.clone(),
                )
            },
            if candidate.dataset_kind == AuthorityDatasetKind::LeapSeconds {
                candidate.release_id.clone()
            } else {
                active_leaps.map_or_else(
                    || self.bootstrap.binding.leap_seconds_release_id.clone(),
                    |release| release.release_id.clone(),
                )
            },
            tzdb_path,
            leap_table,
        )
        .context("preflighting temporal authority activation")?;
        Ok(())
    }

    pub fn bootstrap_release_ids(&self) -> (AuthorityReleaseId, AuthorityReleaseId) {
        (
            self.bootstrap.binding.tzdb_release_id.clone(),
            self.bootstrap.binding.leap_seconds_release_id.clone(),
        )
    }
}
