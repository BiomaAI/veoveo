//! Typed operational catalog pages selected under current SQL authority.
use super::*;
use crate::contract::MapCatalogPage;
use crate::derivations::DerivationSelection;

impl MapMcp {
    pub(super) async fn read_catalog_page(
        &self,
        page: MapCatalogPage,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(
            context,
            match &page {
                MapCatalogPage::Acquisitions { .. } => MapScope::Admin,
                _ => MapScope::DatasetRead,
            },
        )?;
        if matches!(&page, MapCatalogPage::SpatialDerivations { .. }) {
            require_scope(context, MapScope::SpatialDerive)?;
        }
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match page {
            MapCatalogPage::Routes { after } => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .routes_page(&scope, after.as_ref())
                    .await
                    .map_err(internal)?,
            ),
            MapCatalogPage::Matrices { after } => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .matrices_page(&scope, after.as_ref())
                    .await
                    .map_err(internal)?,
            ),
            MapCatalogPage::Acquisitions { after } => {
                if after.is_none() {
                    self.state
                        .acquisitions
                        .reconcile_interrupted(&scope)
                        .await
                        .map_err(internal)?;
                }
                json_read(
                    uri,
                    &self
                        .state
                        .catalog
                        .acquisitions_page(&scope, after.as_ref())
                        .await
                        .map_err(internal)?,
                )
            }
            MapCatalogPage::Releases { dataset, after } => {
                let page = self
                    .state
                    .catalog
                    .releases_page(&scope, dataset.as_ref(), after.as_ref())
                    .await
                    .map_err(internal)?;
                if dataset.is_some() && after.is_none() && page.items.is_empty() {
                    return Err(not_found("dataset"));
                }
                json_read(uri, &page)
            }
            MapCatalogPage::RasterDerivations { after } => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .derivations_page(
                        &scope,
                        &identity.authority.work_context,
                        DerivationSelection::Raster(after.as_ref()),
                    )
                    .await
                    .map_err(internal)?,
            ),
            MapCatalogPage::SpatialDerivations { after } => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .derivations_page(
                        &scope,
                        &identity.authority.work_context,
                        DerivationSelection::Spatial(after.as_ref()),
                    )
                    .await
                    .map_err(internal)?,
            ),
        }
    }
}
