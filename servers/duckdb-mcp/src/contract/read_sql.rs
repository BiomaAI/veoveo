//! SQL fragments for server-materialized DuckDB sources.

use super::{DuckDbFormat, DuckDbReadOptionValue, DuckDbReadOptions};

pub fn duckdb_quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn duckdb_quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Render a `read_*` call around a trusted source SQL expression that the
/// materializer has already quoted (one path literal or a list of literals).
pub fn duckdb_read_function_sql(
    source_expr: &str,
    format: &DuckDbFormat,
    options: &DuckDbReadOptions,
) -> String {
    let options = duckdb_read_options_sql(options);
    match format {
        DuckDbFormat::Auto => format!("read_csv_auto({source_expr}{options})"),
        DuckDbFormat::Csv => format!("read_csv({source_expr}{options})"),
        DuckDbFormat::Parquet => format!("read_parquet({source_expr}{options})"),
        DuckDbFormat::Json => format!("read_json({source_expr}{options})"),
        DuckDbFormat::Ndjson => format!("read_ndjson({source_expr}{options})"),
    }
}

pub fn duckdb_read_options_sql(options: &DuckDbReadOptions) -> String {
    let mut fields = Vec::new();
    if let Some(header) = options.header() {
        fields.push(format!(
            "header = {}",
            if header { "true" } else { "false" }
        ));
    }
    if let Some(delimiter) = options.delimiter() {
        fields.push(format!("delim = {}", duckdb_quote_literal(delimiter)));
    }
    if let Some(timestamp_format) = options.timestamp_format() {
        fields.push(format!(
            "timestampformat = {}",
            duckdb_quote_literal(timestamp_format)
        ));
    }
    for (key, value) in options.extra() {
        fields.push(format!("{} = {}", key.as_str(), option_value_sql(value)));
    }
    if fields.is_empty() {
        String::new()
    } else {
        format!(", {}", fields.join(", "))
    }
}

fn option_value_sql(value: &DuckDbReadOptionValue) -> String {
    match value {
        DuckDbReadOptionValue::Bool(value) => value.to_string(),
        DuckDbReadOptionValue::Number(value) => value.to_string(),
        DuckDbReadOptionValue::String(value) => duckdb_quote_literal(value.as_str()),
        DuckDbReadOptionValue::Array(values) => {
            let values = values
                .iter()
                .map(option_value_sql)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{values}]")
        }
    }
}
