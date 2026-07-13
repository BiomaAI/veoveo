//! Local memory tools: the agent's hands on its own two memory planes.
//!
//! These are rig-native tools registered next to the gateway toolset:
//! `memory_query` (guarded read-only SQL over the DuckDB plane),
//! `memory_write` (typed mutations on allowlisted domain tables, mirrored to
//! the RRD plane), and `timeline_query` (snapshot dataframe reads over the
//! RRD segments).

use std::sync::Arc;

use rig_core::{completion::ToolDefinition, tool::Tool};
use serde::{Deserialize, Serialize};

use crate::{
    memory::{MemoryStore, MemoryWrite},
    rrd::RrdRecorder,
    timeline::{TimelineQuery, query_segments},
};

const MAX_QUERY_ROWS: u64 = 500;

#[derive(Debug)]
pub struct MemoryToolError(String);

impl std::fmt::Display for MemoryToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MemoryToolError {}

impl From<anyhow::Error> for MemoryToolError {
    fn from(err: anyhow::Error) -> Self {
        Self(format!("{err:#}"))
    }
}

#[derive(Debug, Deserialize)]
pub struct MemoryQueryArgs {
    /// One read-only SELECT (or WITH ... SELECT) statement.
    pub sql: String,
    #[serde(default)]
    pub max_rows: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct MemoryQueryOutput {
    pub rows: Vec<serde_json::Value>,
    pub row_count: usize,
}

pub struct MemoryQueryTool {
    memory: MemoryStore,
}

impl MemoryQueryTool {
    pub fn new(memory: MemoryStore) -> Self {
        Self { memory }
    }
}

impl Tool for MemoryQueryTool {
    const NAME: &'static str = "memory_query";
    type Error = MemoryToolError;
    type Args = MemoryQueryArgs;
    type Output = MemoryQueryOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Run one read-only SELECT over your own memory database. Kernel state \
                          lives in the `kernel` schema (episodes, task_ledger, wakes); your \
                          domain tables live in `main`."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "sql": { "type": "string", "description": "A single SELECT or WITH statement." },
                    "max_rows": { "type": "integer", "description": "Row cap (default 50, max 500)." }
                },
                "required": ["sql"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let max_rows = args.max_rows.unwrap_or(50).min(MAX_QUERY_ROWS);
        let rows = self.memory.query_json(&args.sql, max_rows)?;
        Ok(MemoryQueryOutput {
            row_count: rows.len(),
            rows,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct MemoryWriteOutput {
    pub affected_rows: usize,
}

pub struct MemoryWriteTool {
    memory: MemoryStore,
    rrd: Arc<RrdRecorder>,
    allowed_tables: Vec<String>,
}

impl MemoryWriteTool {
    pub fn new(memory: MemoryStore, rrd: Arc<RrdRecorder>, allowed_tables: Vec<String>) -> Self {
        Self {
            memory,
            rrd,
            allowed_tables,
        }
    }
}

impl Tool for MemoryWriteTool {
    const NAME: &'static str = "memory_write";
    type Error = MemoryToolError;
    type Args = MemoryWrite;
    type Output = MemoryWriteOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: format!(
                "Record durable conclusions in your memory database with one typed mutation. \
                 Writable tables: {}.",
                self.allowed_tables.join(", ")
            ),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "op": { "type": "string", "enum": ["insert", "update", "delete"] },
                    "table": { "type": "string" },
                    "row": { "type": "object", "description": "Column values for insert." },
                    "set": { "type": "object", "description": "Column values for update." },
                    "where": { "type": "object", "description": "Equality filters for update/delete." }
                },
                "required": ["op", "table"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let affected_rows = self.memory.write(&args, &self.allowed_tables)?;
        // Mirror the mutation onto the episodic plane so the decision log shows
        // when each durable fact changed.
        self.rrd.log_text(
            &format!("/domain/{}", args.table()),
            serde_json::to_string(&args).unwrap_or_else(|_| "unserializable write".to_string()),
        );
        Ok(MemoryWriteOutput { affected_rows })
    }
}

#[derive(Debug, Serialize)]
pub struct TimelineQueryOutput {
    pub rows: Vec<serde_json::Value>,
    pub row_count: usize,
}

pub struct TimelineQueryTool {
    rrd: Arc<RrdRecorder>,
}

impl TimelineQueryTool {
    pub fn new(rrd: Arc<RrdRecorder>) -> Self {
        Self { rrd }
    }
}

impl Tool for TimelineQueryTool {
    const NAME: &'static str = "timeline_query";
    type Error = MemoryToolError;
    type Args = TimelineQuery;
    type Output = TimelineQueryOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Query your episodic decision log (everything you have observed and \
                          done, time-indexed). Entities: /agent/turns, /agent/tools/**, \
                          /agent/tasks/**, /agent/episodes, /domain/**."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "entities": { "type": "string", "description": "Entity path filter, e.g. /agent/** (default /**)." },
                    "timeline": { "type": "string", "description": "Index timeline: log_time (default) or episode." },
                    "max_rows": { "type": "integer", "description": "Row cap (default 50)." }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // The live segment is mid-append; flush so the snapshot sees
        // everything logged before this query.
        self.rrd.flush();
        let rrd_dir = self.rrd.rrd_dir().to_path_buf();
        let rows = tokio::task::spawn_blocking(move || query_segments(&rrd_dir, &args))
            .await
            .map_err(|err| MemoryToolError(err.to_string()))??;
        Ok(TimelineQueryOutput {
            row_count: rows.len(),
            rows,
        })
    }
}
