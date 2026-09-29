//! Tabular input vocabulary owned by DuckDB and reused by analytical consumers.

use std::collections::BTreeMap;

use super::{DuckDbArtifactSourceUri, DuckDbSourceUris};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_types::HttpsUrl;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DuckDbFormat {
    Auto,
    Csv,
    Parquet,
    Json,
    Ndjson,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbReadOptions {
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
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DuckDbSource {
    InlineCsv {
        csv: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
    Uri {
        uri: HttpsUrl,
        format: DuckDbFormat,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
    Uris {
        uris: DuckDbSourceUris,
        format: DuckDbFormat,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
    /// A neutral `artifact://{artifact_id}` reference resolved through the shared
    /// artifact plane under the caller's identity — the cross-server input path.
    /// Any artifact produced by any hosted server (a media output, a timeseries
    /// RRD, an optimization DuckDB snapshot) can be read here, gated by the same
    /// grant + label checks as any other plane read. The server resolves and
    /// materializes the bytes; the SQL engine never touches the network.
    Artifact {
        uri: DuckDbArtifactSourceUri,
        format: DuckDbFormat,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_wire_shape_is_unchanged_by_extraction() {
        let source: DuckDbSource = serde_json::from_str(
            r#"{"kind":"inline_csv","csv":"a,b\n1,2\n","options":{"header":true}}"#,
        )
        .unwrap();
        let DuckDbSource::InlineCsv { csv, options, .. } = source else {
            panic!("expected inline csv");
        };
        assert_eq!(csv, "a,b\n1,2\n");
        assert_eq!(options.header, Some(true));
    }

    #[test]
    fn artifact_source_wire_shape() {
        let artifact_id = veoveo_artifact_contract::ArtifactId::new();
        let json =
            format!(r#"{{"kind":"artifact","uri":"artifact://{artifact_id}","format":"parquet"}}"#);
        let source: DuckDbSource = serde_json::from_str(&json).unwrap();
        let DuckDbSource::Artifact { uri, format, .. } = &source else {
            panic!("expected artifact source");
        };
        assert_eq!(uri.as_str(), format!("artifact://{artifact_id}"));
        assert_eq!(format, &DuckDbFormat::Parquet);
        // round-trips
        let back: DuckDbSource =
            serde_json::from_str(&serde_json::to_string(&source).unwrap()).unwrap();
        assert_eq!(back, source);
    }
}
