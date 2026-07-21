//! Extraction of a bounded typed grounding subset from a completed
//! perception analysis results document.

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::contract::{GroundingDetection, GroundingDetections, GroundingFrame};

pub const GROUNDING_SCHEMA: &str = "veoveo.reason-grounding/v1";
pub const PERCEPTION_RESULTS_SCHEMA: &str = "veoveo.perception-results/v1";
pub const MAX_GROUNDING_DETECTIONS: usize = 100_000;

/// Lenient read of the perception results contract. Only the fields the
/// reasoning runner consumes are extracted; unknown fields are ignored so a
/// forward-compatible perception results revision still grounds correctly as
/// long as its schema identity matches.
#[derive(Deserialize)]
struct PerceptionResultsDocument {
    schema: String,
    frames: Vec<PerceptionFrame>,
}

#[derive(Deserialize)]
struct PerceptionFrame {
    index: i64,
    detections: Vec<PerceptionDetection>,
}

#[derive(Deserialize)]
struct PerceptionDetection {
    label: String,
    #[serde(default)]
    track_id: Option<u64>,
}

pub fn extract_grounding(source_artifact_uri: &str, bytes: &[u8]) -> Result<GroundingDetections> {
    let document: PerceptionResultsDocument =
        serde_json::from_slice(bytes).context("parsing grounding results document")?;
    ensure!(
        document.schema == PERCEPTION_RESULTS_SCHEMA,
        "grounding artifact schema `{}` is not the typed perception results contract",
        document.schema
    );
    let detection_count: usize = document
        .frames
        .iter()
        .map(|frame| frame.detections.len())
        .sum();
    ensure!(
        detection_count <= MAX_GROUNDING_DETECTIONS,
        "grounding document exceeds {MAX_GROUNDING_DETECTIONS} detections"
    );
    Ok(GroundingDetections {
        schema: GROUNDING_SCHEMA.to_owned(),
        source_artifact_uri: source_artifact_uri.to_owned(),
        frames: document
            .frames
            .into_iter()
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

/// Every track identity a grounding subset can justify a citation for.
pub fn grounded_track_ids(grounding: &GroundingDetections) -> std::collections::BTreeSet<u64> {
    grounding
        .frames
        .iter()
        .flat_map(|frame| frame.detections.iter())
        .filter_map(|detection| detection.track_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_perception_results_ground_with_a_subset() {
        let document = serde_json::json!({
            "schema": PERCEPTION_RESULTS_SCHEMA,
            "pipeline_id": "detect-and-track",
            "model_id": "primary-detector",
            "recording_uri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
            "entity_path": "/camera/front",
            "timeline": "sensor_time",
            "timeline_kind": "duration_nanoseconds",
            "requested_range": {"start": 0, "end": 100},
            "frames": [
                {"index": 10, "detections": [
                    {"class_id": 0, "label": "car", "confidence": 0.9,
                     "bounds": {"x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0},
                     "track_id": 7}
                ]}
            ],
            "processed_frames": 1,
            "elapsed_ms": 5
        });
        let grounding =
            extract_grounding("artifact://test", &serde_json::to_vec(&document).unwrap()).unwrap();
        assert_eq!(grounding.schema, GROUNDING_SCHEMA);
        assert_eq!(grounding.frames.len(), 1);
        assert_eq!(grounding.frames[0].detections[0].label, "car");
        assert!(grounded_track_ids(&grounding).contains(&7));
    }

    #[test]
    fn unversioned_grounding_is_rejected() {
        let document = serde_json::json!({"schema": "something-else/v9", "frames": []});
        let error = extract_grounding("artifact://test", &serde_json::to_vec(&document).unwrap())
            .unwrap_err();
        assert!(error.to_string().contains("typed perception results"));
    }
}
