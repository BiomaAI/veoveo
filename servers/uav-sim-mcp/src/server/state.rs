use std::sync::Arc;

use veoveo_mcp_contract::SubscriptionHub;
use veoveo_task_runtime::TaskRuntime;

use super::{live_view::LiveViewService, live_view_audit::LiveViewAudit};
use crate::adapter::Adapter;

pub(super) struct AppState {
    pub(super) adapter: Arc<Adapter>,
    pub(super) tasks: TaskRuntime,
    pub(super) subscribers: Arc<SubscriptionHub>,
    pub(super) live_views: Arc<LiveViewService>,
    pub(super) live_view_audit: Arc<LiveViewAudit>,
    pub(super) live_view_connect_origin: String,
}
