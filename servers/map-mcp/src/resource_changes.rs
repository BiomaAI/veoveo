//! Store-backed resource invalidation shared by Map replicas.
use futures::StreamExt;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::SubscriptionHub;
use veoveo_platform_store::{PlatformStore, PlatformTable};

pub(crate) const TABLES: &[crate::MapObservationTable] = &[
    crate::MapObservationTable::MapSource,
    crate::MapObservationTable::MapDatasetRelease,
    crate::MapObservationTable::MapActiveRelease,
    crate::MapObservationTable::MapMobilityProfile,
    crate::MapObservationTable::MapRestriction,
    crate::MapObservationTable::MapOperationalSnapshot,
    crate::MapObservationTable::MapRoute,
    crate::MapObservationTable::MapRouteDependency,
    crate::MapObservationTable::MapRouteMatrix,
    crate::MapObservationTable::MapAcquisition,
    crate::MapObservationTable::MapDerivation,
    crate::MapObservationTable::MapFeatureLayer,
    crate::MapObservationTable::MapFeatureSchemaRevision,
    crate::MapObservationTable::MapStyleRevision,
    crate::MapObservationTable::MapFeatureHead,
    crate::MapObservationTable::MapFeatureRevision,
    crate::MapObservationTable::MapFeatureChangeset,
    crate::MapObservationTable::MapLayerPublication,
    crate::MapObservationTable::MapLayerProduct,
    crate::MapObservationTable::MapComposition,
    crate::MapObservationTable::MapCompositionRevision,
];

pub(crate) async fn observe(
    store: PlatformStore,
    hub: Arc<SubscriptionHub>,
    stop: CancellationToken,
) {
    let mut tables: Vec<veoveo_modules::ObservationTable> =
        TABLES.iter().copied().map(Into::into).collect();
    tables.push(PlatformTable::Task.into());
    let mut changes = store.resource_changes(tables);
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
