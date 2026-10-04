//! Native intent recovery; controller claim heartbeats do not wake their owner.
use futures::StreamExt;
use tokio::sync::watch;
use veoveo_platform_store::{
    ChangefeedConsumerId, ChangefeedDelivery, PlatformStore, PlatformTable,
};

pub async fn observe_intents(store: PlatformStore, changed: watch::Sender<u64>) {
    let replica = std::env::var("HOSTNAME").unwrap_or_else(|_| "local".into());
    let consumer = ChangefeedConsumerId::new(format!("agent-manager/{replica}"))
        .expect("manager replica must be a valid consumer identity");
    let cursor = store
        .changefeed_checkpoint(&consumer)
        .await
        .unwrap_or_default();
    let mut source = store.observe_changes(
        vec![
            veoveo_modules::ObservationTable::from(
                veoveo_agent_runtime::AgentObservationTable::ManagedAgent,
            ),
            veoveo_modules::ObservationTable::from(
                veoveo_agent_runtime::AgentObservationTable::AgentDefinition,
            ),
            veoveo_modules::ObservationTable::from(
                veoveo_agent_runtime::AgentObservationTable::Agent,
            ),
            veoveo_modules::ObservationTable::from(
                veoveo_agent_runtime::AgentObservationTable::AgentEpisode,
            ),
            veoveo_modules::ObservationTable::from(PlatformTable::Principal),
            veoveo_modules::ObservationTable::from(PlatformTable::Tenant),
            veoveo_modules::ObservationTable::from(PlatformTable::WorkContext),
        ],
        cursor,
    );
    while let Some(delivery) = source.next().await {
        match delivery {
            Ok(delivery) => {
                let relevant = match &delivery {
                    ChangefeedDelivery::Reconcile { .. } => true,
                    ChangefeedDelivery::Changes { entries, .. } => !entries.is_empty(),
                };
                if relevant {
                    changed.send_modify(|version| *version = version.wrapping_add(1));
                }
                if let Err(error) = store.checkpoint_changes(&consumer, delivery.cursor()).await {
                    tracing::warn!(%error, "managed intent checkpoint unavailable");
                }
            }
            Err(error) => tracing::warn!(%error, "managed intent stream recovering"),
        }
    }
    tracing::error!("managed intent source ended");
}
