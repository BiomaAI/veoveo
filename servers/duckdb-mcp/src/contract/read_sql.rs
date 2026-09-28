//! SQL fragments for server-materialized DuckDB sources.

use std::{collections::BTreeSet, fmt};

use serde_json::Value;

use super::{DuckDbFormat, DuckDbReadOptions};

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
