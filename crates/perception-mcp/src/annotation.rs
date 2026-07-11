use std::fs;

use anyhow::{Context, Result};
use re_log_types::TimeCell;
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::{Boxes2D, TextDocument};
use serde::Serialize;

use crate::contract::{AnalysisResults, VideoTimelineKind};

pub const RRD_MIME_TYPE: &str = "application/vnd.rerun.rrd";
pub const RESULTS_MIME_TYPE: &str = "application/vnd.veoveo.perception-results+json";
pub const MP4_MIME_TYPE: &str = "video/mp4";

#[derive(Serialize)]
struct AnnotationProvenance<'a> {
    schema: &'static str,
    analysis_uri: String,
    results_schema: &'a str,
    pipeline_id: &'a str,
    model_id: &'a str,
    recording_uri: &'a str,
    entity_path: &'a str,
    timeline: &'a str,
    timeline_kind: VideoTimelineKind,
    requested_range: crate::contract::IndexRange,
}

pub fn write_annotation_rrd(task_id: &str, results: &AnalysisResults) -> Result<Vec<u8>> {
    let temp = tempfile::Builder::new()
        .prefix("veoveo-perception-annotations-")
        .suffix(".rrd")
        .tempfile()
        .context("creating annotation RRD")?;
    let path = temp.path().to_path_buf();
    let recording = RecordingStreamBuilder::new("veoveo_perception_analysis")
        .recording_id(task_id.to_owned())
        .recording_name(format!("perception analysis {task_id}"))
        .save(&path)
        .context("opening annotation RRD sink")?;
    recording.log_static(
        "/perception/provenance",
        &TextDocument::new(serde_json::to_string_pretty(&AnnotationProvenance {
            schema: "veoveo.perception-annotations/v1",
            analysis_uri: crate::uris::analysis_uri(task_id),
            results_schema: &results.schema,
            pipeline_id: &results.pipeline_id,
            model_id: &results.model_id,
            recording_uri: &results.recording_uri,
            entity_path: &results.entity_path,
            timeline: &results.timeline,
            timeline_kind: results.timeline_kind,
            requested_range: results.requested_range,
        })?)
        .with_media_type("application/json"),
    )?;
    let annotation_path = format!("{}/perception", results.entity_path.trim_end_matches('/'));
    for frame in &results.frames {
        let time = match results.timeline_kind {
            VideoTimelineKind::TimestampNanoseconds => {
                TimeCell::from_timestamp_nanos_since_epoch(frame.index)
            }
            VideoTimelineKind::DurationNanoseconds => TimeCell::from_duration_nanos(frame.index),
        };
        recording.set_time(results.timeline.as_str(), time);
        if frame.detections.is_empty() {
            recording.log(annotation_path.as_str(), &Boxes2D::clear_fields())?;
            continue;
        }
        let mins = frame
            .detections
            .iter()
            .map(|detection| (detection.bounds.x, detection.bounds.y));
        let sizes = frame
            .detections
            .iter()
            .map(|detection| (detection.bounds.width, detection.bounds.height));
        let labels = frame.detections.iter().map(|detection| {
            let mut label = detection.label.clone();
            if let Some(confidence) = detection.confidence {
                label.push_str(&format!(" {confidence:.3}"));
            }
            if let Some(tracker_confidence) = detection.tracker_confidence {
                label.push_str(&format!(" tracker_confidence={tracker_confidence:.3}"));
            }
            if let Some(track) = detection.track_id {
                label.push_str(&format!(" track={track}"));
            }
            label
        });
        let class_ids = frame
            .detections
            .iter()
            .map(|detection| u16::try_from(detection.class_id))
            .collect::<Result<Vec<_>, _>>()?;
        recording.log(
            annotation_path.as_str(),
            &Boxes2D::from_mins_and_sizes(mins, sizes)
                .with_labels(labels)
                .with_class_ids(class_ids),
        )?;
    }
    recording.flush_blocking()?;
    drop(recording);
    fs::read(&path).with_context(|| format!("reading annotation RRD {}", path.display()))
}
