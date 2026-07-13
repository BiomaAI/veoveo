//! The gateway notification delegate: MCP events become wakes.
//!
//! rig consumes task status itself. Resource updates create durable wakes.
//! Terminal task notifications never create result wakes: only the transaction
//! that stores the terminal result may do that.

use rig_core::tool::rmcp::McpNotificationDelegate;
use rig_core::wasm_compat::WasmBoxedFuture;

use crate::wake::{WakeBus, resource_updated};

#[derive(Clone)]
pub struct KernelNotificationDelegate {
    bus: WakeBus,
}

impl KernelNotificationDelegate {
    pub fn new(bus: WakeBus) -> Self {
        Self { bus }
    }
}

impl McpNotificationDelegate for KernelNotificationDelegate {
    fn on_resource_updated(
        &self,
        params: rmcp::model::ResourceUpdatedNotificationParam,
    ) -> WasmBoxedFuture<'_, ()> {
        Box::pin(async move {
            if let Err(error) = self.bus.send(resource_updated(params.uri.as_str())).await {
                tracing::error!(%error, uri = %params.uri, "persisting resource wake failed");
            }
        })
    }

    fn on_task_status(
        &self,
        params: rmcp::model::TaskStatusNotificationParam,
    ) -> WasmBoxedFuture<'_, ()> {
        Box::pin(async move {
            let status = &params.task.status;
            let terminal = matches!(
                status,
                rmcp::model::TaskStatus::Completed
                    | rmcp::model::TaskStatus::Failed
                    | rmcp::model::TaskStatus::Cancelled
            );
            tracing::info!(
                task_id = params.task.task_id,
                status = ?status,
                terminal,
                "task status notification received"
            );
            // The watcher/subscription persists the payload before waking the
            // scheduler. A status-only notification is only a latency signal
            // for that protocol machinery.
        })
    }
}
