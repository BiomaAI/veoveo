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
pub enum ReasonTaskInput {
    Analyze(AnalyzeRecordingRequest),
}

impl ReasonTaskInput {
    pub fn video(&self) -> &RecordingVideoSelection {
        match self {
            Self::Analyze(request) => &request.video,
        }
    }

    pub fn task_type(&self) -> veoveo_types::TaskTypeName {
        match self {
            Self::Analyze(_) => ReasonTaskKind::AnalyzeRecording.name(),
        }
    }

    pub fn artifact_count(&self) -> NonZeroU32 {
        NonZeroU32::new(match self {
            Self::Analyze(request) if request.include_source_clip => 3,
            Self::Analyze(_) => 2,
        })
        .expect("reason tasks always publish an artifact")
    }

    pub fn recovery_class(&self) -> RecoveryClass {
        RecoveryClass::Resume
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DurableReasonRequest {
    pub input: ReasonTaskInput,
    /// Bounded grounding subset resolved with the caller's authority at
    /// submission time. Neither the caller's bearer nor an artifact URL is
    /// ever persisted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grounding: Option<GroundingDetections>,
    pub artifact_write_capability: IssuedArtifactWriteCapability,
    pub artifact_read_capability: IssuedArtifactReadCapability,
}
