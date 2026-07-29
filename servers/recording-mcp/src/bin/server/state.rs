use veoveo_mcp_contract::SubscriptionHub;
use veoveo_recording_mcp::{RecordingService, playback::PlaybackManager};

pub(super) struct AppState {
    pub(super) recordings: RecordingService,
    pub(super) playback: PlaybackManager,
    pub(super) subscribers: SubscriptionHub,
}
