//! Shared domain invalidation sources. Content still requires an authorized read.
use crate::{PlatformStore, PlatformTable};
use futures::{StreamExt, stream::BoxStream};
use std::time::Duration;
use surrealdb::{
    Notification,
    types::{RecordId, SurrealValue},
};

/// Closed native-LIVE sources. Registry tables use durable outbox events rather
/// than a database changefeed; watching them does not claim changefeed support.
#[derive(Clone, Copy)]
pub enum ResourceChangeTable {
    Platform(PlatformTable),
    AgentDefinition,
    ManagedAgent,
    WorkContext,
    UavVehicleControlGrant,
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
        }
    }
}

#[derive(SurrealValue)]
struct Identity {
    id: RecordId,
}

impl PlatformStore {
    /// A process shares this projected source across its MCP resource listeners.
    /// Reconnect invalidates readers after a delivery gap. No record
    /// content or identity is exposed to the caller; every wake means re-read.
    pub fn resource_changes<T: Into<ResourceChangeTable>>(
        &self,
        tables: Vec<T>,
    ) -> BoxStream<'static, ()> {
        let tables: Vec<ResourceChangeTable> = tables.into_iter().map(Into::into).collect();
        let store = self.clone();
        let query = tables
            .iter()
            .map(|table| format!("LIVE SELECT id FROM {};", table.as_str()))
            .collect::<String>();
        Box::pin(async_stream::stream! {
            if tables.is_empty() { return; }
            loop {
                let connect = async {
                    let mut response = store.client().query(query.clone()).await?.check()?;
                    response.stream::<Notification<Identity>>(())
                };
                let mut stream = match connect.await {
                    Ok(stream) => stream,
                    Err(_) => { tokio::time::sleep(Duration::from_secs(1)).await; continue; }
                };
                yield ();
                loop {
                    tokio::select! {
                        event = stream.next() => match event {
                            Some(Ok(event)) => {
                                let _ = event.data.id;
                                // Coalesce writes without retaining changed records.
                                let delay = tokio::time::sleep(Duration::from_millis(100));
                                tokio::pin!(delay);
                                loop {
                                    tokio::select! {
                                        _ = &mut delay => break,
                                        next = stream.next() => if !matches!(next, Some(Ok(_))) { break; },
                                    }
                                }
                                yield ();
                            }
                            _ => break,
                        },
                    }
                }
            }
        })
    }
}
