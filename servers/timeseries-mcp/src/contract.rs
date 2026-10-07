mod artifact_uri;
pub use artifact_uri::*;
mod resources;
pub use resources::*;
mod scopes;
pub use scopes::*;
mod task_kind;
pub use task_kind::TimeseriesTaskKind;
mod usage;
pub use usage::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use veoveo_artifact_contract::ArtifactMetadata;
mod request;
pub use request::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimeseriesSeriesSummary {
    pub series_id: String,
    pub observed_rows: u64,
    pub forecast_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "TimeseriesForecastSummary")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimeseriesForecastSummaryBuilder {
    pub method: TimeseriesForecastMethod,
    pub horizon: TimeseriesForecastHorizon,
    pub source_rows: u64,
    pub series: Vec<TimeseriesSeriesSummary>,
}

/// One observed point in a bounded chart preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimeseriesPreviewObservation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_time: Option<String>,
    pub value: f64,
}

/// One forecast step in a bounded chart preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimeseriesPreviewForecastPoint {
    pub step: u32,
    pub mean: f64,
    pub q10: f64,
    pub q90: f64,
}

/// Downsampled chartable series shipped in structured output so app views
/// can render without re-reading the RRD artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimeseriesSeriesPreview {
    pub series_id: String,
    #[schemars(length(max = MAX_PREVIEW_POINTS))]
    pub observed: Vec<TimeseriesPreviewObservation>,
    #[schemars(length(max = MAX_PREVIEW_POINTS))]
    pub forecast: Vec<TimeseriesPreviewForecastPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "TimeseriesForecastOutput")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimeseriesForecastOutputBuilder {
    pub result_uri: TimeseriesArtifactUri,
    pub forecast: TimeseriesForecastSummary,
    pub preview: Vec<TimeseriesSeriesPreview>,
    pub artifact: ArtifactMetadata,
}

impl TimeseriesForecastSummaryBuilder {
    pub fn build(self) -> Result<TimeseriesForecastSummary, TimeseriesForecastError> {
        veoveo_types::Checked::new(self).map(TimeseriesForecastSummary)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimeseriesForecastSummary(veoveo_types::Checked<TimeseriesForecastSummaryBuilder>);
impl schemars::JsonSchema for TimeseriesForecastSummary {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <TimeseriesForecastSummaryBuilder as schemars::JsonSchema>::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        <TimeseriesForecastSummaryBuilder as schemars::JsonSchema>::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <TimeseriesForecastSummaryBuilder as schemars::JsonSchema>::json_schema(generator)
    }
}

impl std::ops::Deref for TimeseriesForecastSummary {
    type Target = TimeseriesForecastSummaryBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

impl TimeseriesForecastOutputBuilder {
    pub fn build(self) -> Result<TimeseriesForecastOutput, TimeseriesForecastError> {
        veoveo_types::Checked::new(self).map(TimeseriesForecastOutput)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimeseriesForecastOutput(veoveo_types::Checked<TimeseriesForecastOutputBuilder>);
impl schemars::JsonSchema for TimeseriesForecastOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <TimeseriesForecastOutputBuilder as schemars::JsonSchema>::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        <TimeseriesForecastOutputBuilder as schemars::JsonSchema>::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <TimeseriesForecastOutputBuilder as schemars::JsonSchema>::json_schema(generator)
    }
}

impl std::ops::Deref for TimeseriesForecastOutput {
    type Target = TimeseriesForecastOutputBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

mod relationships;
pub use relationships::{
    MAX_PREVIEW_POINTS, TimeseriesForecastError, validate_forecast_point, validate_forecast_preview,
};

/// Schemas consumed by the server-owned browser App.
pub mod app_schema;

#[cfg(test)]
mod terminal_contract_tests {
    use super::{TimeseriesForecastOutput, TimeseriesForecastRequest};

    #[test]
    fn app_output_fixture_matches_owner_serialization() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/app-output.json")).unwrap();
        let output: TimeseriesForecastOutput = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(output).unwrap(), value);
    }

    #[test]
    fn forecast_input_preserves_the_duckdb_source_contract_schema() {
        let baseline: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/source-contract.schema.json")).unwrap();
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(TimeseriesForecastRequest)).unwrap(),
            baseline
        );
    }

    #[test]
    fn forecast_output_schema_has_one_canonical_result_handoff() {
        let schema = serde_json::to_value(schemars::schema_for!(TimeseriesForecastOutput)).unwrap();
        let properties = schema["properties"].as_object().unwrap();

        assert!(properties.contains_key("resultUri"));
    }
}
