use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use duckdb::Connection;
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::{Scalars, TextDocument};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    DuckDbFormat, DuckDbReadOptions, DuckDbSource, TimeseriesFilterValue, TimeseriesForecastMethod,
    TimeseriesForecastRequest, TimeseriesForecastSummary, TimeseriesRowFilter,
    TimeseriesSeriesSummary, duckdb_quote_identifier, duckdb_quote_literal,
    duckdb_read_function_sql, duckdb_read_options_sql,
};

const DEFAULT_SERIES_ID: &str = "series";
pub const RRD_MIME_TYPE: &str = "application/vnd.veoveo.rerun-rrd";
pub const RRD_FILENAME: &str = "forecast.rrd";

#[derive(Debug, Clone)]
pub struct ForecastArtifact {
    pub summary: TimeseriesForecastSummary,
    pub rrd_bytes: Vec<u8>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
struct Observation {
    source_row: i64,
    event_time: Option<String>,
    value: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ForecastPoint {
    step: u32,
    mean: f64,
    q10: f64,
    q90: f64,
}

#[derive(Debug, Clone, Serialize)]
struct SeriesForecastDocument {
    series_id: String,
    observed_rows: u64,
    observed: Vec<Observation>,
    forecast: Vec<ForecastPoint>,
}

#[derive(Debug, Serialize)]
struct RrdProvenance<'a> {
    task_id: &'a str,
    source_digest: String,
    source: SourceProvenance,
    mapping: &'a veoveo_mcp_contract::TimeseriesTableMapping,
    training_filter: Option<&'a TimeseriesRowFilter>,
    method: &'a TimeseriesForecastMethod,
    horizon: u32,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SourceProvenance {
    InlineCsv {
        filename: Option<String>,
        byte_len: usize,
        options: DuckDbReadOptions,
    },
    Uri {
        uri: String,
        format: DuckDbFormat,
        options: DuckDbReadOptions,
    },
    Uris {
        uris: Vec<String>,
        format: DuckDbFormat,
        options: DuckDbReadOptions,
    },
}

pub fn run_forecast(
    task_id: &str,
    request: &TimeseriesForecastRequest,
) -> Result<ForecastArtifact> {
    validate_request(request)?;
    let conn = Connection::open_in_memory().context("opening DuckDB forecast workspace")?;
    let _inline_file = materialize_source_table(&conn, request)?;
    let observations = read_observations(&conn, request)?;
    if observations.is_empty() {
        bail!("source produced no usable rows");
    }

    let mut series_docs = Vec::new();
    let mut summaries = Vec::new();
    for (series_id, rows) in observations {
        let forecast = forecast_series(&rows, request.horizon);
        summaries.push(TimeseriesSeriesSummary {
            series_id: series_id.clone(),
            observed_rows: rows.len() as u64,
            forecast_rows: forecast.len() as u64,
        });
        series_docs.push(SeriesForecastDocument {
            series_id,
            observed_rows: rows.len() as u64,
            observed: rows,
            forecast,
        });
    }
    summaries.sort_by(|left, right| left.series_id.cmp(&right.series_id));
    series_docs.sort_by(|left, right| left.series_id.cmp(&right.series_id));

    let summary = TimeseriesForecastSummary {
        method: request.method.clone(),
        horizon: request.horizon,
        source_rows: series_docs.iter().map(|series| series.observed_rows).sum(),
        series: summaries,
    };
    let source_digest = source_digest(&request.source)?;
    let provenance = RrdProvenance {
        task_id,
        source_digest,
        source: source_provenance(&request.source),
        mapping: &request.mapping,
        training_filter: request.training_filter.as_ref(),
        method: &request.method,
        horizon: request.horizon,
    };
    let rrd_bytes = write_rrd(task_id, request, &provenance, &series_docs)?;
    let metadata = json!({
        "task_id": task_id,
        "artifact_format": "rerun_rrd",
        "rrd_application_id": "veoveo_timeseries_forecast",
        "summary": summary,
        "provenance": provenance,
    });
    Ok(ForecastArtifact {
        summary,
        rrd_bytes,
        metadata,
    })
}

fn validate_request(request: &TimeseriesForecastRequest) -> Result<()> {
    if request.horizon == 0 {
        bail!("horizon must be greater than zero");
    }
    if request.horizon > 100_000 {
        bail!("horizon must be <= 100000");
    }
    validate_identifier("value_column", &request.mapping.value_column)?;
    if let Some(column) = &request.mapping.time_column {
        validate_identifier("time_column", column)?;
    }
    if let Some(column) = &request.mapping.series_column {
        validate_identifier("series_column", column)?;
    }
    if let DuckDbSource::Uris { uris, .. } = &request.source
        && uris.is_empty()
    {
        bail!("source.uris must not be empty");
    }
    if let Some(filter) = &request.training_filter {
        validate_row_filter(filter)?;
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{label} must not be empty");
    }
    if value.contains('\0') {
        bail!("{label} must not contain NUL bytes");
    }
    Ok(())
}

fn validate_row_filter(filter: &TimeseriesRowFilter) -> Result<()> {
    match filter {
        TimeseriesRowFilter::Eq { column, value } | TimeseriesRowFilter::Ne { column, value } => {
            validate_identifier("filter column", column)?;
            validate_filter_value(value)
        }
        TimeseriesRowFilter::IsNotNull { column } => validate_identifier("filter column", column),
        TimeseriesRowFilter::In { column, values } => {
            validate_identifier("filter column", column)?;
            if values.is_empty() {
                bail!("filter `in` values must not be empty");
            }
            for value in values {
                validate_filter_value(value)?;
            }
            Ok(())
        }
        TimeseriesRowFilter::And { filters } => validate_filter_group("and", filters),
        TimeseriesRowFilter::Or { filters } => validate_filter_group("or", filters),
    }
}

fn validate_filter_value(value: &TimeseriesFilterValue) -> Result<()> {
    if let TimeseriesFilterValue::F64(value) = value
        && !value.is_finite()
    {
        bail!("filter f64 values must be finite");
    }
    Ok(())
}

fn validate_filter_group(op: &str, filters: &[TimeseriesRowFilter]) -> Result<()> {
    if filters.is_empty() {
        bail!("filter `{op}` must contain at least one child filter");
    }
    for filter in filters {
        validate_row_filter(filter)?;
    }
    Ok(())
}

fn materialize_source_table(
    conn: &Connection,
    request: &TimeseriesForecastRequest,
) -> Result<Option<PathBuf>> {
    let mut inline_file = None;
    let expression = match &request.source {
        DuckDbSource::InlineCsv { csv, options, .. } => {
            let path = std::env::temp_dir().join(format!(
                "veoveo-timeseries-{}-{}.csv",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            fs::write(&path, csv).with_context(|| format!("writing {}", path.display()))?;
            let path_literal = duckdb_quote_literal(path.to_string_lossy().as_ref());
            inline_file = Some(path);
            format!(
                "read_csv({path_literal}{})",
                duckdb_read_options_sql(options)?
            )
        }
        DuckDbSource::Uri {
            uri,
            format,
            options,
        } => duckdb_read_function_sql(&duckdb_quote_literal(uri), format, options)?,
        DuckDbSource::Uris {
            uris,
            format,
            options,
        } => {
            let list = uris
                .iter()
                .map(|uri| duckdb_quote_literal(uri))
                .collect::<Vec<_>>()
                .join(", ");
            duckdb_read_function_sql(&format!("[{list}]"), format, options)?
        }
        DuckDbSource::Artifact { .. } => {
            // Cross-server artifact:// input is served by the duckdb server, which
            // holds the artifact-plane client. Query the artifact there, then
            // forecast over the result.
            bail!(
                "artifact:// sources are not supported by timeseries forecast; \
                 read the artifact with the duckdb server instead"
            );
        }
    };
    conn.execute_batch(&format!(
        "CREATE TEMP TABLE veoveo_source AS SELECT * FROM {expression};"
    ))
    .context("materializing timeseries source through DuckDB")?;
    Ok(inline_file)
}

fn read_observations(
    conn: &Connection,
    request: &TimeseriesForecastRequest,
) -> Result<BTreeMap<String, Vec<Observation>>> {
    let series_expr = request
        .mapping
        .series_column
        .as_ref()
        .map(|column| format!("CAST({} AS VARCHAR)", duckdb_quote_identifier(column)))
        .unwrap_or_else(|| duckdb_quote_literal(DEFAULT_SERIES_ID));
    let time_expr = request
        .mapping
        .time_column
        .as_ref()
        .map(|column| format!("CAST({} AS VARCHAR)", duckdb_quote_identifier(column)))
        .unwrap_or_else(|| "NULL".to_string());
    let value_expr = duckdb_quote_identifier(&request.mapping.value_column);
    let filter_clause = request
        .training_filter
        .as_ref()
        .map(row_filter_sql)
        .transpose()?
        .map(|sql| format!(" AND ({sql})"))
        .unwrap_or_default();
    let sql = format!(
        r#"
        SELECT
            {series_expr} AS series_id,
            {time_expr} AS event_time,
            CAST({value_expr} AS DOUBLE) AS value,
            source_row
        FROM (
            SELECT row_number() OVER () - 1 AS source_row, * FROM veoveo_source
        )
        WHERE {value_expr} IS NOT NULL
            {filter_clause}
        ORDER BY series_id, source_row
        "#
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("preparing timeseries extraction query")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .context("extracting timeseries rows")?;
    let mut grouped = BTreeMap::<String, Vec<Observation>>::new();
    for row in rows {
        let (series_id, event_time, value, source_row) = row?;
        if !value.is_finite() {
            continue;
        }
        grouped
            .entry(series_id.unwrap_or_else(|| DEFAULT_SERIES_ID.to_string()))
            .or_default()
            .push(Observation {
                source_row,
                event_time,
                value,
            });
    }
    Ok(grouped)
}

fn row_filter_sql(filter: &TimeseriesRowFilter) -> Result<String> {
    Ok(match filter {
        TimeseriesRowFilter::Eq { column, value } => {
            format!(
                "{} = {}",
                duckdb_quote_identifier(column),
                filter_value_sql(value)
            )
        }
        TimeseriesRowFilter::Ne { column, value } => {
            format!(
                "{} <> {}",
                duckdb_quote_identifier(column),
                filter_value_sql(value)
            )
        }
        TimeseriesRowFilter::In { column, values } => {
            let values = values
                .iter()
                .map(filter_value_sql)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} IN ({values})", duckdb_quote_identifier(column))
        }
        TimeseriesRowFilter::IsNotNull { column } => {
            format!("{} IS NOT NULL", duckdb_quote_identifier(column))
        }
        TimeseriesRowFilter::And { filters } => filters
            .iter()
            .map(|filter| row_filter_sql(filter).map(|sql| format!("({sql})")))
            .collect::<Result<Vec<_>>>()?
            .join(" AND "),
        TimeseriesRowFilter::Or { filters } => filters
            .iter()
            .map(|filter| row_filter_sql(filter).map(|sql| format!("({sql})")))
            .collect::<Result<Vec<_>>>()?
            .join(" OR "),
    })
}

fn filter_value_sql(value: &TimeseriesFilterValue) -> String {
    match value {
        TimeseriesFilterValue::String(value) => duckdb_quote_literal(value),
        TimeseriesFilterValue::Bool(value) => {
            if *value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        TimeseriesFilterValue::I64(value) => value.to_string(),
        TimeseriesFilterValue::U64(value) => value.to_string(),
        TimeseriesFilterValue::F64(value) => value.to_string(),
    }
}

fn forecast_series(rows: &[Observation], horizon: u32) -> Vec<ForecastPoint> {
    let last = rows.last().map(|row| row.value).unwrap_or_default();
    let trend = rows
        .iter()
        .rev()
        .take(2)
        .map(|row| row.value)
        .collect::<Vec<_>>();
    let slope = match trend.as_slice() {
        [last, prev] => *last - *prev,
        _ => 0.0,
    };
    let spread = residual_spread(rows).max(1e-9);
    (1..=horizon)
        .map(|step| {
            let mean = last + slope * f64::from(step);
            ForecastPoint {
                step,
                mean,
                q10: mean - 1.281_551_565_544_600_4 * spread,
                q90: mean + 1.281_551_565_544_600_4 * spread,
            }
        })
        .collect()
}

fn residual_spread(rows: &[Observation]) -> f64 {
    if rows.len() < 2 {
        return 0.0;
    }
    let mean = rows.iter().map(|row| row.value).sum::<f64>() / rows.len() as f64;
    let variance = rows
        .iter()
        .map(|row| {
            let delta = row.value - mean;
            delta * delta
        })
        .sum::<f64>()
        / (rows.len() - 1) as f64;
    variance.sqrt()
}

fn write_rrd(
    task_id: &str,
    request: &TimeseriesForecastRequest,
    provenance: &RrdProvenance<'_>,
    series_docs: &[SeriesForecastDocument],
) -> Result<Vec<u8>> {
    let path = std::env::temp_dir().join(format!(
        "veoveo-timeseries-forecast-{}-{}.rrd",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let rec = RecordingStreamBuilder::new("veoveo_timeseries_forecast")
        .recording_name(format!("forecast {task_id}"))
        .save(&path)
        .context("opening Rerun RRD sink")?;

    let provenance_json = serde_json::to_string_pretty(provenance)?;
    rec.log(
        "/timeseries/provenance",
        &TextDocument::new(provenance_json).with_media_type("application/json"),
    )
    .context("logging RRD provenance")?;
    rec.log(
        "/timeseries/task",
        &TextDocument::new(serde_json::to_string_pretty(&json!({
            "task_id": task_id,
            "horizon": request.horizon,
            "method": request.method,
        }))?)
        .with_media_type("application/json"),
    )
    .context("logging RRD task metadata")?;

    for series in series_docs {
        let segment = entity_segment(&series.series_id);
        for row in &series.observed {
            rec.set_time_sequence("source_row", row.source_row);
            rec.log(
                format!("/timeseries/series/{segment}/observed"),
                &Scalars::single(row.value),
            )
            .with_context(|| format!("logging observed series {}", series.series_id))?;
            if let Some(event_time) = &row.event_time {
                rec.log(
                    format!("/timeseries/series/{segment}/event_time"),
                    &TextDocument::new(event_time.clone()),
                )
                .with_context(|| format!("logging event time for {}", series.series_id))?;
            }
        }
        for point in &series.forecast {
            rec.set_time_sequence("forecast_step", i64::from(point.step));
            rec.log(
                format!("/timeseries/series/{segment}/forecast/mean"),
                &Scalars::single(point.mean),
            )
            .with_context(|| format!("logging forecast mean for {}", series.series_id))?;
            rec.log(
                format!("/timeseries/series/{segment}/forecast/q10"),
                &Scalars::single(point.q10),
            )
            .with_context(|| format!("logging forecast q10 for {}", series.series_id))?;
            rec.log(
                format!("/timeseries/series/{segment}/forecast/q90"),
                &Scalars::single(point.q90),
            )
            .with_context(|| format!("logging forecast q90 for {}", series.series_id))?;
        }
        rec.log(
            format!("/timeseries/series/{segment}/summary"),
            &TextDocument::new(serde_json::to_string_pretty(series)?)
                .with_media_type("application/json"),
        )
        .with_context(|| format!("logging series summary for {}", series.series_id))?;
    }

    rec.flush_blocking().context("flushing Rerun RRD sink")?;
    drop(rec);
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let _ = fs::remove_file(&path);
    Ok(bytes)
}

fn source_digest(source: &DuckDbSource) -> Result<String> {
    let json = serde_json::to_vec(source)?;
    Ok(hex::encode(Sha256::digest(json)))
}

fn source_provenance(source: &DuckDbSource) -> SourceProvenance {
    match source {
        DuckDbSource::InlineCsv {
            csv,
            filename,
            options,
        } => SourceProvenance::InlineCsv {
            filename: filename.clone(),
            byte_len: csv.len(),
            options: options.clone(),
        },
        DuckDbSource::Uri {
            uri,
            format,
            options,
        } => SourceProvenance::Uri {
            uri: uri.clone(),
            format: format.clone(),
            options: options.clone(),
        },
        DuckDbSource::Uris {
            uris,
            format,
            options,
        } => SourceProvenance::Uris {
            uris: uris.clone(),
            format: format.clone(),
            options: options.clone(),
        },
        DuckDbSource::Artifact {
            uri,
            format,
            options,
        } => SourceProvenance::Uri {
            uri: uri.clone(),
            format: format.clone(),
            options: options.clone(),
        },
    }
}

fn entity_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use veoveo_mcp_contract::{
        DuckDbFormat, DuckDbSource, TimeseriesFilterValue, TimeseriesForecastMethod,
        TimeseriesForecastRequest, TimeseriesTableMapping,
    };

    use super::*;

    #[derive(Debug, Deserialize)]
    struct FixtureManifest {
        schema: FixtureSchema,
        training_filter: TimeseriesRowFilter,
        examples: Vec<FixtureExample>,
    }

    #[derive(Debug, Deserialize)]
    struct FixtureSchema {
        time_column: String,
        value_column: String,
    }

    #[derive(Debug, Deserialize)]
    struct FixtureExample {
        id: String,
        file: String,
        rows: u64,
        training_rows: u64,
        smoke_horizon: u32,
    }

    fn timesfm_manifest() -> FixtureManifest {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/timesfm-showcase/manifest.json");
        let text = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn inline_csv_materializes_and_forecasts() {
        let artifact = run_forecast(
            "task-1",
            &TimeseriesForecastRequest {
                source: DuckDbSource::InlineCsv {
                    csv: "ts,value\n2026-01-01,10\n2026-01-02,12\n2026-01-03,15\n".into(),
                    filename: Some("input.csv".into()),
                    options: DuckDbReadOptions {
                        header: Some(true),
                        ..Default::default()
                    },
                },
                mapping: TimeseriesTableMapping {
                    time_column: Some("ts".into()),
                    value_column: "value".into(),
                    series_column: None,
                },
                training_filter: None,
                horizon: 3,
                method: TimeseriesForecastMethod::NaiveTrend,
            },
        )
        .unwrap();

        assert!(!artifact.rrd_bytes.is_empty());
        assert_eq!(artifact.summary.source_rows, 3);
        assert_eq!(artifact.summary.series[0].forecast_rows, 3);
    }

    #[test]
    fn timesfm_showcase_fixture_writes_rrd() {
        let manifest = timesfm_manifest();
        let example = manifest
            .examples
            .iter()
            .find(|example| example.id == "parts_demand_daily")
            .unwrap();
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/timesfm-showcase")
            .join(&example.file);
        let artifact = run_forecast(
            "timesfm-fixture-task",
            &TimeseriesForecastRequest {
                source: DuckDbSource::Uri {
                    uri: fixture.to_string_lossy().into_owned(),
                    format: DuckDbFormat::Csv,
                    options: DuckDbReadOptions {
                        header: Some(true),
                        ..Default::default()
                    },
                },
                mapping: TimeseriesTableMapping {
                    time_column: Some(manifest.schema.time_column.clone()),
                    value_column: manifest.schema.value_column.clone(),
                    series_column: None,
                },
                training_filter: Some(manifest.training_filter.clone()),
                horizon: example.smoke_horizon,
                method: TimeseriesForecastMethod::NaiveTrend,
            },
        )
        .unwrap();

        assert!(!artifact.rrd_bytes.is_empty());
        assert!(example.training_rows < example.rows);
        assert_eq!(artifact.summary.source_rows, example.training_rows);
        assert_eq!(
            artifact.summary.series[0].forecast_rows,
            u64::from(example.smoke_horizon)
        );
    }

    #[test]
    fn training_filter_sql_is_typed_and_quoted() {
        let filter = TimeseriesRowFilter::And {
            filters: vec![
                TimeseriesRowFilter::Eq {
                    column: "split".into(),
                    value: TimeseriesFilterValue::String("context".into()),
                },
                TimeseriesRowFilter::In {
                    column: "series id".into(),
                    values: vec![
                        TimeseriesFilterValue::String("a'b".into()),
                        TimeseriesFilterValue::String("b".into()),
                    ],
                },
            ],
        };

        validate_row_filter(&filter).unwrap();
        assert_eq!(
            row_filter_sql(&filter).unwrap(),
            "(\"split\" = 'context') AND (\"series id\" IN ('a''b', 'b'))"
        );
    }

    #[test]
    fn source_digest_is_stable() {
        let source = DuckDbSource::InlineCsv {
            csv: "ts,value\n2026-01-01,10\n".into(),
            filename: Some("input.csv".into()),
            options: DuckDbReadOptions {
                header: Some(true),
                ..Default::default()
            },
        };

        assert_eq!(
            source_digest(&source).unwrap(),
            source_digest(&source).unwrap()
        );
    }

    #[test]
    fn read_function_uses_typed_format() {
        assert_eq!(
            duckdb_read_function_sql(
                "'s3://bucket/file.parquet'",
                &DuckDbFormat::Parquet,
                &DuckDbReadOptions::default()
            )
            .unwrap(),
            "read_parquet('s3://bucket/file.parquet')"
        );
    }
}
