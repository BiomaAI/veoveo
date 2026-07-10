//! Shared DuckDB contract types.
//!
//! These model DuckDB-readable data sources shared by hosted Veoveo tools.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Error building DuckDB SQL fragments from typed inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuckDbSqlBuildError(pub String);

impl fmt::Display for DuckDbSqlBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DuckDbSqlBuildError {}

pub fn duckdb_quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn duckdb_quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Render a `read_*` table function call for one typed source expression
/// (a quoted path/URI literal or a `[...]` list of them).
pub fn duckdb_read_function_sql(
    source_expr: &str,
    format: &DuckDbFormat,
    options: &DuckDbReadOptions,
) -> Result<String, DuckDbSqlBuildError> {
    let options = duckdb_read_options_sql(options)?;
    Ok(match format {
        DuckDbFormat::Auto => format!("read_csv_auto({source_expr}{options})"),
        DuckDbFormat::Csv => format!("read_csv({source_expr}{options})"),
        DuckDbFormat::Parquet => format!("read_parquet({source_expr}{options})"),
        DuckDbFormat::Json => format!("read_json({source_expr}{options})"),
        DuckDbFormat::Ndjson => format!("read_ndjson({source_expr}{options})"),
    })
}

pub fn duckdb_read_options_sql(options: &DuckDbReadOptions) -> Result<String, DuckDbSqlBuildError> {
    let mut fields = Vec::new();
    if let Some(header) = options.header {
        fields.push(format!(
            "header = {}",
            if header { "true" } else { "false" }
        ));
    }
    if let Some(delimiter) = &options.delimiter {
        fields.push(format!("delim = {}", duckdb_quote_literal(delimiter)));
    }
    if let Some(timestamp_format) = &options.timestamp_format {
        fields.push(format!(
            "timestampformat = {}",
            duckdb_quote_literal(timestamp_format)
        ));
    }
    let mut seen = BTreeSet::from([
        "header".to_string(),
        "delim".to_string(),
        "timestampformat".to_string(),
    ]);
    for (key, value) in &options.extra {
        validate_option_key(key)?;
        if !seen.insert(key.clone()) {
            return Err(DuckDbSqlBuildError(format!(
                "duplicate DuckDB read option `{key}`"
            )));
        }
        fields.push(format!("{key} = {}", option_value_sql(value)?));
    }
    if fields.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!(", {}", fields.join(", ")))
    }
}

fn validate_option_key(key: &str) -> Result<(), DuckDbSqlBuildError> {
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(DuckDbSqlBuildError(format!(
            "invalid DuckDB read option key `{key}`"
        )));
    }
    Ok(())
}

fn option_value_sql(value: &Value) -> Result<String, DuckDbSqlBuildError> {
    match value {
        Value::Bool(value) => Ok(if *value { "true" } else { "false" }.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => Ok(duckdb_quote_literal(value)),
        Value::Array(values) => {
            let values = values
                .iter()
                .map(option_value_sql)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            Ok(format!("[{values}]"))
        }
        Value::Null | Value::Object(_) => Err(DuckDbSqlBuildError(
            "DuckDB read option values must be bool, number, string, or arrays".to_string(),
        )),
    }
}

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
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DuckDbSource {
    InlineCsv {
        csv: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
    Uri {
        uri: String,
        format: DuckDbFormat,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
    Uris {
        uris: Vec<String>,
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
        uri: String,
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
        let artifact_id = crate::ArtifactId::new();
        let json =
            format!(r#"{{"kind":"artifact","uri":"artifact://{artifact_id}","format":"parquet"}}"#);
        let source: DuckDbSource = serde_json::from_str(&json).unwrap();
        let DuckDbSource::Artifact { uri, format, .. } = &source else {
            panic!("expected artifact source");
        };
        assert_eq!(uri, &format!("artifact://{artifact_id}"));
        assert_eq!(format, &DuckDbFormat::Parquet);
        // round-trips
        let back: DuckDbSource =
            serde_json::from_str(&serde_json::to_string(&source).unwrap()).unwrap();
        assert_eq!(back, source);
    }
}
