//! Authorized grounding admission and output-label propagation.
use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use veoveo_artifact_contract::ArtifactObject;
use veoveo_mcp_contract::PlaneCaller;
use veoveo_reason_mcp::{
    contract::{GroundingDetections, RecordingVideoSelection, StreamArtifactUri},
    grounding::extract_grounding,
};
use veoveo_types::DataLabelId;

use super::{app_state::AppState, tasks::ReasonTaskInput};

pub(super) struct GroundingInput {
    detections: GroundingDetections,
    required_labels: BTreeSet<DataLabelId>,
}

impl GroundingInput {
    fn from_artifact(
        source: &StreamArtifactUri,
        selection: &RecordingVideoSelection,
        artifact: ArtifactObject,
    ) -> Result<Self> {
        ensure!(
            artifact.metadata.artifact_id() == source.artifact_id(),
            "grounding Artifact metadata does not match the requested occurrence"
        );
        let detections = extract_grounding(source, selection, &artifact.bytes)?;
        let mut required_labels = artifact.metadata.compliance.data_labels;
        required_labels.extend(artifact.metadata.compliance.classification);
        Ok(Self {
            detections,
            required_labels,
        })
    }

    pub(super) fn required_labels(&self) -> &BTreeSet<DataLabelId> {
        &self.required_labels
    }

    pub(super) fn into_detections(self) -> GroundingDetections {
        self.detections
    }
}

pub(super) async fn resolve(
    state: &AppState,
    caller: &PlaneCaller,
    input: &ReasonTaskInput,
) -> Result<Option<GroundingInput>> {
    let ReasonTaskInput::Analyze(request) = input;
    let Some(reference) = &request.grounding else {
        return Ok(None);
    };
    let id = reference.results_artifact_uri.artifact_id();
    let metadata = state
        .artifacts
        .head(caller, &id)
        .await?
        .context("grounding Artifact not found")?;
    ensure!(
        metadata.byte_len <= state.max_grounding_bytes,
        "grounding Artifact exceeds the configured byte limit"
    );
    let artifact = state
        .artifacts
        .get(caller, &id)
        .await?
        .context("grounding Artifact not found")?;
    ensure!(
        artifact.bytes.len() as u64 <= state.max_grounding_bytes,
        "grounding Artifact exceeds the configured byte limit"
    );
    GroundingInput::from_artifact(&reference.results_artifact_uri, &request.video, artifact)
        .map(Some)
}

#[cfg(test)]
#[path = "grounding_input_tests.rs"]
mod tests;
