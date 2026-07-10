//! Sandboxed analytical memory for one autonomous agent.
//!
//! SurrealDB owns all scheduling and delivery truth. DuckDB contains domain
//! tables, RRD bookkeeping, and an optional episode projection used only for
//! analysis and context assembly.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use duckdb::{Connection, params};
use uuid::Uuid;
use veoveo_duckdb_runtime::{
    EngineSettings, FileAccess, QueryLimits, open_connection, run_read_only_query,
    validate_single_statement,
};
use veoveo_mcp_contract::{duckdb_quote_identifier, duckdb_quote_literal};

const MEMORY_QUERY_MAX_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeOutcome {
    Completed,
    BudgetTerminated,
    Error,
    Crashed,
}

impl EpisodeOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            EpisodeOutcome::Completed => "completed",
            EpisodeOutcome::BudgetTerminated => "budget_terminated",
            EpisodeOutcome::Error => "error",
            EpisodeOutcome::Crashed => "crashed",
        }
    }
}

#[derive(Clone)]
pub struct MemoryStore {
    conn: Arc<Mutex<Connection>>,
}

const MEMORY_DDL: &str = r#"
CREATE SCHEMA IF NOT EXISTS agent_memory;
CREATE TABLE IF NOT EXISTS agent_memory.migrations (
    idx INTEGER NOT NULL,
    name TEXT PRIMARY KEY,
    applied_at TIMESTAMP NOT NULL
);
CREATE TABLE IF NOT EXISTS agent_memory.kv (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
CREATE TABLE IF NOT EXISTS agent_memory.episode_log (
    episode_id TEXT PRIMARY KEY,
    seq BIGINT NOT NULL,
    started_at TIMESTAMP NOT NULL,
    finished_at TIMESTAMP,
    outcome TEXT CHECK (outcome IN ('completed','budget_terminated','error','crashed')),
    wake_note TEXT,
    input_tokens BIGINT,
    output_tokens BIGINT,
    completion_calls INTEGER,
    tool_calls INTEGER,
    final_output TEXT,
    summary TEXT,
    error TEXT
);
"#;

impl MemoryStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let conn = open_connection(
            path,
            false,
            &[],
            &FileAccess::Denied,
            &EngineSettings::new(path.with_extension("spill")),
        )
        .with_context(|| format!("opening local memory store {}", path.display()))?;
        conn.execute_batch(MEMORY_DDL)
            .context("applying memory schema")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn start_episode_projection(
        &self,
        episode_id: Uuid,
        sequence: i64,
        wake_note: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO agent_memory.episode_log (episode_id, seq, started_at, wake_note)
             VALUES (?, ?, ?, ?)",
            params![
                episode_id.to_string(),
                sequence,
                Utc::now().naive_utc(),
                wake_note
            ],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_episode_projection(
        &self,
        episode_id: Uuid,
        outcome: EpisodeOutcome,
        final_output: &str,
        input_tokens: u64,
        output_tokens: u64,
        completion_calls: u64,
        tool_calls: u64,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE agent_memory.episode_log SET finished_at = ?, outcome = ?, final_output = ?,
                 input_tokens = ?, output_tokens = ?, completion_calls = ?, tool_calls = ?, error = ?
             WHERE episode_id = ?",
            params![
                Utc::now().naive_utc(),
                outcome.as_str(),
                final_output,
                input_tokens as i64,
                output_tokens as i64,
                completion_calls as i64,
                tool_calls as i64,
                error,
                episode_id.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn projected_episode_count(&self, outcome: EpisodeOutcome) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM agent_memory.episode_log WHERE outcome = ?",
            params![outcome.as_str()],
            |row| row.get(0),
        )?)
    }

    pub fn kv_set(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.conn().execute(
            "INSERT INTO agent_memory.kv (key, value_json, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json,
                 updated_at = excluded.updated_at",
            params![key, value.to_string(), Utc::now().naive_utc()],
        )?;
        Ok(())
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT value_json FROM agent_memory.kv WHERE key = ?")?;
        let mut rows = statement.query(params![key])?;
        match rows.next()? {
            Some(row) => {
                let raw: String = row.get(0)?;
                Ok(Some(serde_json::from_str(&raw).map_err(|err| {
                    anyhow!("kv `{key}` holds invalid JSON: {err}")
                })?))
            }
            None => Ok(None),
        }
    }
}

