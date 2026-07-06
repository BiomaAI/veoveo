use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ArtifactMetadata;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeseriesDuckDbFormat {
    Auto,
    Csv,
    Parquet,
    Json,
    Ndjson,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesDuckDbReadOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_format: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimeseriesDuckDbSource {
    InlineCsv {
        csv: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(default)]
        options: TimeseriesDuckDbReadOptions,
    },
    Uri {
        uri: String,
        format: TimeseriesDuckDbFormat,
        #[serde(default)]
        options: TimeseriesDuckDbReadOptions,
    },
    Uris {
        uris: Vec<String>,
        format: TimeseriesDuckDbFormat,
        #[serde(default)]
        options: TimeseriesDuckDbReadOptions,
    },
}

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
    pub source: TimeseriesDuckDbSource,
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
