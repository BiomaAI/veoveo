//! Snapshot read-back over spooled segments.
//!
//! `QueryEngine::from_rrd_filepath` decodes a segment and stops cleanly at a
//! partial trailing message, so reading is safe while a live segment is still
//! being appended or after a crash. This module recurses the spool tree
//! (`{dataset}/{day}/*.rrd`) and returns flattened JSON rows, plus row counts
//! per recording — the ground truth the smoke checks against `sensor-sim`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use re_dataframe::{
    ChunkStoreConfig, EntityPathFilter, QueryEngine, QueryExpression, SparseFillStrategy,
    TimelineName,
    external::arrow::util::display::{ArrayFormatter, FormatOptions},
};

/// Recursively collect every `*.rrd` segment under `root`, sorted.
pub fn collect_segments(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_into(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_into(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_into(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rrd") {
            out.push(path);
        }
    }
    Ok(())
}

/// Flattened query result: JSON rows and per-recording row counts.
#[derive(Debug, Default)]
pub struct QueryResult {
    pub rows: Vec<serde_json::Value>,
    /// recording id → number of data rows seen on the queried timeline.
    pub rows_by_recording: BTreeMap<String, u64>,
}

/// Query every segment under `root`, filtering entities and ordering by
/// `timeline`. `max_rows` bounds the returned JSON rows (counts are exact).
pub fn query_tree(
    root: &Path,
    entities: &str,
    timeline: &str,
    max_rows: u64,
) -> Result<QueryResult> {
    let segments = collect_segments(root)?;
    let filter = EntityPathFilter::parse_forgiving(entities);
    let mut result = QueryResult::default();

    for segment in &segments {
        let engines = QueryEngine::from_rrd_filepath(&ChunkStoreConfig::DEFAULT, segment)
            .with_context(|| format!("reading segment {}", segment.display()))?;
        for (store_id, engine) in engines {
            if !store_id.is_recording() {
                continue;
            }
            let recording = store_id.recording_id().as_str().to_owned();
            let view_contents = engine
                .iter_entity_paths_sorted(&filter)
                .map(|path| (path, None))
                .collect();
            let expression = QueryExpression {
                view_contents: Some(view_contents),
                filtered_index: Some(TimelineName::new(timeline)),
                sparse_fill_strategy: SparseFillStrategy::None,
                ..Default::default()
            };
            let mut handle = engine.query(expression);
            let schema = handle.schema().clone();
            for batch in handle.batch_iter() {
                let formatters: Vec<_> = batch
                    .columns()
                    .iter()
                    .map(|column| {
                        ArrayFormatter::try_new(column.as_ref(), &FormatOptions::default())
                    })
                    .collect::<std::result::Result<_, _>>()
                    .context("building arrow formatters")?;
                for row_index in 0..batch.num_rows() {
                    *result
                        .rows_by_recording
                        .entry(recording.clone())
                        .or_default() += 1;
                    if result.rows.len() as u64 >= max_rows {
                        continue;
                    }
                    let mut object = serde_json::Map::new();
                    for (column_index, field) in schema.fields().iter().enumerate() {
                        let rendered = formatters[column_index].value(row_index).to_string();
                        if rendered.is_empty() || rendered == "null" {
                            continue;
                        }
                        object.insert(field.name().clone(), serde_json::Value::String(rendered));
                    }
                    if !object.is_empty() {
                        result.rows.push(serde_json::Value::Object(object));
                    }
                }
            }
        }
    }
    Ok(result)
}
