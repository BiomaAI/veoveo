use std::collections::BTreeMap;
use veoveo_types::TaskId;

use crate::contract::{
    TimeseriesFilterCombination, TimeseriesFilterPredicate, TimeseriesFilterValue,
    TimeseriesForecastHorizon, TimeseriesForecastMetadata, TimeseriesForecastRequest,
    TimeseriesForecastSummary, TimeseriesPreviewForecastPoint, TimeseriesPreviewObservation,
    TimeseriesRecordingTask, TimeseriesRowFilter, TimeseriesRrdProvenance, TimeseriesSeriesPreview,
    TimeseriesSeriesSummary, TimeseriesSourceProvenance,
};
use anyhow::{Context, Result, bail};
use duckdb::Connection;
use re_log_encoding::Encoder;
use re_sdk::{
    AsComponents, StoreId, StoreKind, TimeCell, TimePoint,
    external::re_log_types::{LogMsg, SetStoreInfo, StoreInfo, StoreSource},
    log::{Chunk, ChunkId, RowId},
};
use re_sdk_types::archetypes::{Scalars, TextDocument};
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_duckdb_mcp::contract::{
    DuckDbFormat, DuckDbTabularSource, duckdb_quote_identifier, duckdb_quote_literal,
    duckdb_read_function_sql, duckdb_read_options_sql,
};
use veoveo_duckdb_runtime::{
    EngineSettings, FileAccess, HttpsSourcePolicy, RequestWorkspace, open_in_memory,
};

const DEFAULT_SERIES_ID: &str = "series";
pub const RRD_MIME_TYPE: &str = "application/vnd.veoveo.rerun-rrd";
pub const RRD_FILENAME: &str = "forecast.rrd";

#[derive(Debug, Clone)]
pub struct ForecastArtifact {
    pub summary: TimeseriesForecastSummary,
    pub preview: Vec<TimeseriesSeriesPreview>,
    pub rrd_bytes: Vec<u8>,
    pub metadata: TimeseriesForecastMetadata,
}

/// Bound on preview points per series so structured output stays small; the
/// full-resolution data lives in the RRD artifact.
pub const PREVIEW_POINTS_PER_SERIES: usize = 500;

fn series_preview(series: &SeriesForecastDocument) -> TimeseriesSeriesPreview {
    TimeseriesSeriesPreview {
        series_id: series.series_id.clone(),
        observed: downsample(&series.observed, PREVIEW_POINTS_PER_SERIES)
            .map(|row| TimeseriesPreviewObservation {
                event_time: row.event_time.clone(),
                value: row.value,
            })
            .collect(),
        forecast: downsample(&series.forecast, PREVIEW_POINTS_PER_SERIES)
            .map(|point| TimeseriesPreviewForecastPoint {
                step: point.step,
                mean: point.mean,
                q10: point.q10,
                q90: point.q90,
            })
            .collect(),
    }
}

