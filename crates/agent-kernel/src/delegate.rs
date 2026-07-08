//! The gateway notification delegate: MCP events become wakes.
//!
//! rig consumes what it understands (task-status wakeups, tool-list refresh)
//! and forwards everything else here after its own handling. Resource updates
//! wake episodes directly; terminal task-status notifications are a
//! belt-and-braces second signal next to the watchers — the wake receiver's
//! dedup makes the double delivery harmless.

use rig_core::tool::rmcp::McpNotificationDelegate;
use rig_core::wasm_compat::WasmBoxedFuture;

use crate::wake::{WakeBus, WakeEvent};

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
            self.bus
                .send(WakeEvent::resource_updated(params.uri.as_str()))
                .await;
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
            if terminal {
                self.bus
                    .send(WakeEvent::task_settled(&params.task.task_id))
                    .await;
            }
        })
    }
}
