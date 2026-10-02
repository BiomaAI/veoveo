//! Retrieval judgments and measurements. These values grant no source authority.
use crate::{GenerationId, KnowledgeError};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_embedding_contract::EmbeddingText;
use veoveo_mcp_knowledge_extension::{CollectionId, Revision};
use veoveo_types::{ResourceUri, Sha256Digest};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct EvaluationCaseId(String);
impl EvaluationCaseId {
    pub fn new(value: impl Into<String>) -> Result<Self, KnowledgeError> {
        value.into().try_into()
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for EvaluationCaseId {
    type Error = KnowledgeError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err(KnowledgeError(
                "evaluation case ID requires 1..=64 lowercase ASCII letters, digits or hyphens",
            ));
        }
        Ok(Self(value))
    }
}
impl From<EvaluationCaseId> for String {
    fn from(value: EvaluationCaseId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationMemberId {
    pub collection: CollectionId,
    pub uri: ResourceUri,
}

/// Complete caller-visible corpus member identity, independent of chunk count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationMember {
    pub member: EvaluationMemberId,
    pub revision: Revision,
    pub content_sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "CaseWire", into = "CaseWire")]
pub struct RetrievalCase(CaseWire);
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CaseWire {
    id: EvaluationCaseId,
    query: EmbeddingText,
    /// Empty means every collection in the dataset's corpus.
    #[serde(default)]
    collections: BTreeSet<CollectionId>,
    relevant: BTreeSet<EvaluationMemberId>,
}
impl RetrievalCase {
    pub fn new(
        id: EvaluationCaseId,
        query: EmbeddingText,
        collections: BTreeSet<CollectionId>,
        relevant: BTreeSet<EvaluationMemberId>,
    ) -> Result<Self, KnowledgeError> {
        CaseWire {
            id,
            query,
            collections,
            relevant,
        }
        .try_into()
    }
    pub fn id(&self) -> &EvaluationCaseId {
        &self.0.id
    }
    pub fn query(&self) -> &EmbeddingText {
        &self.0.query
    }
    pub fn collections(&self) -> &BTreeSet<CollectionId> {
        &self.0.collections
    }
    pub fn relevant(&self) -> &BTreeSet<EvaluationMemberId> {
        &self.0.relevant
    }
}
impl TryFrom<CaseWire> for RetrievalCase {
    type Error = KnowledgeError;
    fn try_from(value: CaseWire) -> Result<Self, Self::Error> {
        if value.query.as_str().len() > 4096
            || value.collections.len() > 1024
            || value.relevant.is_empty()
            || value.relevant.len() > 4096
            || value.relevant.iter().any(|member| {
                !value.collections.is_empty() && !value.collections.contains(&member.collection)
            })
        {
            return Err(KnowledgeError(
                "retrieval case requires a bounded query and relevant members within its collection selection",
            ));
        }
        Ok(Self(value))
    }
}
impl From<RetrievalCase> for CaseWire {
    fn from(value: RetrievalCase) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "DatasetWire", into = "DatasetWire")]
pub struct RetrievalDataset(DatasetWire);
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DatasetWire {
    corpus: Vec<EvaluationMember>,
    cases: Vec<RetrievalCase>,
}
impl RetrievalDataset {
    pub fn new(
        corpus: Vec<EvaluationMember>,
        cases: Vec<RetrievalCase>,
    ) -> Result<Self, KnowledgeError> {
        DatasetWire { corpus, cases }.try_into()
    }
    pub fn corpus(&self) -> &[EvaluationMember] {
        &self.0.corpus
    }
    pub fn cases(&self) -> &[RetrievalCase] {
        &self.0.cases
    }
    pub fn revision(&self) -> Sha256Digest {
        crate::digest(self)
    }
    pub fn collections(&self) -> BTreeSet<CollectionId> {
        self.corpus()
            .iter()
            .map(|m| m.member.collection.clone())
            .collect()
    }
}
impl TryFrom<DatasetWire> for RetrievalDataset {
    type Error = KnowledgeError;
    fn try_from(mut value: DatasetWire) -> Result<Self, Self::Error> {
        if value.corpus.is_empty()
            || value.corpus.len() > 4096
            || value.cases.is_empty()
            || value.cases.len() > 256
        {
            return Err(KnowledgeError(
                "retrieval dataset requires 1..=4096 members and 1..=256 cases",
            ));
        }
        value.corpus.sort_by(|a, b| a.member.cmp(&b.member));
        value.cases.sort_by(|a, b| a.id().cmp(b.id()));
        let members: BTreeSet<_> = value.corpus.iter().map(|m| &m.member).collect();
        let collections: BTreeSet<_> = members.iter().map(|m| m.collection.clone()).collect();
        if members.len() != value.corpus.len()
            || value
                .cases
                .windows(2)
                .any(|cases| cases[0].id() == cases[1].id())
            || value.cases.iter().any(|case| {
                !case.collections().is_subset(&collections)
                    || case.relevant().iter().any(|m| !members.contains(m))
            })
        {
            return Err(KnowledgeError(
                "retrieval dataset has duplicate identities or judgments outside its corpus",
            ));
        }
        Ok(Self(value))
    }
}
impl From<RetrievalDataset> for DatasetWire {
    fn from(value: RetrievalDataset) -> Self {
        value.0
    }
}

