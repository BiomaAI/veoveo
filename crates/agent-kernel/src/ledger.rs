//! The kernel ledger: the agent's durable current truth, in one DuckDB file.
//!
//! The `kernel` schema holds runtime bookkeeping (episodes, detached task
//! descriptors, wakes, elicitations, budgets); agent-type domain tables live
//! in `main` via manifest migrations (slice 2+). One connection serves the
//! whole process behind a mutex — every call is a short statement, and DuckDB
//! is single-writer per file.
//!
//! Descriptors are stored as the serde JSON of rig's `ToolTaskDescriptor`, so
//! a suspended task survives process death and rehydrates on the next boot.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use duckdb::{Connection, params};
use uuid::Uuid;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Watching,
    Resolved,
    Failed,
    Cancelled,
    Expired,
}

impl TaskState {
    pub const fn as_str(self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Watching => "watching",
            TaskState::Resolved => "resolved",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
            TaskState::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "pending" => TaskState::Pending,
            "watching" => TaskState::Watching,
            "resolved" => TaskState::Resolved,
            "failed" => TaskState::Failed,
            "cancelled" => TaskState::Cancelled,
            "expired" => TaskState::Expired,
            other => bail!("unknown task state `{other}`"),
        })
    }
}

/// A detached task row that still needs watching.
#[derive(Debug, Clone)]
pub struct WatchableTask {
    pub task_id: String,
    pub tool_name: String,
    pub descriptor_json: String,
    pub state: TaskState,
}

/// A resolved task row ready for consumption by a follow-up episode.
#[derive(Debug, Clone)]
pub struct ResolvedTask {
    pub task_id: String,
    pub tool_name: String,
    pub result_json: String,
    pub result_is_error: bool,
}

#[derive(Clone)]
pub struct KernelLedger {
    conn: Arc<Mutex<Connection>>,
}

