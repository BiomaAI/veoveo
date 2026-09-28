//! Admission of a bounded grounding subset through Stream's public contract.

use anyhow::{Context, Result, ensure};
use veoveo_stream_mcp::contract::AnalysisResults;

use crate::contract::{
    GroundingDetection, GroundingDetections, GroundingFrame, GroundingSchema,
    RecordingVideoSelection, StreamArtifactUri,
};

pub const MAX_GROUNDING_DETECTIONS: usize = 100_000;

/// The caller authorizes the Artifact and enforces its byte limit before decoding.
/// Stream owns the complete input shape and validation; Reason owns the selected
/// detection subset sent to its runner.
pub fn extract_grounding(
    source_artifact_uri: &StreamArtifactUri,
    selection: &RecordingVideoSelection,
    bytes: &[u8],
) -> Result<GroundingDetections> {
    let document: AnalysisResults = serde_json::from_slice(bytes).context(
        "grounding requires a complete supported Stream result; upgrade the reader or rerun with a compatible Stream producer",
    )?;
    let detection_count = document
        .frames
        .iter()
        .flat_map(|frame| &frame.detections)
        .take(MAX_GROUNDING_DETECTIONS + 1)
        .count();
    ensure!(
        detection_count <= MAX_GROUNDING_DETECTIONS,
        "grounding document exceeds {MAX_GROUNDING_DETECTIONS} detections"
    );
    document
        .validate()
        .context("validating Stream replay results")?;
    veoveo_recording_video::contract::validate_video_selection(selection)?;
    ensure!(
        document.recording_uri == selection.recording_uri
            && document.entity_path == selection.entity_path
            && document.timeline == selection.timeline,
        "grounding recording, entity and timeline must match the requested video"
    );
    ensure!(
        document.requested_range.contains(selection.range),
        "grounding result range must cover the requested video range"
    );
    Ok(GroundingDetections {
        schema: GroundingSchema::V1,
        source_artifact_uri: source_artifact_uri.clone(),
        frames: document
            .frames
            .into_iter()
            .filter(|frame| {
                frame.index >= selection.range.start && frame.index <= selection.range.end
            })
            .map(|frame| GroundingFrame {
                index: frame.index,
                detections: frame
                    .detections
                    .into_iter()
                    .map(|detection| GroundingDetection {
                        label: detection.label,
                        track_id: detection.track_id,
                    })
                    .collect(),
            })
            .collect(),
    })
}

/// Every track identity the admitted grounding subset can justify a citation for.
pub fn grounded_track_ids(grounding: &GroundingDetections) -> std::collections::BTreeSet<u64> {
    grounding
        .frames
        .iter()
        .flat_map(|frame| &frame.detections)
        .filter_map(|detection| detection.track_id)
        .collect()
}