/// Validate the query shape for manifest feedback. Runtime read-only behavior
/// is enforced by a native DuckDB read-only transaction, not this parser.
pub fn ensure_single_select(sql: &str) -> Result<()> {
    validate_single_statement(sql)?;
    let trimmed = sql.trim();
    let lowered = trimmed.to_ascii_lowercase();
    if !(lowered.starts_with("select") || lowered.starts_with("with")) {
        bail!("expected a SELECT (or WITH ... SELECT) statement");
    }
    Ok(())
}

/// One typed mutation for `memory_write`. Raw SQL never reaches this path.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum MemoryWrite {
    Insert {
        table: String,
        row: serde_json::Map<String, serde_json::Value>,
    },
    Update {
        table: String,
        set: serde_json::Map<String, serde_json::Value>,
        r#where: serde_json::Map<String, serde_json::Value>,
    },
    Delete {
        table: String,
        r#where: serde_json::Map<String, serde_json::Value>,
    },
}

impl MemoryWrite {
    pub fn table(&self) -> &str {
        match self {
            MemoryWrite::Insert { table, .. }
            | MemoryWrite::Update { table, .. }
            | MemoryWrite::Delete { table, .. } => table,
        }
    }
}

fn sql_literal(value: &serde_json::Value) -> Result<String> {
    Ok(match value {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(text) => duckdb_quote_literal(text),
        composite @ (serde_json::Value::Array(_) | serde_json::Value::Object(_)) => {
            duckdb_quote_literal(&composite.to_string())
        }
    })
}

fn column_assignments(
    fields: &serde_json::Map<String, serde_json::Value>,
    separator: &str,
) -> Result<String> {
    if fields.is_empty() {
        bail!("at least one column is required");
    }
    let mut parts = Vec::with_capacity(fields.len());
    for (column, value) in fields {
        parts.push(format!(
            "{} = {}",
            duckdb_quote_identifier(column),
            sql_literal(value)?
        ));
    }
    Ok(parts.join(separator))
}

impl MemoryStore {
    /// Apply `NNNN_*.sql` files in lexical order, recording each in
    /// `agent_memory.migrations`; already-applied names are skipped.
    pub fn run_migrations(&self, dir: &Path) -> Result<usize> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .with_context(|| format!("reading migrations dir {}", dir.display()))?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
            .collect();
        entries.sort();

