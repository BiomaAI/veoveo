//! Shared domain invalidation sources. Content still requires an authorized read.
use crate::{ChangefeedCursor, PlatformStore, PlatformTable, StoreError, decode_changefeed_entry};
use futures::{StreamExt, stream::BoxStream};
use std::time::Duration;
use surrealdb::{
    Notification,
    types::{RecordId, SurrealValue},
};

/// Closed native-LIVE sources for domain invalidation. Recoverable consumers
/// replay each table through its native changefeed.
#[derive(Clone, Copy)]
pub enum ResourceChangeTable {
    Platform(PlatformTable),
    AgentDefinition,
    ManagedAgent,
    WorkContext,
    UavVehicleControlGrant,
    UavVehicleMissionPlan,
    KnowledgeSync,
    KnowledgeCoordinator,
}
impl From<PlatformTable> for ResourceChangeTable {
    fn from(table: PlatformTable) -> Self {
        Self::Platform(table)
    }
}
impl ResourceChangeTable {
    fn as_str(self) -> &'static str {
        match self {
            Self::Platform(table) => table.as_str(),
            Self::AgentDefinition => "agent_definition",
            Self::ManagedAgent => "managed_agent",
            Self::WorkContext => "work_context",
            Self::UavVehicleControlGrant => "uav_vehicle_control_grant",
            Self::UavVehicleMissionPlan => "uav_vehicle_mission_plan",
            Self::KnowledgeSync => "knowledge_sync",
            Self::KnowledgeCoordinator => "knowledge_coordinator",
        }
    }
}

#[derive(SurrealValue)]
struct Identity {
    id: RecordId,
}

/// Why callers must reread. The signal never includes a record or its identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceInvalidation {
    Reconcile,
    Live,
    Changefeed,
}

impl PlatformStore {
    /// A process shares this projected source across its MCP resource listeners.
    /// Reconnect replays native changefeeds from the last database-clock anchor
    /// before invalidating readers. The invalidation also covers expired history
    /// and registry tables without changefeeds. No record content or identity is
    /// exposed to the caller; every wake means re-read.
    pub fn resource_changes<T: Into<ResourceChangeTable>>(
        &self,
        tables: Vec<T>,
    ) -> BoxStream<'static, ResourceInvalidation> {
        let tables: Vec<ResourceChangeTable> = tables.into_iter().map(Into::into).collect();
        let store = self.clone();
        let query = tables
            .iter()
            .map(|table| format!("LIVE SELECT id FROM {};", table.as_str()))
            .collect::<String>();
        Box::pin(async_stream::stream! {
            if tables.is_empty() { return; }
            let mut cursor = None;
            loop {
                let connect = async {
                    // Establish the overlap before registering LIVE. Mutations
                    // after this anchor are either replayed or queued by LIVE.
                    let anchor = store.changefeed_cursor_now().await?;
                    let mut response = store.client().query(query.clone()).await?.check()?;
                    let stream = response.stream::<Notification<Identity>>(())?;
                    let invalidation = if let Some(cursor) = cursor
                        && store.replay_resource_invalidation(cursor, anchor, &tables).await?
                    {
                        ResourceInvalidation::Changefeed
                    } else {
                        ResourceInvalidation::Reconcile
                    };
                    Ok::<_, StoreError>((stream, anchor, invalidation))
                };
                let (mut stream, anchor, invalidation) = match connect.await {
                    Ok(connected) => connected,
                    Err(_) => { tokio::time::sleep(Duration::from_secs(1)).await; continue; }
                };
                cursor = Some(anchor);
                yield invalidation;
                loop {
                    tokio::select! {
                        event = stream.next() => match event {
                            Some(Ok(event)) => {
                                let _ = event.data.id;
                                // Advance only on an observed write, never on an
                                // idle timer. Keep an overlap for in-flight commits.
                                let Ok(anchor) = store.changefeed_cursor_now().await else { break; };
                                // Coalesce writes without retaining changed records.
                                let delay = tokio::time::sleep(Duration::from_millis(100));
                                tokio::pin!(delay);
                                loop {
                                    tokio::select! {
                                        _ = &mut delay => break,
                                        next = stream.next() => if !matches!(next, Some(Ok(_))) { break; },
                                    }
                                }
                                cursor = Some(anchor);
                                yield ResourceInvalidation::Live;
                            }
                            _ => break,
                        },
                    }
                }
            }
        })
    }

    /// A matching mutation invalidates the whole caller-visible resource set, so
    /// one match completes recovery. Unrelated pages advance to the reconnect
    /// anchor instead of following an indefinitely growing database tail.
    async fn replay_resource_invalidation(
        &self,
        mut cursor: ChangefeedCursor,
        until: ChangefeedCursor,
        tables: &[ResourceChangeTable],
    ) -> Result<bool, StoreError> {
        while cursor <= until {
            let batches = self.replay_changes(cursor, 1_000).await?;
            let Some(last) = batches.last() else {
                break;
            };
            let next = ChangefeedCursor::from_versionstamp(last.versionstamp.saturating_add(1))
                .filter(|next| *next > cursor)
                .ok_or(StoreError::InvalidChangefeedEntry {
                    reason: "resource recovery cursor did not advance",
                })?;
            for batch in batches {
                for change in batch.changes {
                    if decode_changefeed_entry(&change)?
                        .table()
                        .is_some_and(|name| tables.iter().any(|table| table.as_str() == name))
                    {
                        return Ok(true);
                    }
                }
            }
            cursor = next;
        }
        Ok(false)
    }
}
