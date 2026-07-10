use veoveo_mcp_contract::SubscriptionHub;
use veoveo_recording_mcp::RecordingService;

pub(super) struct AppState {
    pub(super) recordings: RecordingService,
    pub(super) subscribers: SubscriptionHub,
}