        let conn = self.conn();
        let mut applied = 0usize;
        for (index, path) in entries.iter().enumerate() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow!("migration {} has a non-UTF8 name", path.display()))?
                .to_string();
            let already: i64 = conn.query_row(
                "SELECT COUNT(*) FROM agent_memory.migrations WHERE name = ?",
                params![name],
                |row| row.get(0),
            )?;
            if already > 0 {
                continue;
            }
            let sql = std::fs::read_to_string(path)?;
            conn.execute_batch(&sql)
                .with_context(|| format!("applying migration {name}"))?;
            conn.execute(
                "INSERT INTO agent_memory.migrations (idx, name, applied_at) VALUES (?, ?, ?)",
                params![index as i64, name, Utc::now().naive_utc()],
            )?;
            applied += 1;
        }
        Ok(applied)
    }

    /// Run one guarded read-only SELECT and return rows as JSON objects.
    pub fn query_json(&self, sql: &str, max_rows: u64) -> Result<Vec<serde_json::Value>> {
        ensure_single_select(sql)?;
        let conn = self.conn();
        let result = run_read_only_query(
            &conn,
            sql,
            QueryLimits::interactive(max_rows.max(1), MEMORY_QUERY_MAX_BYTES),
        )?;
        Ok(result
            .rows
            .into_iter()
            .map(|values| {
                serde_json::Value::Object(
                    result
                        .columns
                        .iter()
                        .map(|column| column.name.clone())
                        .zip(values)
                        .collect(),
                )
            })
            .collect())
    }

    /// Apply one typed mutation to an allowlisted domain table and return the
    /// affected row count.
    pub fn write(&self, write: &MemoryWrite, allowed_tables: &[String]) -> Result<usize> {
        let table = write.table();
        if !allowed_tables.iter().any(|allowed| allowed == table) {
            bail!("table `{table}` is not writable for this agent");
        }
        let table_sql = duckdb_quote_identifier(table);
        let statement = match write {
            MemoryWrite::Insert { row, .. } => {
                if row.is_empty() {
                    bail!("insert requires at least one column");
                }
                let columns = row
                    .keys()
                    .map(|column| duckdb_quote_identifier(column))
                    .collect::<Vec<_>>()
                    .join(", ");
                let values = row
                    .values()
                    .map(sql_literal)
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                format!("INSERT INTO {table_sql} ({columns}) VALUES ({values})")
            }
            MemoryWrite::Update { set, r#where, .. } => format!(
                "UPDATE {table_sql} SET {} WHERE {}",
                column_assignments(set, ", ")?,
                column_assignments(r#where, " AND ")?
            ),
            MemoryWrite::Delete { r#where, .. } => format!(
                "DELETE FROM {table_sql} WHERE {}",
                column_assignments(r#where, " AND ")?
            ),
        };
        Ok(self.conn().execute(&statement, [])?)
    }

    pub fn set_episode_projection_summary(&self, episode_id: Uuid, summary: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE agent_memory.episode_log SET summary = ? WHERE episode_id = ?",
            params![summary, episode_id.to_string()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> (tempfile::TempDir, MemoryStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let memory = MemoryStore::open(&dir.path().join("memory.duckdb")).expect("open");
        (dir, memory)
    }

    #[test]
    fn episode_projection_uses_authoritative_sequence() {
        let (_dir, memory) = memory();
        let episode = Uuid::now_v7();
        memory
            .start_episode_projection(episode, 42, "boot")
            .expect("start projection");
        memory
            .finish_episode_projection(
                episode,
                EpisodeOutcome::Completed,
                "done",
                10,
                20,
                1,
                2,
                None,
            )
            .expect("finish");
        assert_eq!(
            memory
                .projected_episode_count(EpisodeOutcome::Completed)
                .expect("count"),
            1
        );
    }

    #[test]
    fn kv_round_trip() {
        let (_dir, memory) = memory();
        assert!(memory.kv_get("missing").expect("get").is_none());
        memory
            .kv_set("recording_id", &serde_json::json!("veoveo-agent-test"))
            .expect("set");
        assert_eq!(
            memory.kv_get("recording_id").expect("get"),
            Some(serde_json::json!("veoveo-agent-test"))
        );
    }

    #[test]
    fn memory_query_keeps_analytical_sql_and_blocks_ambient_access() {
        let (_dir, memory) = memory();
        memory
            .conn()
            .execute_batch(
                "CREATE TABLE readings (sensor VARCHAR, value INTEGER);\n\
                 INSERT INTO readings VALUES ('a', 1), ('a', 3), ('b', 2);",
            )
            .unwrap();

        let rows = memory
            .query_json(
                "WITH ranked AS (\n\
                    SELECT sensor, value, row_number() OVER (PARTITION BY sensor ORDER BY value DESC) AS rank\n\
                 FROM readings)\n\
                 SELECT sensor, value, 'update text is data' AS note FROM ranked WHERE rank = 1 ORDER BY sensor",
                10,
            )
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["sensor"], serde_json::json!("a"));
        assert_eq!(rows[0]["value"], serde_json::json!(3));

        assert!(
            memory
                .query_json("SELECT * FROM read_text('/proc/self/environ')", 10)
                .is_err()
        );
        assert!(
            memory
                .query_json(
                    "WITH selected AS (SELECT value FROM readings) DELETE FROM readings RETURNING value",
                    10,
                )
                .is_err()
        );
        let count: i64 = memory
            .conn()
            .query_row("SELECT count(*) FROM readings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }
}
