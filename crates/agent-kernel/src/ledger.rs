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
use veoveo_mcp_contract::{duckdb_quote_identifier, duckdb_quote_literal};

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
CREATE TABLE IF NOT EXISTS kernel.migrations (
    idx INTEGER NOT NULL,
    name TEXT PRIMARY KEY,
    applied_at TIMESTAMP NOT NULL
);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeState {
    Queued,
    Handled,
    Coalesced,
    Dropped,
}

impl WakeState {
    pub const fn as_str(self) -> &'static str {
        match self {
            WakeState::Queued => "queued",
            WakeState::Handled => "handled",
            WakeState::Coalesced => "coalesced",
            WakeState::Dropped => "dropped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElicitationState {
    Parked,
    Answered,
    Declined,
    Cancelled,
}

impl ElicitationState {
    pub const fn as_str(self) -> &'static str {
        match self {
            ElicitationState::Parked => "parked",
            ElicitationState::Answered => "answered",
            ElicitationState::Declined => "declined",
            ElicitationState::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParkedElicitation {
    pub elicitation_id: String,
    pub related_task_id: Option<String>,
    pub message: String,
    pub schema_json: Option<String>,
}

impl KernelLedger {
    pub fn record_wake(
        &self,
        wake_id: Uuid,
        kind: &str,
        dedup_key: &str,
        payload: &serde_json::Value,
        state: WakeState,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO kernel.wakes (wake_id, kind, dedup_key, payload_json, created_at, state)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                wake_id.to_string(),
                kind,
                dedup_key,
                payload.to_string(),
                Utc::now().naive_utc(),
                state.as_str()
            ],
        )?;
        Ok(())
    }

    pub fn mark_wake(
        &self,
        wake_id: Uuid,
        state: WakeState,
        episode_id: Option<Uuid>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE kernel.wakes SET state = ?, handled_episode = ?, handled_at = ?
             WHERE wake_id = ?",
            params![
                state.as_str(),
                episode_id.map(|id| id.to_string()),
                Utc::now().naive_utc(),
                wake_id.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn task_consumed(&self, task_id: &str) -> Result<bool> {
        let consumed: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM kernel.task_ledger
             WHERE task_id = ? AND consumed_by_episode IS NOT NULL",
            params![task_id],
            |row| row.get(0),
        )?;
        Ok(consumed > 0)
    }

    pub fn park_elicitation(
        &self,
        elicitation_id: Uuid,
        related_task_id: Option<&str>,
        message: &str,
        schema_json: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO kernel.elicitations
                 (elicitation_id, related_task_id, requested_at, message, schema_json, state)
             VALUES (?, ?, ?, ?, ?, 'parked')",
            params![
                elicitation_id.to_string(),
                related_task_id,
                Utc::now().naive_utc(),
                message,
                schema_json
            ],
        )?;
        Ok(())
    }

    pub fn answer_elicitation(
        &self,
        elicitation_id: Uuid,
        state: ElicitationState,
        answer_json: Option<&str>,
        answered_by: &str,
    ) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE kernel.elicitations SET state = ?, answer_json = ?, answered_at = ?,
                 answered_by = ? WHERE elicitation_id = ? AND state = 'parked'",
            params![
                state.as_str(),
                answer_json,
                Utc::now().naive_utc(),
                answered_by,
                elicitation_id.to_string()
            ],
        )?;
        if changed == 0 {
            bail!("elicitation `{elicitation_id}` is not parked");
        }
        Ok(())
    }

    pub fn parked_elicitations(&self) -> Result<Vec<ParkedElicitation>> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT elicitation_id, related_task_id, message, schema_json
             FROM kernel.elicitations WHERE state = 'parked' ORDER BY requested_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ParkedElicitation {
                elicitation_id: row.get(0)?,
                related_task_id: row.get(1)?,
                message: row.get(2)?,
                schema_json: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Episodes started in the current UTC hour, for window budgets.
    pub fn episodes_started_this_hour(&self) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM kernel.episodes
             WHERE started_at >= date_trunc('hour', now()::TIMESTAMP)",
            [],
            |row| row.get(0),
        )?)
    }
}

/// Reject anything but one read-only statement: the guard for `memory_query`
/// and manifest context sections. Read-only is a policy here, not an engine
/// property — the kernel owns this database, so the guard is about keeping
/// the model's ad-hoc SQL from mutating state meant for `memory_write`.
pub fn ensure_single_select(sql: &str) -> Result<()> {
    let trimmed = sql.trim().trim_end_matches(';');
    if trimmed.contains(';') {
        bail!("expected a single SQL statement");
    }
    let lowered = trimmed.to_ascii_lowercase();
    if !(lowered.starts_with("select") || lowered.starts_with("with")) {
        bail!("expected a SELECT (or WITH ... SELECT) statement");
    }
    for keyword in [
        "attach", "copy ", "install", "load ", "export", "import", "pragma", "set ", "create",
        "insert", "update ", "delete", "drop", "alter",
    ] {
        if lowered.contains(keyword) {
            bail!("statement contains disallowed keyword `{}`", keyword.trim());
        }
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

impl KernelLedger {
    /// Apply `NNNN_*.sql` files in lexical order, recording each in
    /// `kernel.migrations`; already-applied names are skipped.
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
                "SELECT COUNT(*) FROM kernel.migrations WHERE name = ?",
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
                "INSERT INTO kernel.migrations (idx, name, applied_at) VALUES (?, ?, ?)",
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
        let mut statement = conn.prepare(sql.trim().trim_end_matches(';'))?;
        let mut rows = statement.query([])?;
        let mut column_names: Option<Vec<String>> = None;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            if out.len() as u64 >= max_rows {
                break;
            }
            // Column metadata is only available once the statement executed.
            let names = column_names.get_or_insert_with(|| {
                row.as_ref()
                    .column_names()
                    .into_iter()
                    .map(String::from)
                    .collect()
            });
            let mut object = serde_json::Map::with_capacity(names.len());
            for (index, name) in names.iter().enumerate() {
                object.insert(name.clone(), value_ref_to_json(row.get_ref(index)?));
            }
            out.push(serde_json::Value::Object(object));
        }
        Ok(out)
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

    pub fn set_episode_summary(&self, episode_id: Uuid, summary: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE kernel.episodes SET summary = ? WHERE episode_id = ?",
            params![summary, episode_id.to_string()],
        )?;
        Ok(())
    }
}

fn value_ref_to_json(value: duckdb::types::ValueRef<'_>) -> serde_json::Value {
    use duckdb::types::ValueRef;
    match value {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Boolean(flag) => serde_json::Value::Bool(flag),
        ValueRef::TinyInt(number) => serde_json::json!(number),
        ValueRef::SmallInt(number) => serde_json::json!(number),
        ValueRef::Int(number) => serde_json::json!(number),
        ValueRef::BigInt(number) => serde_json::json!(number),
        ValueRef::HugeInt(number) => serde_json::json!(number.to_string()),
        ValueRef::UTinyInt(number) => serde_json::json!(number),
        ValueRef::USmallInt(number) => serde_json::json!(number),
        ValueRef::UInt(number) => serde_json::json!(number),
        ValueRef::UBigInt(number) => serde_json::json!(number),
        ValueRef::Float(number) => serde_json::json!(number),
        ValueRef::Double(number) => serde_json::json!(number),
        ValueRef::Text(bytes) => serde_json::Value::String(String::from_utf8_lossy(bytes).into()),
        other => serde_json::Value::String(format!("{other:?}")),
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
