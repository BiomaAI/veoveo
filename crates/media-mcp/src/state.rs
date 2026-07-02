use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use rmcp::model::Task;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::provider::Prediction;

#[derive(Debug)]
pub struct PersistedTask {
    pub task: Task,
    pub payload: Option<Value>,
    pub error: Option<String>,
    pub provider_job_id: Option<String>,
}

#[derive(Clone)]
pub struct SqliteState {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteState {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating state db directory {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening state db {}", path.display()))?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS tasks (
                task_id TEXT PRIMARY KEY,
                task_json TEXT NOT NULL,
                provider_job_id TEXT,
                payload_json TEXT,
                error TEXT,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS predictions (
                prediction_id TEXT PRIMARY KEY,
                prediction_json TEXT NOT NULL,
                status TEXT NOT NULL,
                model TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn record_task(
        &self,
        task: &Task,
        payload: Option<&Value>,
        error: Option<&str>,
        provider_job_id: Option<&str>,
    ) -> Result<()> {
        let task_json = serde_json::to_string(task)?;
        let payload_json = payload.map(serde_json::to_string).transpose()?;
        let updated_at = task.last_updated_at.clone();
        let conn = self.conn.lock().expect("sqlite state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO tasks (
                task_id, task_json, provider_job_id, payload_json, error, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(task_id) DO UPDATE SET
                task_json = excluded.task_json,
                provider_job_id = COALESCE(excluded.provider_job_id, tasks.provider_job_id),
                payload_json = COALESCE(excluded.payload_json, tasks.payload_json),
                error = COALESCE(excluded.error, tasks.error),
                updated_at = excluded.updated_at
            "#,
            params![
                task.task_id.as_str(),
                task_json,
                provider_job_id,
                payload_json,
                error,
                updated_at
            ],
        )?;
        Ok(())
    }

    pub fn set_provider_job_id(&self, task_id: &str, provider_job_id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("sqlite state mutex poisoned");
        conn.execute(
            "UPDATE tasks SET provider_job_id = ?2, updated_at = ?3 WHERE task_id = ?1",
            params![task_id, provider_job_id, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn task_id_for_provider_job_id(&self, provider_job_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("sqlite state mutex poisoned");
        conn.query_row(
            "SELECT task_id FROM tasks WHERE provider_job_id = ?1",
            params![provider_job_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn record_prediction(&self, prediction: &Prediction) -> Result<()> {
        let prediction_json = serde_json::to_string(prediction)?;
        let conn = self.conn.lock().expect("sqlite state mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO predictions (
                prediction_id, prediction_json, status, model, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(prediction_id) DO UPDATE SET
                prediction_json = excluded.prediction_json,
                status = excluded.status,
                model = excluded.model,
                updated_at = excluded.updated_at
            "#,
            params![
                prediction.id.as_str(),
                prediction_json,
                prediction.status.as_str(),
                prediction.model.as_str(),
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn load_tasks(&self) -> Result<Vec<PersistedTask>> {
        let conn = self.conn.lock().expect("sqlite state mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT task_json, provider_job_id, payload_json, error FROM tasks ORDER BY updated_at",
        )?;
        let rows = stmt.query_map([], |row| {
            let task_json: String = row.get(0)?;
            let provider_job_id: Option<String> = row.get(1)?;
            let payload_json: Option<String> = row.get(2)?;
            let error: Option<String> = row.get(3)?;
            Ok((task_json, provider_job_id, payload_json, error))
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            let (task_json, provider_job_id, payload_json, error) = row?;
            let task = serde_json::from_str(&task_json)?;
            let payload = payload_json.map(|s| serde_json::from_str(&s)).transpose()?;
            tasks.push(PersistedTask {
                task,
                payload,
                error,
                provider_job_id,
            });
        }
        Ok(tasks)
    }

    pub fn load_predictions(&self) -> Result<Vec<Prediction>> {
        let conn = self.conn.lock().expect("sqlite state mutex poisoned");
        let mut stmt =
            conn.prepare("SELECT prediction_json FROM predictions ORDER BY updated_at")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut predictions = Vec::new();
        for row in rows {
            predictions.push(serde_json::from_str(&row?)?);
        }
        Ok(predictions)
    }

    pub fn prediction(&self, id: &str) -> Result<Option<Prediction>> {
        let conn = self.conn.lock().expect("sqlite state mutex poisoned");
        let json = conn
            .query_row(
                "SELECT prediction_json FROM predictions WHERE prediction_id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(json.map(|s| serde_json::from_str(&s)).transpose()?)
    }
}
