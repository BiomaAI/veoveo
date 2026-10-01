//! Compact, checked finding retained with the successful Task at publication.
use super::*;
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::Sha256Digest;

/// Leaves room for the resource identity and timestamps in a 64 KiB member.
pub const FINDING_DATA_BYTES: usize = 60 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FindingAnswer {
    Description {
        excerpt: FindingExcerpt,
    },
    Answer {
        excerpt: FindingExcerpt,
    },
    Events {
        events: Vec<FindingEvent>,
        total: u64,
        truncated: bool,
    },
}
impl FindingAnswer {
    pub fn kind(&self) -> ReasoningKind {
        match self {
            Self::Description { .. } => ReasoningKind::DescribeSegment,
            Self::Answer { .. } => ReasoningKind::AnswerQuestion,
            Self::Events { .. } => ReasoningKind::DetectEvents,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(try_from = "FindingDataWire", into = "FindingDataWire")]
pub struct FindingData(pub(super) FindingDataWire);

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct FindingDataWire {
    pub(super) pipeline_id: PipelineId,
    pub(super) model_id: ModelId,
    pub(super) model_digest: Option<String>,
    pub(super) prompt_revision: String,
    pub(super) decode: DecodePolicy,
    pub(super) confidence_basis: ConfidenceBasis,
    pub(super) recording_uri: veoveo_recording_mcp::contract::RecordingUri,
    pub(super) entity_path: String,
    pub(super) timeline: String,
    pub(super) timeline_kind: VideoTimelineKind,
    pub(super) requested_range: IndexRange,
    pub(super) source_snapshot_sha256: Sha256Digest,
    task: ReasoningTask,
    answer: FindingAnswer,
}
impl FindingData {
    pub fn from_results(results: &ReasoningResults) -> Result<Self> {
        ensure!(
            results.schema == REASONING_RESULTS_SCHEMA,
            "unsupported Reason result schema"
        );
        ensure!(
            results.task.kind() == results.answer.kind(),
            "finding answer differs from its task"
        );
        ensure!(
            results.source_snapshot.recording_id == results.recording_uri.id(),
            "finding recording differs from source snapshot"
        );
        if let ReasoningAnswer::Events { events } = &results.answer {
            for event in events {
                ensure!(
                    event.track_ids.len() <= 64,
                    "finding event has too many track citations"
                );
                ensure!(
                    event.range.start <= event.range.end
                        && results.requested_range.contains(event.range),
                    "finding event is outside its recording range"
                );
            }
        }
        let answer = match &results.answer {
            ReasoningAnswer::Description { text } => FindingAnswer::Description {
                excerpt: FindingExcerpt::new(text, 4096),
            },
            ReasoningAnswer::Answer { text } => FindingAnswer::Answer {
                excerpt: FindingExcerpt::new(text, 4096),
            },
            ReasoningAnswer::Events { events } => FindingAnswer::Events {
                total: events.len() as u64,
                truncated: events.len() > 8,
                events: events
                    .iter()
                    .take(8)
                    .map(|e| FindingEvent {
                        range: e.range,
                        label: FindingExcerpt::new(&e.label, 256),
                        description: FindingExcerpt::new(&e.description, 512),
                        track_ids: e.track_ids.clone(),
                    })
                    .collect(),
            },
        };
        FindingDataWire {
            pipeline_id: results.pipeline_id.clone(),
            model_id: results.model_id.clone(),
            model_digest: results.model_digest.clone(),
            prompt_revision: results.prompt_revision.clone(),
            decode: results.decode,
            confidence_basis: results.confidence_basis,
            recording_uri: results.recording_uri.clone(),
            entity_path: results.entity_path.clone(),
            timeline: results.timeline.clone(),
            timeline_kind: results.timeline_kind,
            requested_range: results.requested_range,
            source_snapshot_sha256: results.source_snapshot.digest_sha256()?,
            task: results.task.clone(),
            answer,
        }
        .try_into()
    }
    pub fn task(&self) -> &ReasoningTask {
        &self.0.task
    }
    pub fn answer(&self) -> &FindingAnswer {
        &self.0.answer
    }
    pub fn pipeline_id(&self) -> &PipelineId {
        &self.0.pipeline_id
    }
    pub fn model_id(&self) -> &ModelId {
        &self.0.model_id
    }
    pub fn model_digest(&self) -> Option<&str> {
        self.0.model_digest.as_deref()
    }
    pub fn prompt_revision(&self) -> &str {
        &self.0.prompt_revision
    }
    pub fn decode(&self) -> DecodePolicy {
        self.0.decode
    }
    pub fn confidence_basis(&self) -> ConfidenceBasis {
        self.0.confidence_basis
    }
    pub fn recording_uri(&self) -> &veoveo_recording_mcp::contract::RecordingUri {
        &self.0.recording_uri
    }
    pub fn entity_path(&self) -> &str {
        &self.0.entity_path
    }
    pub fn timeline(&self) -> &str {
        &self.0.timeline
    }
    pub fn timeline_kind(&self) -> VideoTimelineKind {
        self.0.timeline_kind
    }
    pub fn requested_range(&self) -> IndexRange {
        self.0.requested_range
    }
    pub fn source_snapshot_sha256(&self) -> &Sha256Digest {
        &self.0.source_snapshot_sha256
    }
}
impl TryFrom<FindingDataWire> for FindingData {
    type Error = anyhow::Error;
    fn try_from(value: FindingDataWire) -> Result<Self> {
        validate_reasoning_task(&value.task)?;
        validate_decode(value.decode)?;
        ensure!(
            value.requested_range.start <= value.requested_range.end,
            "finding range is reversed"
        );
        ensure!(
            ReasoningKind::from(&value.task) == value.answer.kind(),
            "finding answer differs from its task"
        );
        FindingContent::from(&value.answer)
            .validate(FindingCollection::Results, value.requested_range)?;
        ensure!(
            serde_json::to_vec(&value)?.len() <= FINDING_DATA_BYTES,
            "retained finding exceeds 60 KiB"
        );
        Ok(Self(value))
    }
}
impl From<FindingData> for FindingDataWire {
    fn from(value: FindingData) -> Self {
        value.0
    }
}
