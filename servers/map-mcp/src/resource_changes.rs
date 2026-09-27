//! Store-backed resource invalidation shared by Map replicas.
use futures::StreamExt;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::SubscriptionHub;
use veoveo_platform_store::{PlatformStore, PlatformTable};

pub(crate) const TABLES: &[PlatformTable] = &[
    PlatformTable::MapSource,
    PlatformTable::MapDatasetRelease,
    PlatformTable::MapActiveRelease,
    PlatformTable::MapMobilityProfile,
    PlatformTable::MapRestriction,
    PlatformTable::MapOperationalSnapshot,
    PlatformTable::MapRoute,
    PlatformTable::MapRouteDependency,
    PlatformTable::MapRouteMatrix,
    PlatformTable::MapAcquisition,
    PlatformTable::MapDerivation,
    PlatformTable::Task,
    PlatformTable::MapFeatureLayer,
    PlatformTable::MapFeatureSchemaRevision,
    PlatformTable::MapStyleRevision,
    PlatformTable::MapFeatureHead,
    PlatformTable::MapFeatureRevision,
    PlatformTable::MapFeatureChangeset,
    PlatformTable::MapLayerPublication,
    PlatformTable::MapLayerProduct,
    PlatformTable::MapComposition,
    PlatformTable::MapCompositionRevision,
];

pub(crate) async fn observe(
    store: PlatformStore,
    hub: Arc<SubscriptionHub>,
    stop: CancellationToken,
) {
    let mut changes = store.resource_changes(TABLES.to_vec());
    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            change = changes.next() => {
                if change.is_none() { break; }
                // A feature read reconciles its replica's DuckDB projection through
                // the committed Map head before it answers. No local row is authority.
                hub.notify_resource_contents_changed().await;
            }
        }
    }
}
