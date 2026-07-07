use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactMetadata, duckdb::DuckDbSource};

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
pub enum TimeseriesRowFilter {
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
    And {
        filters: Vec<TimeseriesRowFilter>,
    },
    Or {
        filters: Vec<TimeseriesRowFilter>,
    },
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesForecastOutput {
    pub forecast: TimeseriesForecastSummary,
    pub artifact: ArtifactMetadata,
}
