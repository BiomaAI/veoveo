//! Durable transcription; the MCP and browser projections share this domain.
use veoveo_speech_contract::TranscribeRequest;
mod access;
mod admission;
mod execution;
mod output;

use crate::process::WorkerProcess;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    ArtifactMetadata, IssuedArtifactReadCapability, IssuedArtifactWriteCapability,
};
use veoveo_task_runtime::TaskRuntime;

pub use admission::owner;

pub struct SpeechService {
    pub tasks: TaskRuntime,
    pub dictations: Arc<crate::dictation::Dictations>,
    pub artifacts: HttpArtifactPlane,
    pub worker: Arc<WorkerProcess>,
    slots: Arc<tokio::sync::Semaphore>,
    queue: Arc<tokio::sync::Semaphore>,
}

impl SpeechService {
    pub fn new(
        tasks: TaskRuntime,
        artifacts: HttpArtifactPlane,
        worker: Arc<WorkerProcess>,
        concurrent_files: usize,
        dictation_capacity: usize,
    ) -> Self {
        Self {
            tasks,
            dictations: crate::dictation::Dictations::new(worker.clone(), dictation_capacity),
            artifacts,
            worker,
            slots: Arc::new(tokio::sync::Semaphore::new(concurrent_files)),
            queue: Arc::new(tokio::sync::Semaphore::new(64)),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableRequest {
    input: TranscribeRequest,
    source: ArtifactMetadata,
    read: IssuedArtifactReadCapability,
    write: IssuedArtifactWriteCapability,
}
