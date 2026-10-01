//! Bounded excerpts retain source identity and the model's recorded provenance.
use super::*;
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_types::Sha256Digest;

pub const FINDING_SUMMARY_BYTES: usize = 64 * 1024;
const EXCERPT_BYTES: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FindingExcerpt {
    text: String,
    truncated: bool,
}
impl FindingExcerpt {
    pub(super) fn new(text: &str, limit: usize) -> Self {
        let mut end = text.len().min(limit);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            text: text[..end].into(),
            truncated: end < text.len(),
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FindingContent {
    Analysis {
        task: ReasoningTask,
    },
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
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FindingEvent {
    pub range: IndexRange,
    pub label: FindingExcerpt,
    pub description: FindingExcerpt,
    pub track_ids: Vec<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(try_from = "FindingSummaryWire", into = "FindingSummaryWire")]
pub struct FindingSummary(FindingSummaryWire);

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
struct FindingSummaryWire {
    uri: FindingResource,
    analysis_id: AnalysisId,
    result_artifact: ArtifactUri,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
    pipeline_id: PipelineId,
    model_id: ModelId,
    model_digest: Option<String>,
    prompt_revision: String,
    decode: DecodePolicy,
    confidence_basis: ConfidenceBasis,
    recording_uri: veoveo_recording_mcp::contract::RecordingUri,
    entity_path: String,
    timeline: String,
    timeline_kind: VideoTimelineKind,
    requested_range: IndexRange,
    source_snapshot_sha256: Sha256Digest,
    content: FindingContent,
}
impl FindingSummary {
    pub fn new(
        collection: FindingCollection,
        analysis: AnalysisId,
        artifact: ArtifactId,
        created_at: DateTime<Utc>,
        modified_at: DateTime<Utc>,
        finding: &FindingData,
    ) -> Result<Self> {
        let content = match collection {
            FindingCollection::Analyses => FindingContent::Analysis {
                task: finding.task().clone(),
            },
            FindingCollection::Results => FindingContent::from(finding.answer()),
        };
        let value = FindingSummaryWire {
            uri: FindingResource::Member {
                collection,
                analysis,
            },
            analysis_id: analysis,
            result_artifact: ArtifactUri::plane(artifact),
            created_at,
            modified_at,
            pipeline_id: finding.0.pipeline_id.clone(),
            model_id: finding.0.model_id.clone(),
            model_digest: finding.0.model_digest.clone(),
            prompt_revision: finding.0.prompt_revision.clone(),
            decode: finding.0.decode,
            confidence_basis: finding.0.confidence_basis,
            recording_uri: finding.0.recording_uri.clone(),
            entity_path: finding.0.entity_path.clone(),
            timeline: finding.0.timeline.clone(),
            timeline_kind: finding.0.timeline_kind,
            requested_range: finding.0.requested_range,
            source_snapshot_sha256: finding.0.source_snapshot_sha256.clone(),
            content,
        };
        value.try_into()
    }
    pub fn uri(&self) -> &FindingResource {
        &self.0.uri
    }
    pub fn analysis_id(&self) -> AnalysisId {
        self.0.analysis_id
    }
    pub fn result_artifact(&self) -> &ArtifactUri {
        &self.0.result_artifact
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }
    pub fn modified_at(&self) -> DateTime<Utc> {
        self.0.modified_at
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
    pub fn content(&self) -> &FindingContent {
        &self.0.content
    }
}

impl TryFrom<FindingSummaryWire> for FindingSummary {
    type Error = anyhow::Error;
    fn try_from(value: FindingSummaryWire) -> Result<Self> {
        let FindingResource::Member {
            collection,
            analysis,
        } = value.uri
        else {
            anyhow::bail!("finding summary requires a member address");
        };
        ensure!(
            analysis == value.analysis_id,
            "finding URI and analysis disagree"
        );
        ensure!(
            value.result_artifact == ArtifactUri::plane(value.result_artifact.artifact_id()),
            "finding result must use the neutral Artifact URI"
        );
        ensure!(
            value.modified_at >= value.created_at,
            "finding timestamps are inconsistent"
        );
        ensure!(
            value.requested_range.start <= value.requested_range.end,
            "finding range is reversed"
        );
        validate_decode(value.decode)?;
        value.content.validate(collection, value.requested_range)?;
        ensure!(
            serde_json::to_vec(&value)?.len() <= FINDING_SUMMARY_BYTES,
            "finding member exceeds 64 KiB"
        );
        Ok(Self(value))
    }
}
impl From<FindingSummary> for FindingSummaryWire {
    fn from(value: FindingSummary) -> Self {
        value.0
    }
}

impl FindingContent {
    pub(super) fn validate(&self, collection: FindingCollection, range: IndexRange) -> Result<()> {
        match (self, collection) {
            (FindingContent::Analysis { task }, FindingCollection::Analyses) => {
                validate_reasoning_task(task)?
            }
            (
                FindingContent::Answer { excerpt } | FindingContent::Description { excerpt },
                FindingCollection::Results,
            ) => {
                ensure!(
                    excerpt.text.len() <= EXCERPT_BYTES,
                    "finding excerpt exceeds its byte limit"
                );
            }
            (
                FindingContent::Events {
                    events,
                    total,
                    truncated,
                },
                FindingCollection::Results,
            ) => {
                ensure!(
                    events.len() <= 8
                        && *total >= events.len() as u64
                        && *truncated == (*total > events.len() as u64),
                    "finding event count and omissions disagree"
                );
                for event in events {
                    ensure!(
                        event.label.text.len() <= 256
                            && event.description.text.len() <= 512
                            && event.track_ids.len() <= 64,
                        "finding event exceeds its limits"
                    );
                    ensure!(
                        event.range.start <= event.range.end && range.contains(event.range),
                        "finding event is outside its recording range"
                    );
                }
            }
            _ => anyhow::bail!("finding content disagrees with its collection"),
        }
        Ok(())
    }
}
impl From<&FindingAnswer> for FindingContent {
    fn from(answer: &FindingAnswer) -> Self {
        match answer {
            FindingAnswer::Description { excerpt } => Self::Description {
                excerpt: excerpt.clone(),
            },
            FindingAnswer::Answer { excerpt } => Self::Answer {
                excerpt: excerpt.clone(),
            },
            FindingAnswer::Events {
                events,
                total,
                truncated,
            } => Self::Events {
                events: events.clone(),
                total: *total,
                truncated: *truncated,
            },
        }
    }
}