/// Even-stride downsampling that always keeps the final point, so the chart
/// tail (the most recent observation) is never dropped.
fn downsample<T>(rows: &[T], cap: usize) -> impl Iterator<Item = &T> {
    let stride = rows.len().div_ceil(cap.max(1)).max(1);
    let last = rows.len().saturating_sub(1);
    rows.iter()
        .enumerate()
        .filter(move |(index, _)| index % stride == 0 || *index == last)
        .map(|(_, row)| row)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct Observation {
    source_row: i64,
    event_time: Option<String>,
    value: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct ForecastPoint {
    step: u32,
    mean: f64,
    q10: f64,
    q90: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct SeriesForecastDocument {
    series_id: String,
    observed_rows: u64,
    observed: Vec<Observation>,
    forecast: Vec<ForecastPoint>,
}

pub fn run_forecast(
    task_id: TaskId,
    request: &TimeseriesForecastRequest,
    source_policy: &HttpsSourcePolicy,
) -> Result<ForecastArtifact> {
    let workspace = RequestWorkspace::new("veoveo-timeseries-")?;
    let conn = open_in_memory(
        &FileAccess::RequestDirectory(workspace.request_dir().to_path_buf()),
        &EngineSettings::new(workspace.spill_dir()),
    )
    .context("opening DuckDB forecast workspace")?;
    materialize_source_table(&conn, request, &workspace, source_policy)?;
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
    let preview: Vec<_> = series_docs.iter().map(series_preview).collect();

    let summary = crate::contract::TimeseriesForecastSummaryBuilder {
        method: request.method,
        horizon: request.horizon,
        source_rows: series_docs
            .iter()
            .try_fold(0_u64, |total, series| {
                total.checked_add(series.observed_rows)
            })
            .context("forecast source count overflow")?,
        series: summaries,
    }
    .build()?;
    for series in &series_docs {
        for point in &series.forecast {
            crate::contract::validate_forecast_point(point.step, point.mean, point.q10, point.q90)?;
        }
    }
    crate::contract::validate_forecast_preview(&summary, &preview)?;
    let source_digest = source_digest(&request.source)?;
    let provenance = TimeseriesRrdProvenance {
        task_id,
        source_digest: veoveo_artifact_contract::UploadSha256::parse(source_digest)?,
        source: source_provenance(&request.source),
        mapping: request.mapping.clone(),
        training_filter: request.training_filter.clone(),
        method: request.method,
        horizon: request.horizon,
    };
    let rrd_bytes = write_rrd(task_id, request, &provenance, &series_docs)?;
    let metadata = TimeseriesForecastMetadata::new(task_id, summary.clone(), provenance)?;
    Ok(ForecastArtifact {
        summary,
        preview,
        rrd_bytes,
        metadata,
    })
}

fn materialize_source_table(
    conn: &Connection,
    request: &TimeseriesForecastRequest,
    workspace: &RequestWorkspace,
    source_policy: &HttpsSourcePolicy,
) -> Result<()> {
    let expression = match &request.source {
        DuckDbTabularSource::InlineCsv { csv, options, .. } => {
            let path = workspace.materialize_inline(
                "inline.csv",
                csv.as_bytes(),
                source_policy.max_bytes,
            )?;
            let path_literal = duckdb_quote_literal(path.to_string_lossy().as_ref());
            format!(
                "read_csv({path_literal}{})",
                duckdb_read_options_sql(options)
            )
        }
        DuckDbTabularSource::Uri {
            uri,
            format,
            options,
        } => {
            let filename = source_filename(0, format);
            let path = workspace.fetch_https(uri, &filename, source_policy)?;
            duckdb_read_function_sql(
                &duckdb_quote_literal(path.to_string_lossy().as_ref()),
                format,
                options,
            )
        }
        DuckDbTabularSource::Uris {
            uris,
            format,
            options,
        } => {
            let list = uris
                .iter()
                .enumerate()
                .map(|(index, uri)| {
                    let filename = source_filename(index, format);
                    workspace
                        .fetch_https(uri, &filename, source_policy)
                        .map(|path| duckdb_quote_literal(path.to_string_lossy().as_ref()))
                })
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            duckdb_read_function_sql(&format!("[{list}]"), format, options)
        }
    };
    conn.execute_batch(&format!(
        "CREATE TEMP TABLE veoveo_source AS SELECT * FROM {expression};"
    ))
    .context("materializing timeseries source through DuckDB")?;
    Ok(())
}

fn source_filename(index: usize, format: &DuckDbFormat) -> String {
    let extension = match format {
        DuckDbFormat::Auto | DuckDbFormat::Csv => "csv",
        DuckDbFormat::Parquet => "parquet",
        DuckDbFormat::Json => "json",
        DuckDbFormat::Ndjson => "ndjson",
    };
    format!("source-{index}.{extension}")
}

fn read_observations(
    conn: &Connection,
    request: &TimeseriesForecastRequest,
) -> Result<BTreeMap<String, Vec<Observation>>> {
    let series_expr = request
        .mapping
        .series_column
        .as_ref()
        .map(|column| {
            format!(
                "CAST({} AS VARCHAR)",
                duckdb_quote_identifier(column.as_str())
            )
        })
        .unwrap_or_else(|| duckdb_quote_literal(DEFAULT_SERIES_ID));
    let time_expr = request
        .mapping
        .time_column
        .as_ref()
        .map(|column| {
            format!(
                "CAST({} AS VARCHAR)",
                duckdb_quote_identifier(column.as_str())
            )
        })
        .unwrap_or_else(|| "NULL".to_string());
    let value_expr = duckdb_quote_identifier(request.mapping.value_column.as_str());
    let filter_clause = request
        .training_filter
        .as_ref()
        .map(row_filter_sql)
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
        WHERE {value_expr} IS NOT NULL AND isfinite(CAST({value_expr} AS DOUBLE))
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

fn row_filter_sql(filter: &TimeseriesRowFilter) -> String {
    let separator = match filter.combination() {
        TimeseriesFilterCombination::All => " AND ",
        TimeseriesFilterCombination::Any => " OR ",
    };
    filter
        .predicates()
        .iter()
        .map(|predicate| format!("({})", row_filter_predicate_sql(predicate)))
        .collect::<Vec<_>>()
        .join(separator)
}

fn row_filter_predicate_sql(predicate: &TimeseriesFilterPredicate) -> String {
    match predicate {
        TimeseriesFilterPredicate::Eq { column, value } => {
            format!(
                "{} = {}",
                duckdb_quote_identifier(column.as_str()),
                filter_value_sql(value)
            )
        }
        TimeseriesFilterPredicate::Ne { column, value } => {
            format!(
                "{} <> {}",
                duckdb_quote_identifier(column.as_str()),
                filter_value_sql(value)
            )
        }
        TimeseriesFilterPredicate::In { column, values } => {
            let values = values
                .as_slice()
                .iter()
                .map(filter_value_sql)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} IN ({values})", duckdb_quote_identifier(column.as_str()))
        }
        TimeseriesFilterPredicate::IsNotNull { column } => {
            format!("{} IS NOT NULL", duckdb_quote_identifier(column.as_str()))
        }
    }
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
        TimeseriesFilterValue::F64(value) => value.get().to_string(),
    }
}

fn forecast_series(rows: &[Observation], horizon: TimeseriesForecastHorizon) -> Vec<ForecastPoint> {
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
    (1..=horizon.get())
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
    task_id: TaskId,
    request: &TimeseriesForecastRequest,
    provenance: &TimeseriesRrdProvenance,
    series_docs: &[SeriesForecastDocument],
) -> Result<Vec<u8>> {
    let mut writer = DeterministicRrdWriter::new(task_id)?;

    let provenance_json = serde_json::to_string_pretty(provenance)?;
    writer
        .log(
            "/timeseries/provenance".to_owned(),
            TimePoint::STATIC,
            &TextDocument::new(provenance_json).with_media_type("application/json"),
        )
        .context("logging RRD provenance")?;
    writer
        .log(
            "/timeseries/task".to_owned(),
            TimePoint::STATIC,
            &TextDocument::new(serde_json::to_string_pretty(&TimeseriesRecordingTask {
                horizon: request.horizon,
                method: request.method,
                task_id,
            })?)
            .with_media_type("application/json"),
        )
        .context("logging RRD task metadata")?;

    for series in series_docs {
        let segment = entity_segment(&series.series_id);
        for row in &series.observed {
            let source_time =
                TimePoint::from([("source_row", TimeCell::from_sequence(row.source_row))]);
            writer
                .log(
                    format!("/timeseries/series/{segment}/observed"),
                    source_time.clone(),
                    &Scalars::single(row.value),
                )
                .with_context(|| format!("logging observed series {}", series.series_id))?;
            if let Some(event_time) = &row.event_time {
                writer
                    .log(
                        format!("/timeseries/series/{segment}/event_time"),
                        source_time,
                        &TextDocument::new(event_time.clone()),
                    )
                    .with_context(|| format!("logging event time for {}", series.series_id))?;
            }
        }
        for point in &series.forecast {
            let forecast_time = TimePoint::from([(
                "forecast_step",
                TimeCell::from_sequence(i64::from(point.step)),
            )]);
            writer
                .log(
                    format!("/timeseries/series/{segment}/forecast/mean"),
                    forecast_time.clone(),
                    &Scalars::single(point.mean),
                )
                .with_context(|| format!("logging forecast mean for {}", series.series_id))?;
            writer
                .log(
                    format!("/timeseries/series/{segment}/forecast/q10"),
                    forecast_time.clone(),
                    &Scalars::single(point.q10),
                )
                .with_context(|| format!("logging forecast q10 for {}", series.series_id))?;
            writer
                .log(
                    format!("/timeseries/series/{segment}/forecast/q90"),
                    forecast_time,
                    &Scalars::single(point.q90),
                )
                .with_context(|| format!("logging forecast q90 for {}", series.series_id))?;
        }
        writer
            .log(
                format!("/timeseries/series/{segment}/summary"),
                TimePoint::STATIC,
                &TextDocument::new(serde_json::to_string_pretty(series)?)
                    .with_media_type("application/json"),
            )
            .with_context(|| format!("logging series summary for {}", series.series_id))?;
    }

    writer.finish()
}

struct DeterministicRrdWriter {
    encoder: Encoder<Vec<u8>>,
    store_id: StoreId,
    task_id: TaskId,
    row_sequence: u64,
}

impl DeterministicRrdWriter {
    fn new(task_id: TaskId) -> Result<Self> {
        let mut encoder = Encoder::local().context("opening deterministic Rerun RRD encoder")?;
        // Rerun's manifest builder iterates an internal hash map when it emits the optional
        // footer. The message stream is deterministic, but that manifest column order is not.
        // Footerless RRD streams remain valid and make task retries byte-for-byte stable.
        encoder.do_not_emit_footer();
        let store_id = StoreId::new(
            StoreKind::Recording,
            "veoveo_timeseries_forecast",
            task_id.to_string(),
        );
        let info = StoreInfo::new_unversioned(
            store_id.clone(),
            StoreSource::Other("veoveo-timeseries-mcp".to_owned()),
        );
        let row_id = deterministic_rerun_id::<RowId>(task_id, 0, "store_info")?;
        encoder
            .append(
                &SetStoreInfo {
                    row_id: *row_id,
                    info,
                }
                .into(),
            )
            .context("encoding deterministic Rerun store info")?;
        Ok(Self {
            encoder,
            store_id,
            task_id,
            row_sequence: 0,
        })
    }

    fn log(
        &mut self,
        entity_path: String,
        timepoint: TimePoint,
        components: &dyn AsComponents,
    ) -> Result<()> {
        let sequence = self.row_sequence;
        let chunk_id = deterministic_rerun_id::<ChunkId>(self.task_id, sequence, "chunk")?;
        let row_id = deterministic_rerun_id::<RowId>(self.task_id, sequence, "row")?;
        let chunk = Chunk::builder_with_id(chunk_id, entity_path)
            .with_archetype(row_id, timepoint, components)
            .build()
            .context("building deterministic Rerun chunk")?;
        self.encoder
            .append(&LogMsg::ArrowMsg(
                self.store_id.clone(),
                chunk
                    .to_arrow_msg()
                    .context("encoding deterministic Rerun chunk")?,
            ))
            .context("writing deterministic Rerun chunk")?;
        self.row_sequence += 1;
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<u8>> {
        self.encoder
            .finish()
            .context("finishing deterministic Rerun RRD encoder")?;
        self.encoder
            .into_inner()
            .context("extracting deterministic Rerun RRD bytes")
    }
}

fn deterministic_rerun_id<T>(task_id: TaskId, sequence: u64, kind: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let digest = Sha256::digest(format!("{task_id}:{kind}:{sequence}"));
    hex::encode(&digest[..16])
        .parse::<T>()
        .map_err(|error| anyhow::anyhow!("invalid deterministic Rerun {kind} id: {error}"))
}

fn source_digest(source: &DuckDbTabularSource) -> Result<String> {
    let json = serde_json::to_vec(source)?;
    Ok(hex::encode(Sha256::digest(json)))
}

fn source_provenance(source: &DuckDbTabularSource) -> TimeseriesSourceProvenance {
    match source {
        DuckDbTabularSource::InlineCsv {
            csv,
            filename,
            options,
        } => TimeseriesSourceProvenance::InlineCsv {
            filename: filename.clone(),
            byte_len: csv.len(),
            options: options.clone(),
        },
        DuckDbTabularSource::Uri {
            uri,
            format,
            options,
        } => TimeseriesSourceProvenance::Uri {
            uri: uri.clone(),
            format: format.clone(),
            options: options.clone(),
        },
        DuckDbTabularSource::Uris {
            uris,
            format,
            options,
        } => TimeseriesSourceProvenance::Uris {
            uris: uris.clone(),
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
    use crate::contract::{
        TimeseriesFilterValue, TimeseriesForecastMethod, TimeseriesForecastRequest,
        TimeseriesTableMapping,
    };
    use serde::Deserialize;
    use serde_json::Value;
    use veoveo_duckdb_mcp::contract::{DuckDbFormat, DuckDbReadOptions, DuckDbTabularSource};

    use super::*;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureManifest {
        schema: FixtureSchema,
        training_filter: TimeseriesRowFilter,
        examples: Vec<FixtureExample>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureSchema {
        time_column: veoveo_duckdb_mcp::contract::DuckDbColumnName,
        value_column: veoveo_duckdb_mcp::contract::DuckDbColumnName,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureExample {
        id: String,
        file: String,
        rows: u64,
        training_rows: u64,
        smoke_horizon: TimeseriesForecastHorizon,
    }

    fn timesfm_manifest() -> FixtureManifest {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/timesfm-showcase/manifest.json");
        let text = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn tail_preserving_preview_has_the_declared_501_bound() {
        let rows = (0..1000).collect::<Vec<_>>();
        let preview = downsample(&rows, PREVIEW_POINTS_PER_SERIES).collect::<Vec<_>>();
        assert_eq!(preview.len(), crate::contract::MAX_PREVIEW_POINTS);
        assert_eq!(preview.last().copied(), rows.last());
    }

    #[test]
    fn finite_observations_that_overflow_forecasting_fail_before_rrd_creation() {
        let input = TimeseriesForecastRequest {
            source: DuckDbTabularSource::InlineCsv {
                csv: "time,value,series\n0,-1e308,a\n1,1e308,a\n".into(),
                filename: None,
                options: DuckDbReadOptions::default().with_header(true),
            },
            mapping: TimeseriesTableMapping {
                time_column: None,
                value_column: "value".parse().unwrap(),
                series_column: Some("series".parse().unwrap()),
            },
            training_filter: None,
            horizon: TimeseriesForecastHorizon::new(2).unwrap(),
            method: TimeseriesForecastMethod::NaiveTrend,
        };
        let error =
            run_forecast(TaskId::new(), &input, &HttpsSourcePolicy::deny_network()).unwrap_err();
        assert!(error.to_string().contains("finite"));
    }

    #[test]
    fn actual_rrd_embeds_current_owned_json_and_recomputed_source_digest() {
        let task = TaskId::new();
        let request = TimeseriesForecastRequest::new(
            DuckDbTabularSource::InlineCsv {
                csv: "value\n1\n2\n".into(),
                filename: Some("series.csv".into()),
                options: DuckDbReadOptions::default()
                    .with_header(true)
                    .with_timestamp_format("%Y-%m-%d")
                    .unwrap(),
            },
            TimeseriesTableMapping::new("value".parse().unwrap()),
            TimeseriesForecastHorizon::new(2).unwrap(),
        );
        let artifact = run_forecast(task, &request, &HttpsSourcePolicy::deny_network()).unwrap();
        let messages = re_log_encoding::Decoder::<LogMsg>::decode_eager(std::io::Cursor::new(
            &artifact.rrd_bytes,
        ))
        .unwrap();
        let mut documents = BTreeMap::new();
        for message in messages {
            if let LogMsg::ArrowMsg(_, arrow) = message.unwrap() {
                let chunk = Chunk::from_arrow_msg(&arrow).unwrap();
                for batch in chunk.iter_component::<re_sdk_types::components::Text>(
                    TextDocument::descriptor_text().component,
                ) {
                    for text in batch.as_slice() {
                        documents.insert(
                            chunk.entity_path().to_string(),
                            serde_json::from_str::<Value>(&text.0).unwrap(),
                        );
                    }
                }
            }
        }
        let provenance = &documents["/timeseries/provenance"];
        assert_eq!(provenance["taskId"], task.to_string());
        assert_eq!(
            provenance["sourceDigest"],
            source_digest(&request.source).unwrap()
        );
        assert_eq!(provenance["mapping"]["valueColumn"], "value");
        assert!(provenance.get("task_id").is_none());
        assert!(provenance.get("source_digest").is_none());
        assert_eq!(documents["/timeseries/task"]["taskId"], task.to_string());
        let series = documents
            .values()
            .find(|value| value.get("seriesId").is_some())
            .unwrap();
        assert_eq!(series["observedRows"], 2);
        assert!(series.get("series_id").is_none());
        assert_eq!(
            serde_json::to_value(&artifact.metadata.provenance).unwrap(),
            *provenance
        );
        let mut obsolete = serde_json::to_value(&request.source).unwrap();
        let value = obsolete["options"]
            .as_object_mut()
            .unwrap()
            .remove("timestampFormat")
            .unwrap();
        obsolete["options"]["timestamp_format"] = value;
        assert!(serde_json::from_value::<DuckDbTabularSource>(obsolete.clone()).is_err());
        assert_ne!(
            source_digest(&request.source).unwrap(),
            hex::encode(Sha256::digest(serde_json::to_vec(&obsolete).unwrap()))
        );
        let current = serde_json::to_vec(&request.source).unwrap();
        assert_eq!(
            source_digest(&request.source).unwrap(),
            hex::encode(Sha256::digest(current))
        );
    }

    #[test]
    fn inline_csv_materializes_and_forecasts() {
        let artifact = run_forecast(
            TaskId::new(),
            &TimeseriesForecastRequest {
                source: DuckDbTabularSource::InlineCsv {
                    csv: "ts,value\n2026-01-01,10\n2026-01-02,12\n2026-01-03,15\n".into(),
                    filename: Some("input.csv".into()),
                    options: DuckDbReadOptions::default().with_header(true),
                },
                mapping: TimeseriesTableMapping {
                    time_column: Some("ts".parse().unwrap()),
                    value_column: "value".parse().unwrap(),
                    series_column: None,
                },
                training_filter: None,
                horizon: TimeseriesForecastHorizon::new(3).unwrap(),
                method: TimeseriesForecastMethod::NaiveTrend,
            },
            &HttpsSourcePolicy::deny_network(),
        )
        .unwrap();

        assert!(!artifact.rrd_bytes.is_empty());
        assert_eq!(artifact.summary.source_rows, 3);
        assert_eq!(artifact.summary.series[0].forecast_rows, 3);
    }

    #[test]
    fn extraction_filters_nonfinite_values_in_sql_and_preserves_source_positions() {
        let request = TimeseriesForecastRequest::new(
            DuckDbTabularSource::InlineCsv {
                csv: "\"quoted value\",split\n10,train\nNaN,train\nInfinity,train\n-Infinity,train\n20,test\n30,train\n".into(),
                filename: None,
                options: DuckDbReadOptions::default().with_header(true),
            },
            TimeseriesTableMapping::new("quoted value".parse().unwrap()),
            TimeseriesForecastHorizon::new(2).unwrap(),
        ).with_training_filter(TimeseriesRowFilter::new(
            TimeseriesFilterCombination::All,
            TimeseriesFilterPredicate::Eq {
                column: "split".parse().unwrap(),
                value: TimeseriesFilterValue::String("train".into()),
            },
            [],
        ));
        let workspace = RequestWorkspace::new("veoveo-timeseries-filter-").unwrap();
        let conn = open_in_memory(
            &FileAccess::RequestDirectory(workspace.request_dir().to_path_buf()),
            &EngineSettings::new(workspace.spill_dir()),
        )
        .unwrap();
        materialize_source_table(
            &conn,
            &request,
            &workspace,
            &HttpsSourcePolicy::deny_network(),
        )
        .unwrap();
        let observations = read_observations(&conn, &request).unwrap();
        let rows = &observations[DEFAULT_SERIES_ID];
        assert_eq!(
            rows.iter()
                .map(|row| (row.source_row, row.value))
                .collect::<Vec<_>>(),
            vec![(0, 10.0), (5, 30.0)]
        );
    }

    #[test]
    fn resumable_forecast_rrd_is_byte_deterministic() {
        let request = TimeseriesForecastRequest {
            source: DuckDbTabularSource::InlineCsv {
                csv: "ts,series,value\n2026-01-01,a,10\n2026-01-02,a,12\n2026-01-01,b,4\n2026-01-02,b,5\n".into(),
                filename: Some("input.csv".into()),
                options: DuckDbReadOptions::default().with_header(true),
            },
            mapping: TimeseriesTableMapping {
                time_column: Some("ts".parse().unwrap()),
                value_column: "value".parse().unwrap(),
                series_column: Some("series".parse().unwrap()),
            },
            training_filter: None,
            horizon: TimeseriesForecastHorizon::new(3).unwrap(),
            method: TimeseriesForecastMethod::NaiveTrend,
        };

        let first = run_forecast(
            "019f0000-0000-7000-8000-000000000001".parse().unwrap(),
            &request,
            &HttpsSourcePolicy::deny_network(),
        )
        .unwrap();
        let second = run_forecast(
            "019f0000-0000-7000-8000-000000000001".parse().unwrap(),
            &request,
            &HttpsSourcePolicy::deny_network(),
        )
        .unwrap();

        assert_eq!(first.summary, second.summary);
        assert_eq!(first.metadata, second.metadata);
        assert_eq!(first.rrd_bytes, second.rrd_bytes);
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
        let csv = std::fs::read_to_string(fixture).unwrap();
        let artifact = run_forecast(
            TaskId::new(),
            &TimeseriesForecastRequest {
                source: DuckDbTabularSource::InlineCsv {
                    csv,
                    filename: Some(example.file.clone()),
                    options: DuckDbReadOptions::default().with_header(true),
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
            &HttpsSourcePolicy::deny_network(),
        )
        .unwrap();

        assert!(!artifact.rrd_bytes.is_empty());
        assert!(example.training_rows < example.rows);
        assert_eq!(artifact.summary.source_rows, example.training_rows);
        assert_eq!(
            artifact.summary.series[0].forecast_rows,
            u64::from(example.smoke_horizon.get())
        );
    }

    #[test]
    fn training_filter_sql_is_typed_and_quoted() {
        let filter = TimeseriesRowFilter::new(
            TimeseriesFilterCombination::All,
            TimeseriesFilterPredicate::Eq {
                column: "split".parse().unwrap(),
                value: TimeseriesFilterValue::String("context".into()),
            },
            [TimeseriesFilterPredicate::In {
                column: "series id".parse().unwrap(),
                values: crate::contract::TimeseriesFilterValues::new(
                    TimeseriesFilterValue::String("a'b".into()),
                    [TimeseriesFilterValue::String("b".into())],
                ),
            }],
        );

        assert_eq!(
            row_filter_sql(&filter),
            "(\"split\" = 'context') AND (\"series id\" IN ('a''b', 'b'))"
        );
    }

    #[test]
    fn source_digest_is_stable() {
        let source = DuckDbTabularSource::InlineCsv {
            csv: "ts,value\n2026-01-01,10\n".into(),
            filename: Some("input.csv".into()),
            options: DuckDbReadOptions::default().with_header(true),
        };

        assert_eq!(
            source_digest(&source).unwrap(),
            source_digest(&source).unwrap()
        );
    }

    #[test]
    fn remote_source_rejects_private_addresses_before_duckdb() {
        let policy = HttpsSourcePolicy::new(["127.0.0.1".to_string()]);
        let error = run_forecast(
            TaskId::new(),
            &TimeseriesForecastRequest {
                source: DuckDbTabularSource::Uri {
                    uri: "https://127.0.0.1/input.csv".parse().unwrap(),
                    format: DuckDbFormat::Csv,
                    options: DuckDbReadOptions::default(),
                },
                mapping: TimeseriesTableMapping {
                    time_column: None,
                    value_column: "value".parse().unwrap(),
                    series_column: None,
                },
                training_filter: None,
                horizon: TimeseriesForecastHorizon::new(1).unwrap(),
                method: TimeseriesForecastMethod::NaiveTrend,
            },
            &policy,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("private or reserved address"));
    }

    #[test]
    fn read_function_uses_typed_format() {
        assert_eq!(
            duckdb_read_function_sql(
                "'s3://bucket/file.parquet'",
                &DuckDbFormat::Parquet,
                &DuckDbReadOptions::default()
            ),
            "read_parquet('s3://bucket/file.parquet')"
        );
    }
}
