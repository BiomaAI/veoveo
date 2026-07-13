//! Read-back over the episodic plane: snapshot dataframe queries across the
//! agent's RRD segments.
//!
//! `QueryEngine::from_rrd_filepath` decodes a segment into memory and stops
//! cleanly at a partial trailing message, so querying is safe while the live
//! segment is still being appended — a query sees everything durable so far.

use std::path::Path;

use anyhow::{Context, Result};
use re_dataframe::{
    ChunkStoreConfig, EntityPathFilter, QueryEngine, QueryExpression, SparseFillStrategy,
    TimelineName,
    external::arrow::util::display::{ArrayFormatter, FormatOptions},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineQuery {
    /// Entity path filter expression, e.g. `/agent/**` (Rerun filter syntax).
    #[serde(default = "default_entity_filter")]
    pub entities: String,
    /// Index timeline to order by (`log_time` or `episode`).
    #[serde(default = "default_timeline")]
    pub timeline: String,
    #[serde(default = "default_max_rows")]
    pub max_rows: u64,
}

fn default_entity_filter() -> String {
    "/**".to_string()
}

fn default_timeline() -> String {
    "log_time".to_string()
}

fn default_max_rows() -> u64 {
    50
}

/// Run a snapshot query over every segment in `rrd_dir`, newest segment last,
/// and return flattened JSON rows (column name → rendered value).
pub fn query_segments(rrd_dir: &Path, query: &TimelineQuery) -> Result<Vec<serde_json::Value>> {
    let mut segments: Vec<_> = std::fs::read_dir(rrd_dir)
        .with_context(|| format!("reading rrd dir {}", rrd_dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rrd"))
        .collect();
    segments.sort();

    let filter = EntityPathFilter::parse_forgiving(&query.entities);
    let mut rows = Vec::new();
    for segment in &segments {
        if rows.len() as u64 >= query.max_rows {
            break;
        }
        let engines = QueryEngine::from_rrd_filepath(&ChunkStoreConfig::DEFAULT, segment)
            .with_context(|| format!("reading rrd segment {}", segment.display()))?;
        for (store_id, engine) in engines {
            if !store_id.is_recording() {
                continue;
            }
            let view_contents = engine
                .iter_entity_paths_sorted(&filter)
                .map(|path| (path, None))
                .collect();
            let expression = QueryExpression {
                view_contents: Some(view_contents),
                filtered_index: Some(TimelineName::new(&query.timeline)),
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
                    if rows.len() as u64 >= query.max_rows {
                        break;
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
                        rows.push(serde_json::Value::Object(object));
                    }
                }
            }
        }
    }
    Ok(rows)
}
