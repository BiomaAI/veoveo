use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use veoveo_mcp_contract::{ArtifactId, ArtifactMetadata, DuckDbSource};

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct TimeseriesArtifactUri(String);

impl TimeseriesArtifactUri {
    pub fn parse(value: impl Into<String>) -> Result<Self, TimeseriesArtifactUriError> {
        let value = value.into();
        value
            .strip_prefix("timeseries://artifact/")
            .and_then(|id| ArtifactId::parse(id).ok())
            .ok_or(TimeseriesArtifactUriError)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TimeseriesArtifactUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for TimeseriesArtifactUri {
    type Error = TimeseriesArtifactUriError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<TimeseriesArtifactUri> for String {
    fn from(value: TimeseriesArtifactUri) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeseriesArtifactUriError;

impl fmt::Display for TimeseriesArtifactUriError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Timeseries artifact URI")
    }
}

impl Error for TimeseriesArtifactUriError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesTableMapping {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_column: Option<String>,
    pub value_column: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_column: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TimeseriesFilterValue {
    String(String),
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TimeseriesFilterPredicate {
    Eq {
        column: String,
        value: TimeseriesFilterValue,
    },
    Ne {
        column: String,
        value: TimeseriesFilterValue,
    },
    In {
        column: String,
        values: Vec<TimeseriesFilterValue>,
    },
    IsNotNull {
        column: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeseriesFilterCombination {
    #[default]
    All,
    Any,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesRowFilter {
    #[serde(default)]
    pub combination: TimeseriesFilterCombination,
    pub predicates: Vec<TimeseriesFilterPredicate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeseriesForecastMethod {
    NaiveTrend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesForecastRequest {
    pub source: DuckDbSource,
    pub mapping: TimeseriesTableMapping,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_filter: Option<TimeseriesRowFilter>,
    pub horizon: u32,
    #[serde(default = "default_forecast_method")]
    pub method: TimeseriesForecastMethod,
}

fn default_forecast_method() -> TimeseriesForecastMethod {
    TimeseriesForecastMethod::NaiveTrend
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesSeriesSummary {
    pub series_id: String,
    pub observed_rows: u64,
    pub forecast_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesForecastSummary {
    pub method: TimeseriesForecastMethod,
    pub horizon: u32,
    pub source_rows: u64,
    pub series: Vec<TimeseriesSeriesSummary>,
}

/// One observed point in a bounded chart preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesPreviewObservation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_time: Option<String>,
    pub value: f64,
}

/// One forecast step in a bounded chart preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesPreviewForecastPoint {
    pub step: u32,
    pub mean: f64,
    pub q10: f64,
    pub q90: f64,
}

/// Downsampled chartable series shipped in structured output so app views
/// can render without re-reading the RRD artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesSeriesPreview {
    pub series_id: String,
    pub observed: Vec<TimeseriesPreviewObservation>,
    pub forecast: Vec<TimeseriesPreviewForecastPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesForecastOutput {
    pub result_uri: TimeseriesArtifactUri,
    pub forecast: TimeseriesForecastSummary,
    pub preview: Vec<TimeseriesSeriesPreview>,
    pub artifact: ArtifactMetadata,
}

#[cfg(test)]
mod terminal_contract_tests {
    use super::TimeseriesForecastOutput;

    #[test]
    fn forecast_output_schema_has_one_canonical_result_handoff() {
        let schema = serde_json::to_value(schemars::schema_for!(TimeseriesForecastOutput)).unwrap();
        let properties = schema["properties"].as_object().unwrap();

        assert!(properties.contains_key("result_uri"));
    }
}
