//! Read-back over the episodic plane: snapshot dataframe queries across the
//! agent's RRD segments.
//!
//! `QueryEngine::from_rrd_filepath` decodes a segment into memory and stops
//! cleanly at a partial trailing message, so querying is safe while the live
//! segment is still being appended — a query sees everything durable so far.

use std::{collections::VecDeque, path::Path};

use anyhow::{Context, Result};
use re_dataframe::{
    ChunkStoreConfig, EntityPathFilter, QueryEngine, QueryExpression, SparseFillStrategy,
    TimelineName,
    external::arrow::util::display::{ArrayFormatter, FormatOptions},
};
use serde::{Deserialize, Serialize};

const MAX_TIMELINE_OUTPUT_BYTES: usize = 16 * 1024;
const MAX_TIMELINE_CELL_BYTES: usize = 4 * 1024;

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

/// Run a snapshot query over every segment in `rrd_dir` and return the most
/// recent flattened JSON rows (column name → rendered value) in chronological
/// order. Both row count and encoded output size are bounded because these
/// rows can become model input.
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
    let timeline = TimelineName::try_new(&query.timeline).context("invalid timeline name")?;
    let mut rows = RecentRows::new(query.max_rows as usize, MAX_TIMELINE_OUTPUT_BYTES);
    for segment in &segments {
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
                filtered_index: Some(timeline),
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
                    let mut object = serde_json::Map::new();
                    for (column_index, field) in schema.fields().iter().enumerate() {
                        let rendered = cap_text(
                            formatters[column_index].value(row_index).to_string(),
                            MAX_TIMELINE_CELL_BYTES,
                        );
                        if rendered.is_empty() || rendered == "null" {
                            continue;
                        }
                        object.insert(field.name().clone(), serde_json::Value::String(rendered));
                    }
                    if !object.is_empty() {
                        rows.push(bounded_row(object, MAX_TIMELINE_OUTPUT_BYTES));
                    }
                }
            }
        }
    }
    Ok(rows.into_vec())
}

struct RecentRows {
    rows: VecDeque<(serde_json::Value, usize)>,
    bytes: usize,
    max_rows: usize,
    max_bytes: usize,
}

impl RecentRows {
    fn new(max_rows: usize, max_bytes: usize) -> Self {
        Self {
            rows: VecDeque::new(),
            bytes: 0,
            max_rows,
            max_bytes,
        }
    }

    fn push(&mut self, row: serde_json::Value) {
        if self.max_rows == 0 || self.max_bytes == 0 {
            return;
        }
        let row_bytes = encoded_len(&row);
        while !self.rows.is_empty()
            && (self.rows.len() >= self.max_rows
                || encoded_array_len(self.bytes + row_bytes, self.rows.len() + 1) > self.max_bytes)
        {
            if let Some((_, removed_bytes)) = self.rows.pop_front() {
                self.bytes -= removed_bytes;
            }
        }
        if encoded_array_len(row_bytes, 1) <= self.max_bytes {
            self.bytes += row_bytes;
            self.rows.push_back((row, row_bytes));
        }
    }

    fn into_vec(self) -> Vec<serde_json::Value> {
        self.rows.into_iter().map(|(row, _)| row).collect()
    }
}

fn encoded_array_len(row_bytes: usize, row_count: usize) -> usize {
    2 + row_bytes + row_count.saturating_sub(1)
}

fn bounded_row(
    object: serde_json::Map<String, serde_json::Value>,
    max_bytes: usize,
) -> serde_json::Value {
    let mut bounded = serde_json::Map::new();
    let mut skipped = 0usize;
    for (key, value) in object {
        let mut candidate = bounded.clone();
        candidate.insert(key.clone(), value.clone());
        if encoded_len(&serde_json::Value::Object(candidate)) <= max_bytes {
            bounded.insert(key, value);
        } else {
            skipped += 1;
        }
    }
    if skipped > 0 {
        bounded.insert(
            "_truncated_fields".to_owned(),
            serde_json::Value::String(skipped.to_string()),
        );
    }
    serde_json::Value::Object(bounded)
}

fn encoded_len(value: &serde_json::Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |encoded| encoded.len())
}

fn cap_text(mut value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let removed = value.len() - end;
    value.truncate(end);
    format!("{value}... (+{removed} bytes)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_rows_keep_the_newest_bounded_window_in_order() {
        let mut rows = RecentRows::new(3, 1_024);
        for value in 0..5 {
            rows.push(serde_json::json!({"value": value}));
        }
        assert_eq!(
            rows.into_vec(),
            vec![
                serde_json::json!({"value": 2}),
                serde_json::json!({"value": 3}),
                serde_json::json!({"value": 4}),
            ]
        );
    }

    #[test]
    fn recent_rows_bound_encoded_model_input() {
        let mut rows = RecentRows::new(50, 1_024);
        for value in 0..20 {
            rows.push(bounded_row(
                serde_json::Map::from_iter([(
                    "payload".to_owned(),
                    serde_json::Value::String(format!("{value}:{}", "x".repeat(300))),
                )]),
                1_024,
            ));
        }
        let rows = rows.into_vec();
        assert!(encoded_len(&serde_json::Value::Array(rows.clone())) <= 1_024);
        assert!(
            rows.last()
                .and_then(|row| row.get("payload"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value.starts_with("19:"))
        );
    }

    #[test]
    fn text_caps_preserve_utf8_boundaries() {
        let capped = cap_text("flight 🚁".repeat(1_000), 101);
        assert!(capped.is_char_boundary(capped.len()));
        assert!(capped.contains("bytes)"));
    }
}
