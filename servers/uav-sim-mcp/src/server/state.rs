use std::sync::Arc;

use veoveo_mcp_contract::SubscriptionHub;
use veoveo_task_runtime::TaskRuntime;

use crate::adapter::Adapter;

use super::live_stream::LiveStreamService;

pub(super) struct AppState {
    pub(super) adapter: Arc<Adapter>,
    pub(super) tasks: TaskRuntime,
    pub(super) subscribers: SubscriptionHub,
    pub(super) live_streams: LiveStreamService,
    pub(super) live_stream_connect_origin: String,
}
