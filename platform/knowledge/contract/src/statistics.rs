use crate::KnowledgeError;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Aggregates of the indexed members visible to one caller at query time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "StatisticsWire", into = "StatisticsWire")]
pub struct CollectionStatistics(veoveo_types::Checked<StatisticsWire>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StatisticsWire {
    indexed_members: u64,
    indexed_chunks: u64,
    last_observed_at: Option<DateTime<Utc>>,
    last_modified_at: Option<DateTime<Utc>>,
}

impl CollectionStatistics {
    pub fn new(
        indexed_members: u64,
        indexed_chunks: u64,
        last_observed_at: Option<DateTime<Utc>>,
        last_modified_at: Option<DateTime<Utc>>,
    ) -> Result<Self, KnowledgeError> {
        StatisticsWire {
            indexed_members,
            indexed_chunks,
            last_observed_at,
            last_modified_at,
        }
        .try_into()
    }
    pub fn empty() -> Self {
        Self::new(0, 0, None, None).expect("empty statistics")
    }
    pub fn indexed_members(&self) -> u64 {
        self.0.indexed_members
    }
    pub fn indexed_chunks(&self) -> u64 {
        self.0.indexed_chunks
    }
    pub fn last_observed_at(&self) -> Option<DateTime<Utc>> {
        self.0.last_observed_at
    }
    pub fn last_modified_at(&self) -> Option<DateTime<Utc>> {
        self.0.last_modified_at
    }
}
impl veoveo_types::Check for StatisticsWire {
    type Error = KnowledgeError;
    fn check(&self) -> Result<(), Self::Error> {
        let value = self;
        if value.indexed_chunks < value.indexed_members
            || value.indexed_chunks > value.indexed_members.saturating_mul(256)
            || (value.indexed_members == 0) != value.last_observed_at.is_none()
            || (value.indexed_members == 0 && value.last_modified_at.is_some())
        {
            return Err(KnowledgeError("inconsistent collection statistics"));
        }
        Ok(())
    }
}
impl TryFrom<StatisticsWire> for CollectionStatistics {
    type Error = KnowledgeError;
    fn try_from(value: StatisticsWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<CollectionStatistics> for StatisticsWire {
    fn from(value: CollectionStatistics) -> Self {
        value.0.into_inner()
    }
}
