//! Projection selection and bounds shared by service and RRD consumers.

use std::{collections::BTreeSet, ops::Deref};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::RecordingContractError;

pub const MAX_PROJECTION_ENTITIES: usize = 64;
pub const MAX_PROJECTION_COMPONENTS: usize = 64;
pub const MAX_PROJECTION_SAMPLES: usize = 10_000;
pub const MAX_PROJECTION_ROWS: u64 = 10_000;
pub const MAX_PROJECTION_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_PROJECTION_DEADLINE_MS: u64 = 15_000;
pub const MAX_PROJECTION_SELECTOR_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingProjectionSparseFill {
    None,
    LatestAtGlobal,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecordingProjectionSampling {
    Range { start: i64, end: i64 },
    LatestAt { at: i64 },
    SampleGrid { values: Vec<i64> },
}

impl RecordingProjectionSampling {
    pub fn sample_grid(&self) -> Vec<i64> {
        match self {
            Self::Range { .. } => Vec::new(),
            Self::LatestAt { at } => vec![*at],
            Self::SampleGrid { values } => values.clone(),
        }
    }
}

/// Complete selection inputs. RRD separately parses names with the pinned Rerun grammar.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[schemars(rename = "RecordingProjectionQuery")]
pub struct RecordingProjectionQueryBuilder {
    #[schemars(length(min = 1, max = 64))]
    pub entity_paths: Vec<String>,
    #[schemars(length(min = 1, max = 64))]
    pub component_ids: Vec<String>,
    pub timeline: String,
    pub sampling: RecordingProjectionSampling,
    pub sparse_fill: RecordingProjectionSparseFill,
    #[schemars(range(min = 1, max = 64))]
    pub maximum_entities: usize,
    #[schemars(range(min = 1, max = 64))]
    pub maximum_columns: usize,
    #[schemars(range(min = 1, max = 10000))]
    pub maximum_samples: usize,
    #[schemars(range(min = 1, max = 10000))]
    pub maximum_rows: u64,
    #[schemars(range(min = 1, max = 33554432))]
    pub maximum_bytes: u64,
}

impl RecordingProjectionQueryBuilder {
    pub fn build(self) -> Result<RecordingProjectionQuery, RecordingContractError> {
        if !(1..=MAX_PROJECTION_ENTITIES).contains(&self.maximum_entities)
            || !(1..=MAX_PROJECTION_COMPONENTS).contains(&self.maximum_columns)
            || !(1..=MAX_PROJECTION_SAMPLES).contains(&self.maximum_samples)
            || !(1..=MAX_PROJECTION_ROWS).contains(&self.maximum_rows)
            || !(1..=MAX_PROJECTION_BYTES).contains(&self.maximum_bytes)
            || self.entity_paths.len() > self.maximum_entities
            || self.component_ids.len() > self.maximum_columns
        {
            return Err(RecordingContractError::ProjectionBounds);
        }
        if self.entity_paths.is_empty()
            || self.component_ids.is_empty()
            || self.entity_paths.iter().collect::<BTreeSet<_>>().len() != self.entity_paths.len()
            || self.component_ids.iter().collect::<BTreeSet<_>>().len() != self.component_ids.len()
            || !self
                .entity_paths
                .iter()
                .chain(&self.component_ids)
                .chain([&self.timeline])
                .all(|value| valid_text(value, MAX_PROJECTION_SELECTOR_BYTES))
        {
            return Err(RecordingContractError::ProjectionSelection);
        }
        // Rerun reserves i64::MIN for static data. Temporal queries must not clamp it.
        let valid_sampling = match &self.sampling {
            RecordingProjectionSampling::Range { start, end } => *start > i64::MIN && start <= end,
            RecordingProjectionSampling::LatestAt { at } => *at > i64::MIN,
            RecordingProjectionSampling::SampleGrid { values } => {
                !values.is_empty()
                    && values.len() <= self.maximum_samples
                    && values[0] > i64::MIN
                    && values.windows(2).all(|pair| pair[0] < pair[1])
            }
        };
        if !valid_sampling {
            return Err(RecordingContractError::ProjectionSampling);
        }
        Ok(RecordingProjectionQuery(self))
    }
}

/// An immutable selection with admitted bounds and sampling relationships.
/// ```compile_fail
/// use veoveo_recording_contract::RecordingProjectionQuery;
/// fn remove_limit(query: &mut RecordingProjectionQuery) {
///     query.maximum_bytes = u64::MAX;
/// }
/// ```
#[derive(Clone, Debug, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(transparent)]
#[schemars(with = "RecordingProjectionQueryBuilder")]
pub struct RecordingProjectionQuery(RecordingProjectionQueryBuilder);

impl<'de> Deserialize<'de> for RecordingProjectionQuery {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        RecordingProjectionQueryBuilder::deserialize(deserializer)?
            .build()
            .map_err(serde::de::Error::custom)
    }
}

impl Deref for RecordingProjectionQuery {
    type Target = RecordingProjectionQueryBuilder;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(super) fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}
