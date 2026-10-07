//! Local memory tools: the agent's hands on its own two memory planes.
//!
//! These are rig-native tools registered next to the gateway toolset:
//! `memory_query` (guarded read-only SQL over the DuckDB plane),
//! `memory_write` (typed mutations on allowlisted domain tables, mirrored to
//! the RRD plane), and `timeline_query` (snapshot dataframe reads over the
//! RRD segments).

use std::sync::Arc;

use rig::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};

use crate::{
    memory::{MemoryStore, MemoryWrite},
    rrd::RrdRecorder,
    timeline::{TimelineQuery, query_segments},
};

/// Rig advertises the schema of the exact DTO its decoder uses.
pub(crate) fn argument_schema<Args: schemars::JsonSchema>() -> serde_json::Value {
    schemars::schema_for!(Args).into()
}

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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryQueryArgs {
    /// One read-only SELECT (or WITH ... SELECT) statement.
    pub sql: String,
    /// Row cap: omitted or null uses 50; execution caps admitted u64 values at 500.
    #[serde(default)]
    #[schemars(range(max = u64::MAX))]
    pub max_rows: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
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

    fn description(&self) -> String {
        "Run one read-only SELECT over your own memory database. Kernel state lives in the \
         `agent_memory` schema (migrations, kv, episode_log); your domain tables live in `main`."
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        argument_schema::<MemoryQueryArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let max_rows = args.max_rows.unwrap_or(50).min(MAX_QUERY_ROWS);
        let rows = self.memory.query_json(&args.sql, max_rows)?;
        Ok(MemoryQueryOutput {
            row_count: rows.len(),
            rows,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
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

    fn description(&self) -> String {
        format!(
            "Record durable conclusions in your memory database with one typed mutation. \
             Writable tables: {}.",
            self.allowed_tables.join(", ")
        )
    }

    fn parameters(&self) -> serde_json::Value {
        argument_schema::<MemoryWrite>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
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
#[serde(rename_all = "camelCase")]
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

    fn description(&self) -> String {
        "Query the most recent rows of your episodic decision log (everything you have observed \
         and done, time-indexed). Results are returned in chronological order. Entities: \
         /agent/turns, /agent/tools/**, /agent/tasks/**, /agent/episodes, /domain/**."
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        argument_schema::<TimelineQuery>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        mut args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        // The live segment is mid-append; flush so the snapshot sees
        // everything logged before this query.
        self.rrd.flush();
        args.max_rows = args.max_rows.min(MAX_QUERY_ROWS);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn admit<Args: serde::de::DeserializeOwned>(
        schema: &serde_json::Value,
        value: serde_json::Value,
        expected: bool,
    ) {
        let validator = jsonschema::validator_for(schema).unwrap();
        assert_eq!(
            validator.is_valid(&value),
            expected,
            "schema admission for {value}"
        );
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            serde_json::from_slice::<Args>(&bytes).is_ok(),
            expected,
            "decoder admission for {value}"
        );
    }

    #[test]
    fn rig_parameters_match_memory_and_timeline_argument_decoders() {
        let dir = tempfile::tempdir().unwrap();
        let memory = MemoryStore::open(&dir.path().join("memory.duckdb")).unwrap();
        let rrd = Arc::new(
            RrdRecorder::open(dir.path(), "rrd", 1024, "parameters", &memory, None).unwrap(),
        );
        let query = MemoryQueryTool::new(memory.clone());
        let write = MemoryWriteTool::new(memory, rrd.clone(), vec!["readings".into()]);
        let timeline = TimelineQueryTool::new(rrd);
        let query_schema = query.parameters();
        for value in [
            json!({"sql":"SELECT 1"}),
            json!({"sql":"SELECT 1","maxRows":null}),
            json!({"sql":"SELECT 1","maxRows":0}),
            json!({"sql":"SELECT 1","maxRows":u64::MAX}),
        ] {
            admit::<MemoryQueryArgs>(&query_schema, value, true);
        }
        for value in [
            json!({}),
            json!({"sql":1}),
            json!({"sql":"SELECT 1","extra":true}),
            json!({"sql":"SELECT 1","max_rows":4}),
            json!({"sql":"SELECT 1","maxRows":4,"max_rows":4}),
            json!({"sql":"SELECT 1","maxRows":-1}),
            json!({"sql":"SELECT 1","maxRows":0.5}),
            json!({"sql":"SELECT 1","maxRows":1e20}),
        ] {
            admit::<MemoryQueryArgs>(&query_schema, value, false);
        }
        let write_schema = write.parameters();
        for value in [
            json!({"op":"insert","table":"readings","row":{"domain_extension":{"nested":[1,true,null]}}}),
            json!({"op":"update","table":"readings","set":{"value":3},"where":{"sensor":"a","domain_filter":null}}),
            json!({"op":"delete","table":"readings","where":{"sensor":"a"}}),
        ] {
            admit::<MemoryWrite>(&write_schema, value.clone(), true);
            let mut extra = value.clone();
            extra["extra"] = json!(true);
            admit::<MemoryWrite>(&write_schema, extra, false);
            let mut missing_table = value;
            missing_table.as_object_mut().unwrap().remove("table");
            admit::<MemoryWrite>(&write_schema, missing_table, false);
        }
        for value in [
            json!({"op":"insert","table":"readings"}),
            json!({"op":"insert","table":"readings","row":[]}),
            json!({"op":"update","table":"readings","set":{}}),
            json!({"op":"update","table":"readings","where":{}}),
            json!({"op":"delete","table":"readings"}),
            json!({"op":"delete","table":"readings","where":{},"row":{}}),
            json!({"op":"unknown","table":"readings","row":{}}),
            json!({"table":"readings","row":{}}),
            json!({"op":"update","table":"readings","set":[],"where":{}}),
            json!({"op":"delete","table":"readings","where":[]}),
        ] {
            admit::<MemoryWrite>(&write_schema, value, false);
        }
        let timeline_schema = timeline.parameters();
        for value in [
            json!({}),
            json!({"timeline":"custom_capture_index","entities":"/domain/**","maxRows":u64::MAX}),
            json!({"maxRows":0}),
        ] {
            admit::<TimelineQuery>(&timeline_schema, value, true);
        }
        for value in [
            json!({"timeline":1}),
            json!({"entities":null}),
            json!({"maxRows":null}),
            json!({"max_rows":4}),
            json!({"maxRows":4,"max_rows":4}),
            json!({"maxRows":-1}),
            json!({"maxRows":1e20}),
            json!({"extra":true}),
        ] {
            admit::<TimelineQuery>(&timeline_schema, value, false);
        }
        let defaults: TimelineQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults.entities, "/**");
        assert_eq!(defaults.timeline, "log_time");
        assert_eq!(defaults.max_rows, 50);
        assert_eq!(timeline_schema["properties"]["maxRows"]["default"], 50);
        assert!(query.description().contains("agent_memory"));
        assert!(!query.description().contains("task_ledger"));
    }

    #[tokio::test]
    async fn memory_query_execution_preserves_default_and_caps_admitted_u64() {
        let dir = tempfile::tempdir().unwrap();
        let memory = MemoryStore::open(&dir.path().join("memory.duckdb")).unwrap();
        let tool = MemoryQueryTool::new(memory);
        let mut context = ToolContext::new();
        for (max_rows, expected) in [(None, 50), (Some(u64::MAX), 500)] {
            let result = tool
                .call(
                    &mut context,
                    MemoryQueryArgs {
                        sql: "SELECT * FROM range(600)".into(),
                        max_rows,
                    },
                )
                .await
                .unwrap();
            assert_eq!(result.row_count, expected);
        }
    }
}
