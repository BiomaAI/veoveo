//! Forecast provenance shared by RRD documents, Artifact metadata and consumers.
use super::{
    TimeseriesForecastHorizon, TimeseriesForecastMethod, TimeseriesForecastSummary,
    TimeseriesRowFilter, TimeseriesTableMapping,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::UploadSha256;
use veoveo_duckdb_mcp::contract::{DuckDbFormat, DuckDbReadOptions, DuckDbSourceUris};
use veoveo_types::{Check, Checked, HttpsUrl, TaskId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeseriesRrdProvenance {
    pub task_id: TaskId,
    pub source_digest: UploadSha256,
    pub source: TimeseriesSourceProvenance,
    pub mapping: TimeseriesTableMapping,
    pub training_filter: Option<TimeseriesRowFilter>,
    pub method: TimeseriesForecastMethod,
    pub horizon: TimeseriesForecastHorizon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TimeseriesSourceProvenance {
    InlineCsv {
        filename: Option<String>,
        byte_len: usize,
        options: DuckDbReadOptions,
    },
    Uri {
        uri: HttpsUrl,
        format: DuckDbFormat,
        options: DuckDbReadOptions,
    },
    Uris {
        uris: DuckDbSourceUris,
        format: DuckDbFormat,
        options: DuckDbReadOptions,
    },
}

/// The recording's static `/timeseries/task` JSON document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeseriesRecordingTask {
    pub horizon: TimeseriesForecastHorizon,
    pub method: TimeseriesForecastMethod,
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "TimeseriesForecastMetadata")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeseriesForecastMetadataBuilder {
    pub task_id: TaskId,
    pub artifact_format: String,
    pub rrd_application_id: String,
    pub summary: TimeseriesForecastSummary,
    pub provenance: TimeseriesRrdProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimeseriesForecastMetadata(Checked<TimeseriesForecastMetadataBuilder>);
impl TimeseriesForecastMetadata {
    pub fn new(
        task_id: TaskId,
        summary: TimeseriesForecastSummary,
        provenance: TimeseriesRrdProvenance,
    ) -> Result<Self, TimeseriesForecastMetadataError> {
        TimeseriesForecastMetadataBuilder {
            task_id,
            artifact_format: "rerun_rrd".into(),
            rrd_application_id: "veoveo_timeseries_forecast".into(),
            summary,
            provenance,
        }
        .build()
    }
}
impl TimeseriesForecastMetadataBuilder {
    pub fn build(self) -> Result<TimeseriesForecastMetadata, TimeseriesForecastMetadataError> {
        Checked::new(self).map(TimeseriesForecastMetadata)
    }
}
impl Check for TimeseriesForecastMetadataBuilder {
    type Error = TimeseriesForecastMetadataError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.task_id != self.provenance.task_id {
            return Err(TimeseriesForecastMetadataError::TaskIdentity);
        }
        if self.summary.method != self.provenance.method
            || self.summary.horizon != self.provenance.horizon
        {
            return Err(TimeseriesForecastMetadataError::Forecast);
        }
        if self.artifact_format != "rerun_rrd"
            || self.rrd_application_id != "veoveo_timeseries_forecast"
        {
            return Err(TimeseriesForecastMetadataError::ArtifactProfile);
        }
        Ok(())
    }
}
impl std::ops::Deref for TimeseriesForecastMetadata {
    type Target = TimeseriesForecastMetadataBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl JsonSchema for TimeseriesForecastMetadata {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        TimeseriesForecastMetadataBuilder::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        TimeseriesForecastMetadataBuilder::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        TimeseriesForecastMetadataBuilder::json_schema(generator)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TimeseriesForecastMetadataError {
    #[error("Timeseries Artifact and provenance Task identities differ")]
    TaskIdentity,
    #[error("Timeseries summary and provenance method or horizon differ")]
    Forecast,
    #[error("Timeseries Artifact format or RRD application differs")]
    ArtifactProfile,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{TimeseriesForecastSummaryBuilder, TimeseriesSeriesSummary};
    fn metadata() -> TimeseriesForecastMetadata {
        let task_id = TaskId::new();
        let horizon = TimeseriesForecastHorizon::new(2).unwrap();
        let summary = TimeseriesForecastSummaryBuilder {
            method: TimeseriesForecastMethod::NaiveTrend,
            horizon,
            source_rows: 2,
            series: vec![TimeseriesSeriesSummary {
                series_id: "series".into(),
                observed_rows: 2,
                forecast_rows: 2,
            }],
        }
        .build()
        .unwrap();
        let provenance = TimeseriesRrdProvenance {
            task_id,
            source_digest: UploadSha256::parse("ab".repeat(32)).unwrap(),
            source: TimeseriesSourceProvenance::InlineCsv {
                filename: None,
                byte_len: 10,
                options: DuckDbReadOptions::default().with_header(true),
            },
            mapping: TimeseriesTableMapping::new("value".parse().unwrap()),
            training_filter: None,
            method: TimeseriesForecastMethod::NaiveTrend,
            horizon,
        };
        TimeseriesForecastMetadata::new(task_id, summary, provenance).unwrap()
    }
    #[test]
    fn metadata_preserves_owner_wire_plain_digest_and_null_fields() {
        let metadata = metadata();
        let value = serde_json::to_value(&metadata).unwrap();
        assert_eq!(value["artifactFormat"], "rerun_rrd");
        assert_eq!(value["rrdApplicationId"], "veoveo_timeseries_forecast");
        assert_eq!(value["provenance"]["sourceDigest"], "ab".repeat(32));
        assert!(value["provenance"]["trainingFilter"].is_null());
        assert!(value["provenance"]["source"]["filename"].is_null());
        assert_eq!(value["provenance"]["source"]["kind"], "inline_csv");
        assert!(value.get("task_id").is_none());
        assert_eq!(
            serde_json::from_value::<TimeseriesForecastMetadata>(value).unwrap(),
            metadata
        );
        let task = TimeseriesRecordingTask {
            horizon: metadata.provenance.horizon,
            method: metadata.provenance.method,
            task_id: metadata.task_id,
        };
        // Preserve the previous json! object's sorted textual key order in the RRD.
        let old =
            serde_json::json!({"taskId":task.task_id,"horizon":task.horizon,"method":task.method});
        assert_eq!(
            serde_json::to_string_pretty(&task).unwrap(),
            serde_json::to_string_pretty(&old).unwrap()
        );
        assert_eq!(
            serde_json::from_value::<TimeseriesRecordingTask>(old).unwrap(),
            task
        );
    }
    #[test]
    fn metadata_decode_rejects_identity_profile_digest_and_forecast_drift() {
        let valid = serde_json::to_value(metadata()).unwrap();
        for (field, replacement) in [
            ("taskId", serde_json::to_value(TaskId::new()).unwrap()),
            ("artifactFormat", serde_json::json!("other")),
            ("rrdApplicationId", serde_json::json!("other")),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = replacement;
            assert!(serde_json::from_value::<TimeseriesForecastMetadata>(invalid).is_err());
        }
        let mut invalid = valid.clone();
        invalid["provenance"]["horizon"] = serde_json::json!(3);
        assert!(serde_json::from_value::<TimeseriesForecastMetadata>(invalid).is_err());
        let mut invalid = valid.clone();
        invalid["provenance"]["sourceDigest"] =
            serde_json::json!(format!("sha256:{}", "ab".repeat(32)));
        assert!(serde_json::from_value::<TimeseriesForecastMetadata>(invalid).is_err());
        let mut invalid = valid.clone();
        invalid["provenance"]["source"]["byte_len"] = serde_json::json!(10);
        assert!(serde_json::from_value::<TimeseriesForecastMetadata>(invalid).is_err());
        let mut invalid = valid;
        invalid["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<TimeseriesForecastMetadata>(invalid).is_err());
    }
}
