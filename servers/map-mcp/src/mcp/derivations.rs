//! Derivation resource pages and SQL completion under the admitted Work Context.
use super::*;
use crate::catalog::MapAccessContext;
use crate::persistence::MapDerivationKind;

impl MapMcp {
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
