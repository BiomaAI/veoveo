//! Derivation resource pages and SQL completion under the admitted Work Context.
use super::*;
use crate::catalog::MapAccessContext;
use veoveo_platform_store::MapDerivationKind;

impl MapMcp {
    pub(super) async fn read_derivation_resource(
        &self,
        uri: &str,
        identity: &GatewayInternalIdentity,
        scope: &MapAccessContext,
    ) -> Result<Option<ReadResourceResult>, McpError> {
        let (root, query) = uri
            .split_once('?')
            .map_or((uri, None), |(root, query)| (root, Some(query)));
        let kind = match root {
            uris::RASTER_DERIVATIONS_URI => Some(MapDerivationKind::Raster),
            uris::SPATIAL_DERIVATIONS_URI => Some(MapDerivationKind::Spatial),
            _ => None,
        };
        if let Some(kind) = kind {
            spatial_scope(identity, kind)?;
            let cursor = query
                .map(|query| {
                    query
                        .strip_prefix("cursor=")
                        .ok_or_else(|| invalid_params("expected one derivation cursor"))
                })
                .transpose()?;
            let after = crate::derivations::parse_cursor(kind, cursor).map_err(invalid_params)?;
            let page = self
                .state
                .catalog
                .derivations_page(
                    scope,
                    &identity.authority.work_context,
                    kind,
                    after.as_deref(),
                )
                .await
                .map_err(internal)?;
            return json_resource(uri, &page).map(Some);
        }
        if let Ok(address) = crate::contract::MapRasterDerivationUri::parse(uri) {
            let value = self
                .state
                .catalog
                .raster_derivation(scope, &identity.authority.work_context, address.id())
                .await
                .map_err(internal)?
                .ok_or_else(|| not_found("raster derivation"))?;
            return json_resource(uri, &value).map(Some);
        }
        if let Ok(address) = crate::contract::MapSpatialDerivationUri::parse(uri) {
            spatial_scope(identity, MapDerivationKind::Spatial)?;
            let value = self
                .state
                .catalog
                .spatial_derivation(scope, &identity.authority.work_context, address.id())
                .await
                .map_err(internal)?
                .ok_or_else(|| not_found("spatial derivation"))?;
            return json_resource(uri, &value).map(Some);
        }
        Ok(None)
    }

    pub(super) async fn complete_derivation(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapAccessContext,
        template: &str,
        name: &str,
        needle: &str,
    ) -> Result<Option<CompleteResult>, McpError> {
        let kind = match (template, name) {
            (uris::RASTER_DERIVATION_TEMPLATE, "raster_derivation_id") => MapDerivationKind::Raster,
            (uris::SPATIAL_DERIVATION_TEMPLATE, "spatial_derivation_id") => {
                MapDerivationKind::Spatial
            }
            _ => return Ok(None),
        };
        spatial_scope(identity, kind)?;
        let mut values = self
            .state
            .catalog
            .complete_derivations(scope, &identity.authority.work_context, kind, needle)
            .await
            .map_err(internal)?;
        let more = values.len() > crate::derivations::PAGE_SIZE;
        values.truncate(crate::derivations::PAGE_SIZE);
        let total = (!more).then_some(values.len() as u32);
        Ok(Some(CompleteResult::new(
            CompletionInfo::with_pagination(values, total, more).map_err(internal)?,
        )))
    }
}
fn spatial_scope(
    identity: &GatewayInternalIdentity,
    kind: MapDerivationKind,
) -> Result<(), McpError> {
    if kind == MapDerivationKind::Spatial {
        crate::server::auth::require_scope(&identity.actor.scopes, MapScope::SpatialDerive)?;
    }
    Ok(())
}
