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
pub struct TimeseriesSeriesSummary {
    pub series_id: String,
    pub observed_rows: u64,
    pub forecast_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeseriesForecastSummary {
    pub method: TimeseriesForecastMethod,
    pub horizon: TimeseriesForecastHorizon,
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
    use super::{TimeseriesForecastOutput, TimeseriesForecastRequest};

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

        assert!(properties.contains_key("result_uri"));
    }
}