/// Ordered, source-revision-checked results from one search with limit 10.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetrievalMeasurement {
    pub case: EvaluationCaseId,
    pub ranked: Vec<EvaluationMemberId>,
    pub elapsed_micros: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecallCounts {
    retrieved_relevant: u32,
    relevant: u32,
}
impl RecallCounts {
    pub fn retrieved_relevant(self) -> u32 {
        self.retrieved_relevant
    }
    pub fn relevant(self) -> u32 {
        self.relevant
    }
    pub fn recall(self) -> f64 {
        f64::from(self.retrieved_relevant) / f64::from(self.relevant)
    }
}

/// A historical measurement attached to a generation, never an activation grant.
/// Scores are derived from the retained judgments and ranks, not accepted as input.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EvaluationWire", into = "EvaluationWire")]
pub struct RetrievalEvaluation(EvaluationWire);
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvaluationWire {
    generation: GenerationId,
    specification_revision: Sha256Digest,
    audience_revision: Sha256Digest,
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
    dataset: RetrievalDataset,
    measurements: Vec<RetrievalMeasurement>,
}
impl RetrievalEvaluation {
    pub fn builder(
        generation: GenerationId,
        specification_revision: Sha256Digest,
        audience_revision: Sha256Digest,
        dataset: RetrievalDataset,
        started_at: DateTime<Utc>,
    ) -> EvaluationBuilder {
        EvaluationBuilder(EvaluationWire {
            generation,
            specification_revision,
            audience_revision,
            dataset,
            started_at,
            completed_at: started_at,
            measurements: vec![],
        })
    }
    pub fn generation(&self) -> GenerationId {
        self.0.generation
    }
    pub fn specification_revision(&self) -> &Sha256Digest {
        &self.0.specification_revision
    }
    pub fn dataset(&self) -> &RetrievalDataset {
        &self.0.dataset
    }
    pub fn measurements(&self) -> &[RetrievalMeasurement] {
        &self.0.measurements
    }
    pub fn revision(&self) -> Sha256Digest {
        crate::digest(self)
    }
    pub fn case_recall(&self) -> BTreeMap<EvaluationCaseId, RecallCounts> {
        self.0
            .dataset
            .cases()
            .iter()
            .zip(&self.0.measurements)
            .map(|(case, measured)| {
                (
                    case.id().clone(),
                    RecallCounts {
                        retrieved_relevant: measured
                            .ranked
                            .iter()
                            .filter(|m| case.relevant().contains(m))
                            .count() as u32,
                        relevant: case.relevant().len() as u32,
                    },
                )
            })
            .collect()
    }
    /// Macro average: every judged query has equal weight.
    pub fn recall_at_ten(&self) -> f64 {
        self.case_recall()
            .values()
            .map(|counts| counts.recall())
            .sum::<f64>()
            / self.0.measurements.len() as f64
    }
}
pub struct EvaluationBuilder(EvaluationWire);
impl EvaluationBuilder {
    pub fn measure(mut self, measurement: RetrievalMeasurement) -> Self {
        self.0.measurements.push(measurement);
        self
    }
    pub fn finish(
        mut self,
        completed_at: DateTime<Utc>,
    ) -> Result<RetrievalEvaluation, KnowledgeError> {
        self.0.completed_at = completed_at;
        self.0.try_into()
    }
}
impl TryFrom<EvaluationWire> for RetrievalEvaluation {
    type Error = KnowledgeError;
    fn try_from(mut value: EvaluationWire) -> Result<Self, Self::Error> {
        if value.completed_at < value.started_at
            || (value.completed_at - value.started_at).num_seconds() > 3600
            || value.measurements.len() != value.dataset.cases().len()
        {
            return Err(KnowledgeError(
                "retrieval evaluation requires every case and a run of at most one hour",
            ));
        }
        value.measurements.sort_by(|a, b| a.case.cmp(&b.case));
        let corpus: BTreeSet<_> = value.dataset.corpus().iter().map(|m| &m.member).collect();
        for (case, measurement) in value.dataset.cases().iter().zip(&value.measurements) {
            if case.id() != &measurement.case
                || measurement.elapsed_micros == 0
                || measurement.elapsed_micros > 60_000_000
                || measurement.ranked.len() > 10
                || measurement.ranked.iter().collect::<BTreeSet<_>>().len()
                    != measurement.ranked.len()
                || measurement.ranked.iter().any(|member| {
                    !corpus.contains(member)
                        || (!case.collections().is_empty()
                            && !case.collections().contains(&member.collection))
                })
            {
                return Err(KnowledgeError(
                    "retrieval measurement has invalid timing, ranks, case or corpus membership",
                ));
            }
        }
        Ok(Self(value))
    }
}
impl From<RetrievalEvaluation> for EvaluationWire {
    fn from(value: RetrievalEvaluation) -> Self {
        value.0
    }
}
