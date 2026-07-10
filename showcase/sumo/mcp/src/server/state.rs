use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::SubscriptionHub;
use veoveo_task_runtime::TaskRuntime;

use crate::driver::SimDriver;
use crate::recording::RecordingPublisher;

pub(super) struct World {
    pub(super) driver: Box<dyn SimDriver>,
    pub(super) publisher: RecordingPublisher,
    pub(super) congested: bool,
}

pub(super) type SharedWorld = Arc<Mutex<World>>;

#[derive(Clone)]
pub(super) struct OfflineBinaries {
    pub(super) netgenerate: PathBuf,
    pub(super) duarouter: PathBuf,
    pub(super) tls_coordinator: PathBuf,
}

pub(super) struct AppState {
    pub(super) world: SharedWorld,
    pub(super) tasks: TaskRuntime,
    pub(super) work_dir: PathBuf,
    pub(super) binaries: OfflineBinaries,
    pub(super) subscribers: SubscriptionHub,
    pub(super) artifacts: HttpArtifactPlane,
    pub(super) max_artifact_bytes: u64,
}