const KERNEL_DDL: &str = r#"
CREATE SCHEMA IF NOT EXISTS kernel;
CREATE TABLE IF NOT EXISTS kernel.kv (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
CREATE TABLE IF NOT EXISTS kernel.episodes (
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
CREATE TABLE IF NOT EXISTS kernel.task_ledger (
    task_id TEXT PRIMARY KEY,
    tool_name TEXT NOT NULL,
    server_key TEXT,
    descriptor_json TEXT NOT NULL,
    descriptor_complete BOOLEAN NOT NULL DEFAULT FALSE,
    dispatched_episode TEXT NOT NULL,
    dispatched_at TIMESTAMP NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending','watching','resolved','failed','cancelled','expired')),
    resolved_at TIMESTAMP,
    result_json TEXT,
    result_is_error BOOLEAN,
    consumed_by_episode TEXT
);
CREATE TABLE IF NOT EXISTS kernel.wakes (
    wake_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    dedup_key TEXT NOT NULL,
    payload_json TEXT,
    created_at TIMESTAMP NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('queued','handled','coalesced','dropped')),
    handled_episode TEXT,
    handled_at TIMESTAMP
);
CREATE TABLE IF NOT EXISTS kernel.elicitations (
    elicitation_id TEXT PRIMARY KEY,
    related_task_id TEXT,
    requested_at TIMESTAMP NOT NULL,
    message TEXT,
    schema_json TEXT,
    state TEXT NOT NULL CHECK (state IN ('parked','answered','declined','cancelled','expired')),
    answer_json TEXT,
    answered_at TIMESTAMP,
    answered_by TEXT
);
CREATE TABLE IF NOT EXISTS kernel.budget_ledger (
    window_kind TEXT NOT NULL CHECK (window_kind IN ('hour','day')),
    window_start TIMESTAMP NOT NULL,
    episodes INTEGER NOT NULL,
    input_tokens BIGINT NOT NULL,
    output_tokens BIGINT NOT NULL,
    tool_calls INTEGER NOT NULL,
    PRIMARY KEY (window_kind, window_start)
);
"#;

impl KernelLedger {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening kernel ledger {}", path.display()))?;
        conn.execute_batch(KERNEL_DDL)
            .context("applying kernel schema")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Boot recovery: any episode without an outcome died with the process.
    pub fn mark_inflight_episodes_crashed(&self) -> Result<usize> {
        let changed = self.conn().execute(
            "UPDATE kernel.episodes SET outcome = 'crashed', finished_at = ? WHERE outcome IS NULL",
            params![Utc::now().naive_utc()],
        )?;
        Ok(changed)
    }

    /// Insert an in-flight episode row and return its monotonic sequence.
    pub fn begin_episode(&self, episode_id: Uuid, wake_note: &str) -> Result<i64> {
        let conn = self.conn();
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM kernel.episodes",
            [],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO kernel.episodes (episode_id, seq, started_at, wake_note)
             VALUES (?, ?, ?, ?)",
            params![
                episode_id.to_string(),
                seq,
                Utc::now().naive_utc(),
                wake_note
            ],
        )?;
        Ok(seq)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_episode(
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
            "UPDATE kernel.episodes SET finished_at = ?, outcome = ?, final_output = ?,
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

    pub fn episode_count(&self, outcome: EpisodeOutcome) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM kernel.episodes WHERE outcome = ?",
            params![outcome.as_str()],
            |row| row.get(0),
        )?)
    }

    /// Crash-safety row written the moment a task dispatch is observed, with
    /// the minimal resumable descriptor (backend + task id + tool name).
    pub fn record_provisional_task(
        &self,
        task_id: &str,
        tool_name: &str,
        descriptor_json: &str,
        episode_id: Uuid,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO kernel.task_ledger
                 (task_id, tool_name, descriptor_json, descriptor_complete,
                  dispatched_episode, dispatched_at, state)
             VALUES (?, ?, ?, FALSE, ?, ?, 'pending')
             ON CONFLICT (task_id) DO NOTHING",
            params![
                task_id,
                tool_name,
                descriptor_json,
                episode_id.to_string(),
                Utc::now().naive_utc()
            ],
        )?;
        Ok(())
    }

    /// Upgrade a task row with the full descriptor detached at episode end.
    pub fn record_detached_task(
        &self,
        task_id: &str,
        tool_name: &str,
        server_key: Option<&str>,
        descriptor_json: &str,
        episode_id: Uuid,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO kernel.task_ledger
                 (task_id, tool_name, server_key, descriptor_json, descriptor_complete,
                  dispatched_episode, dispatched_at, state)
             VALUES (?, ?, ?, ?, TRUE, ?, ?, 'pending')
             ON CONFLICT (task_id) DO UPDATE SET
                 descriptor_json = excluded.descriptor_json,
                 descriptor_complete = TRUE,
                 server_key = excluded.server_key",
            params![
                task_id,
                tool_name,
                server_key,
                descriptor_json,
                episode_id.to_string(),
                Utc::now().naive_utc()
            ],
        )?;
        Ok(())
    }

    pub fn set_task_state(&self, task_id: &str, state: TaskState) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE kernel.task_ledger SET state = ? WHERE task_id = ?",
            params![state.as_str(), task_id],
        )?;
        if changed == 0 {
            bail!("task `{task_id}` is not in the ledger");
        }
        Ok(())
    }

    pub fn resolve_task(&self, task_id: &str, result_json: &str, is_error: bool) -> Result<()> {
        let state = if is_error {
            TaskState::Failed
        } else {
            TaskState::Resolved
        };
        let changed = self.conn().execute(
            "UPDATE kernel.task_ledger SET state = ?, resolved_at = ?, result_json = ?,
                 result_is_error = ? WHERE task_id = ?",
            params![
                state.as_str(),
                Utc::now().naive_utc(),
                result_json,
                is_error,
                task_id
            ],
        )?;
        if changed == 0 {
            bail!("task `{task_id}` is not in the ledger");
        }
        Ok(())
    }

    pub fn expire_task(&self, task_id: &str, reason: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE kernel.task_ledger SET state = 'expired', resolved_at = ?, result_json = ?,
                 result_is_error = TRUE WHERE task_id = ?",
            params![
                Utc::now().naive_utc(),
                serde_json::json!({ "expired": reason }).to_string(),
                task_id
            ],
        )?;
        Ok(())
    }

    /// Tasks that need a watcher: detached or interrupted mid-watch.
    pub fn tasks_to_watch(&self) -> Result<Vec<WatchableTask>> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT task_id, tool_name, descriptor_json, state FROM kernel.task_ledger
             WHERE state IN ('pending', 'watching') ORDER BY dispatched_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut tasks = Vec::new();
        for row in rows {
            let (task_id, tool_name, descriptor_json, state) = row?;
            tasks.push(WatchableTask {
                task_id,
                tool_name,
                descriptor_json,
                state: TaskState::parse(&state)?,
            });
        }
        Ok(tasks)
    }

    /// Resolved tasks not yet consumed by a follow-up episode.
    pub fn unconsumed_results(&self) -> Result<Vec<ResolvedTask>> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT task_id, tool_name, result_json, result_is_error FROM kernel.task_ledger
             WHERE state IN ('resolved', 'failed') AND consumed_by_episode IS NULL
             ORDER BY resolved_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ResolvedTask {
                task_id: row.get(0)?,
                tool_name: row.get(1)?,
                result_json: row.get(2)?,
                result_is_error: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn mark_task_consumed(&self, task_id: &str, episode_id: Uuid) -> Result<()> {
        self.conn().execute(
            "UPDATE kernel.task_ledger SET consumed_by_episode = ? WHERE task_id = ?",
            params![episode_id.to_string(), task_id],
        )?;
        Ok(())
    }

    pub fn task_state(&self, task_id: &str) -> Result<TaskState> {
        let state: String = self.conn().query_row(
            "SELECT state FROM kernel.task_ledger WHERE task_id = ?",
            params![task_id],
            |row| row.get(0),
        )?;
        TaskState::parse(&state)
    }

    pub fn kv_set(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.conn().execute(
            "INSERT INTO kernel.kv (key, value_json, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json,
                 updated_at = excluded.updated_at",
            params![key, value.to_string(), Utc::now().naive_utc()],
        )?;
        Ok(())
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT value_json FROM kernel.kv WHERE key = ?")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> (tempfile::TempDir, KernelLedger) {
        let dir = tempfile::tempdir().expect("tempdir");
        let ledger = KernelLedger::open(&dir.path().join("memory.duckdb")).expect("open");
        (dir, ledger)
    }

    #[test]
    fn episode_lifecycle_and_crash_recovery() {
        let (_dir, ledger) = ledger();
        let first = Uuid::new_v4();
        assert_eq!(ledger.begin_episode(first, "boot").expect("begin"), 1);
        ledger
            .finish_episode(first, EpisodeOutcome::Completed, "done", 10, 20, 1, 2, None)
            .expect("finish");

        let stale = Uuid::new_v4();
        assert_eq!(ledger.begin_episode(stale, "wake").expect("begin"), 2);
        assert_eq!(ledger.mark_inflight_episodes_crashed().expect("crash"), 1);
        assert_eq!(
            ledger
                .episode_count(EpisodeOutcome::Crashed)
                .expect("count"),
            1
        );
    }

    #[test]
    fn task_ledger_provisional_upgrade_resolve_consume() {
        let (_dir, ledger) = ledger();
        let episode = Uuid::new_v4();
        ledger.begin_episode(episode, "boot").expect("begin");

        ledger
            .record_provisional_task("t-1", "duckdb__export", r#"{"backend":"mcp"}"#, episode)
            .expect("provisional");
        assert_eq!(ledger.task_state("t-1").expect("state"), TaskState::Pending);

        ledger
            .record_detached_task(
                "t-1",
                "duckdb__export",
                Some("duckdb@1"),
                r#"{"backend":"mcp","task_id":"t-1","tool_name":"duckdb__export"}"#,
                episode,
            )
            .expect("detached");
        let watchable = ledger.tasks_to_watch().expect("watchable");
        assert_eq!(watchable.len(), 1);
        assert!(watchable[0].descriptor_json.contains("duckdb__export"));

        ledger
            .set_task_state("t-1", TaskState::Watching)
            .expect("watching");
        ledger
            .resolve_task("t-1", r#"{"artifact":"duckdb://artifact/abc"}"#, false)
            .expect("resolve");
        assert_eq!(
            ledger.task_state("t-1").expect("state"),
            TaskState::Resolved
        );

        let results = ledger.unconsumed_results().expect("results");
        assert_eq!(results.len(), 1);
        ledger
            .mark_task_consumed("t-1", Uuid::new_v4())
            .expect("consume");
        assert!(ledger.unconsumed_results().expect("results").is_empty());
    }

    #[test]
    fn kv_round_trip() {
        let (_dir, ledger) = ledger();
        assert!(ledger.kv_get("missing").expect("get").is_none());
        ledger
            .kv_set("recording_id", &serde_json::json!("veoveo-agent-test"))
            .expect("set");
        assert_eq!(
            ledger.kv_get("recording_id").expect("get"),
            Some(serde_json::json!("veoveo-agent-test"))
        );
    }
}
