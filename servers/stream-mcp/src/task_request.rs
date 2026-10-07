//! Current durable request shared by producers, recovery and lookup contributions.
use crate::contract::*;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use veoveo_artifact_contract::{IssuedArtifactReadCapability, IssuedArtifactWriteCapability};
use veoveo_task_runtime::RecoveryClass;
use veoveo_types::TaskTypeDefinition;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StreamTaskInput {
    RunRecording(RunRecordingRequest),
}

impl StreamTaskInput {
    pub fn video(&self) -> &RecordingVideoSelection {
        match self {
            Self::RunRecording(request) => &request.video,
        }
    }

    pub fn task_type(&self) -> veoveo_types::TaskTypeName {
        match self {
            Self::RunRecording(_) => StreamTaskKind::RunRecording.name(),
        }
    }

    pub fn artifact_count(&self) -> NonZeroU32 {
        NonZeroU32::new(match self {
            Self::RunRecording(request) if request.include_source_clip => 3,
            Self::RunRecording(_) => 2,
        })
        .expect("stream recording runs always publish artifacts")
    }

    pub fn recovery_class(&self) -> RecoveryClass {
        RecoveryClass::Resume
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DurableStreamRequest {
    pub input: StreamTaskInput,
    pub artifact_write_capability: IssuedArtifactWriteCapability,
    pub artifact_read_capability: IssuedArtifactReadCapability,
}
