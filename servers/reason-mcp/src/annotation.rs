use std::fs;

use anyhow::{Context, Result};
use re_log_types::TimeCell;
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::{TextDocument, TextLog};
use serde::Serialize;

use crate::contract::{ReasoningAnswer, ReasoningResults, VideoTimelineKind};

pub const RRD_MIME_TYPE: &str = "application/vnd.rerun.rrd";
pub const RESULTS_MIME_TYPE: &str = "application/vnd.veoveo.reason-results+json";
pub const MP4_MIME_TYPE: &str = "video/mp4";

#[derive(Serialize)]
struct AnnotationProvenance<'a> {
    schema: &'static str,
    analysis_uri: String,
    results_schema: &'a str,
    pipeline_id: &'a str,
    model_id: &'a str,
    prompt_revision: &'a str,
    model_digest: Option<&'a str>,
    recording_uri: &'a str,
    entity_path: &'a str,
    timeline: &'a str,
    timeline_kind: VideoTimelineKind,
    requested_range: crate::contract::IndexRange,
    task_kind: &'static str,
    decode: crate::contract::DecodePolicy,
    confidence_basis: crate::contract::ConfidenceBasis,
}

pub fn write_annotation_rrd(task_id: &str, results: &ReasoningResults) -> Result<Vec<u8>> {
    let temp = tempfile::Builder::new()
        .prefix("veoveo-reason-annotations-")
        .suffix(".rrd")
        .tempfile()
        .context("creating annotation RRD")?;
    let path = temp.path().to_path_buf();
    let recording = RecordingStreamBuilder::new("veoveo_reason_analysis")
        .recording_id(task_id.to_owned())
        .recording_name(format!("reason analysis {task_id}"))
        .save(&path)
        .context("opening annotation RRD sink")?;
    recording.log_static(
        "/reason/provenance",
        &TextDocument::new(serde_json::to_string_pretty(&AnnotationProvenance {
            schema: "veoveo.reason-annotations/v1",
            analysis_uri: crate::uris::analysis_uri(task_id),
            results_schema: &results.schema,
            pipeline_id: &results.pipeline_id,
            model_id: &results.model_id,
            prompt_revision: &results.prompt_revision,
            model_digest: results.model_digest.as_deref(),
            recording_uri: &results.recording_uri,
            entity_path: &results.entity_path,
            timeline: &results.timeline,
            timeline_kind: results.timeline_kind,
            requested_range: results.requested_range,
            task_kind: results.task.kind(),
            decode: results.decode,
            confidence_basis: results.confidence_basis,
        })?)
        .with_media_type("application/json"),
    )?;
    let annotation_path = format!("{}/reason", results.entity_path.trim_end_matches('/'));
    let time_cell = |index: i64| match results.timeline_kind {
        VideoTimelineKind::TimestampNanoseconds => {
            TimeCell::from_timestamp_nanos_since_epoch(index)
        }
        VideoTimelineKind::DurationNanoseconds => TimeCell::from_duration_nanos(index),
    };
    match &results.answer {
        ReasoningAnswer::Description { text } | ReasoningAnswer::Answer { text } => {
            recording.log_static(
                format!("{annotation_path}/answer").as_str(),
                &TextDocument::new(text.clone()),
            )?;
            recording.set_time(
                results.timeline.as_str(),
                time_cell(results.requested_range.start),
            );
            recording.log(annotation_path.as_str(), &TextLog::new(text.clone()))?;
        }
        ReasoningAnswer::Events { events } => {
            for event in events {
                recording.set_time(results.timeline.as_str(), time_cell(event.range.start));
                let mut line = format!("{}: {}", event.label, event.description);
                if !event.track_ids.is_empty() {
                    let tracks = event
                        .track_ids
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",");
                    line.push_str(&format!(" tracks={tracks}"));
                }
                recording.log(annotation_path.as_str(), &TextLog::new(line))?;
            }
        }
    }
    recording.flush_blocking()?;
    drop(recording);
    fs::read(&path).with_context(|| format!("reading annotation RRD {}", path.display()))
}
