use std::sync::{Arc, Mutex};

use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::VideoStream;
use re_sdk_types::components::VideoCodec;
use tokio::sync::mpsc;
use veoveo_stream_mcp::catalog::RecordingOutputConfig;
use veoveo_stream_mcp::contract::{LiveRecordingLifecycle, LiveRecordingOutputView};

struct RecordingFrame {
    timestamp_us: u64,
    keyframe: bool,
    bytes: Vec<u8>,
}

struct RecordingState {
    lifecycle: LiveRecordingLifecycle,
    forwarded_video_frames: u64,
    error: Option<String>,
}

pub(super) struct LiveRecordingOutput {
    recording_key: String,
    config: RecordingOutputConfig,
    sender: Mutex<Option<mpsc::Sender<RecordingFrame>>>,
    state: Arc<Mutex<RecordingState>>,
}

impl LiveRecordingOutput {
    pub(super) fn start(recording_key: String, config: RecordingOutputConfig) -> Self {
        let (sender, receiver) = mpsc::channel(config.queue_capacity);
        let state = Arc::new(Mutex::new(RecordingState {
            lifecycle: LiveRecordingLifecycle::Starting,
            forwarded_video_frames: 0,
            error: None,
        }));
        tokio::task::spawn_blocking({
            let recording_key = recording_key.clone();
            let config = config.clone();
            let state = state.clone();
            move || run_worker(&recording_key, &config, receiver, &state)
        });
        Self {
            recording_key,
            config,
            sender: Mutex::new(Some(sender)),
            state,
        }
    }

    pub(super) fn try_record(&self, timestamp_us: u64, keyframe: bool, bytes: Vec<u8>) {
        if self
            .state
            .lock()
            .expect("recording state poisoned")
            .lifecycle
            == LiveRecordingLifecycle::Failed
        {
            return;
        }
        let sender = self.sender.lock().expect("recording sender poisoned");
        let Some(sender) = sender.as_ref() else {
            return;
        };
        if let Err(error) = sender.try_send(RecordingFrame {
            timestamp_us,
            keyframe,
            bytes,
        }) {
            let mut state = self.state.lock().expect("recording state poisoned");
            state.lifecycle = LiveRecordingLifecycle::Failed;
            state.error = Some(match error {
                mpsc::error::TrySendError::Full(_) => {
                    "non-blocking recording route exceeded its admitted queue".to_owned()
                }
                mpsc::error::TrySendError::Closed(_) => {
                    "recording route closed unexpectedly".to_owned()
                }
            });
        }
    }

    pub(super) fn stop(&self) {
        self.sender
            .lock()
            .expect("recording sender poisoned")
            .take();
        let mut state = self.state.lock().expect("recording state poisoned");
        if matches!(
            state.lifecycle,
            LiveRecordingLifecycle::Starting | LiveRecordingLifecycle::Forwarding
        ) {
            state.lifecycle = LiveRecordingLifecycle::Draining;
        }
    }

    pub(super) fn view(&self) -> LiveRecordingOutputView {
        let state = self.state.lock().expect("recording state poisoned");
        LiveRecordingOutputView {
            recording_key: self.recording_key.clone(),
            application_id: self.config.application_id.clone(),
            entity_path: self.config.entity_path.clone(),
            timeline: self.config.timeline.clone(),
            lifecycle: state.lifecycle,
            forwarded_video_frames: state.forwarded_video_frames,
            error: state.error.clone(),
        }
    }
}

fn run_worker(
    recording_key: &str,
    config: &RecordingOutputConfig,
    mut receiver: mpsc::Receiver<RecordingFrame>,
    state: &Mutex<RecordingState>,
) {
    let recording = match RecordingStreamBuilder::new(config.application_id.clone())
        .recording_id(recording_key.to_owned())
        .connect_grpc_opts(config.proxy_url.clone())
    {
        Ok(recording) => recording,
        Err(error) => {
            fail(
                state,
                format!("connecting the local recording route failed: {error}"),
            );
            return;
        }
    };
    {
        let mut state = state.lock().expect("recording state poisoned");
        if state.lifecycle == LiveRecordingLifecycle::Failed {
            return;
        }
        if state.lifecycle == LiveRecordingLifecycle::Draining {
            state.lifecycle = LiveRecordingLifecycle::Stopped;
            return;
        }
        state.lifecycle = LiveRecordingLifecycle::Forwarding;
    }
    let timeline = re_sdk::TimelineName::try_new(config.timeline.as_str())
        .expect("catalog validation guarantees a non-empty timeline");
    while let Some(frame) = receiver.blocking_recv() {
        recording.set_duration_secs(timeline, frame.timestamp_us as f64 / 1_000_000.0);
        let mut video = VideoStream::new(VideoCodec::H264).with_sample(frame.bytes);
        if frame.keyframe {
            video = video.with_is_keyframe(true);
        }
        if let Err(error) = recording.log(config.entity_path.clone(), &video) {
            fail(
                state,
                format!("publishing an encoded Stream frame failed: {error}"),
            );
            return;
        }
        state
            .lock()
            .expect("recording state poisoned")
            .forwarded_video_frames += 1;
    }
    let mut state = state.lock().expect("recording state poisoned");
    if state.lifecycle != LiveRecordingLifecycle::Failed {
        state.lifecycle = LiveRecordingLifecycle::Stopped;
    }
}

fn fail(state: &Mutex<RecordingState>, error: String) {
    let mut state = state.lock().expect("recording state poisoned");
    state.lifecycle = LiveRecordingLifecycle::Failed;
    state.error = Some(error);
}
