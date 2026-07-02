use std::{collections::BTreeSet, path::Path};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use duckdb::{OptionalExt, params};
use rmcp::model::{Task, TaskStatus};
use serde_json::Value;
use veoveo_mcp_contract::{
    ArtifactMetadata, DataLabelId, DuckDbAnalytics, GatewayProfileId, PrincipalId,
    SharedDuckDbConnection, TenantId, UsageRecord, open_duckdb,
};

use crate::provider::Prediction;

fn parse_rfc3339_utc(timestamp: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(timestamp)
        .with_context(|| format!("parsing RFC3339 timestamp {timestamp:?}"))
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

#[derive(Debug)]
pub struct PersistedTask {
    pub task: Task,
    pub payload: Option<Value>,
    pub error: Option<String>,
    pub provider_job_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaStateGcSummary {
    pub tasks_deleted: u64,
    pub task_owners_deleted: u64,
    pub predictions_deleted: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOwner {
    pub task_id: String,
    pub principal_id: PrincipalId,
    pub profile: GatewayProfileId,
    pub tenant: Option<TenantId>,
    pub data_labels: BTreeSet<DataLabelId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactOwner {
    pub sha256: String,
    pub task_id: String,
    pub principal_id: PrincipalId,
    pub profile: GatewayProfileId,
    pub tenant: Option<TenantId>,
    pub data_labels: BTreeSet<DataLabelId>,
}

#[derive(Clone)]
pub struct DuckdbState {
    conn: SharedDuckDbConnection,
    analytics: DuckDbAnalytics,
}

impl DuckdbState {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = open_duckdb(path)?;
        let analytics = DuckDbAnalytics::from_connection(conn.clone())?;
        {
            let conn = conn.lock();
            conn.execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS tasks (
                task_id TEXT PRIMARY KEY,
                task_json TEXT NOT NULL,
                provider_job_id TEXT,
                payload_json TEXT,
                error TEXT,
                updated_at TIMESTAMP NOT NULL
            );

            CREATE TABLE IF NOT EXISTS predictions (
                prediction_id TEXT PRIMARY KEY,
                prediction_json TEXT NOT NULL,
                status TEXT NOT NULL,
                model TEXT NOT NULL,
                updated_at TIMESTAMP NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artifacts (
                sha256 TEXT PRIMARY KEY,
                metadata_json TEXT NOT NULL,
                updated_at TIMESTAMP NOT NULL
            );

            CREATE TABLE IF NOT EXISTS task_owners (
                task_id TEXT PRIMARY KEY,
                principal_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                tenant TEXT,
                data_labels_json TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artifact_owners (
                sha256 TEXT NOT NULL,
                task_id TEXT NOT NULL,
                principal_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                tenant TEXT,
                data_labels_json TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL,
                PRIMARY KEY (sha256, task_id, principal_id)
            );
            "#,
            )?;
        }
        Ok(Self { conn, analytics })
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
        let updated_at = parse_rfc3339_utc(&task.last_updated_at)?;
        let conn = self.conn.lock();
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
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE tasks SET provider_job_id = ?2, updated_at = ?3 WHERE task_id = ?1",
            params![task_id, provider_job_id, chrono::Utc::now()],
        )?;
        Ok(())
    }

    pub fn task_id_for_provider_job_id(&self, provider_job_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
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
        let conn = self.conn.lock();
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
                chrono::Utc::now()
            ],
        )?;
        Ok(())
    }

    pub fn load_tasks(&self) -> Result<Vec<PersistedTask>> {
        let conn = self.conn.lock();
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

    pub fn delete_terminal_tasks_before(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<MediaStateGcSummary> {
        let mut summary = MediaStateGcSummary::default();
        for task in self.load_tasks()? {
            if !matches!(
                task.task.status,
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
            ) {
                continue;
            }
            let updated_at = parse_rfc3339_utc(&task.task.last_updated_at)?;
            if updated_at >= cutoff {
                continue;
            }
            let deleted =
                self.delete_task_state(&task.task.task_id, task.provider_job_id.as_deref())?;
            summary.tasks_deleted += deleted.tasks_deleted;
            summary.task_owners_deleted += deleted.task_owners_deleted;
            summary.predictions_deleted += deleted.predictions_deleted;
        }
        Ok(summary)
    }

    pub fn delete_task_state(
        &self,
        task_id: &str,
        provider_job_id: Option<&str>,
    ) -> Result<MediaStateGcSummary> {
        let conn = self.conn.lock();
        let task_owners_deleted = conn.execute(
            "DELETE FROM task_owners WHERE task_id = ?1",
            params![task_id],
        )?;
        let tasks_deleted =
            conn.execute("DELETE FROM tasks WHERE task_id = ?1", params![task_id])?;
        let predictions_deleted = match provider_job_id {
            Some(provider_job_id) => conn.execute(
                "DELETE FROM predictions WHERE prediction_id = ?1",
                params![provider_job_id],
            )?,
            None => 0,
        };
        Ok(MediaStateGcSummary {
            tasks_deleted: u64::try_from(tasks_deleted)?,
            task_owners_deleted: u64::try_from(task_owners_deleted)?,
            predictions_deleted: u64::try_from(predictions_deleted)?,
        })
    }

    pub fn record_task_owner(&self, owner: &TaskOwner) -> Result<()> {
        let data_labels_json = data_labels_to_json(&owner.data_labels)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO task_owners (
                task_id, principal_id, profile, tenant, data_labels_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(task_id) DO UPDATE SET
                principal_id = excluded.principal_id,
                profile = excluded.profile,
                tenant = excluded.tenant,
                data_labels_json = excluded.data_labels_json,
                updated_at = excluded.updated_at
            "#,
            params![
                owner.task_id.as_str(),
                owner.principal_id.as_str(),
                owner.profile.as_str(),
                owner.tenant.as_ref().map(TenantId::as_str),
                data_labels_json,
                chrono::Utc::now()
            ],
        )?;
        Ok(())
    }

    pub fn load_task_owners(&self) -> Result<Vec<TaskOwner>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT task_id, principal_id, profile, tenant, data_labels_json FROM task_owners",
        )?;
        let rows = stmt.query_map([], |row| {
            let task_id: String = row.get(0)?;
            let principal_id: String = row.get(1)?;
            let profile: String = row.get(2)?;
            let tenant: Option<String> = row.get(3)?;
            let data_labels_json: String = row.get(4)?;
            Ok((task_id, principal_id, profile, tenant, data_labels_json))
        })?;
        let mut owners = Vec::new();
        for row in rows {
            let (task_id, principal_id, profile, tenant, data_labels_json) = row?;
            owners.push(TaskOwner {
                task_id,
                principal_id: PrincipalId::new(principal_id)?,
                profile: GatewayProfileId::new(profile)?,
                tenant: tenant.map(TenantId::new).transpose()?,
                data_labels: data_labels_from_json(&data_labels_json)?,
            });
        }
        Ok(owners)
    }

    pub fn task_owner(&self, task_id: &str) -> Result<Option<TaskOwner>> {
        let conn = self.conn.lock();
        let row = conn
            .query_row(
                "SELECT principal_id, profile, tenant, data_labels_json FROM task_owners WHERE task_id = ?1",
                params![task_id],
                |row| {
                    let principal_id: String = row.get(0)?;
                    let profile: String = row.get(1)?;
                    let tenant: Option<String> = row.get(2)?;
                    let data_labels_json: String = row.get(3)?;
                    Ok((principal_id, profile, tenant, data_labels_json))
                },
            )
            .optional()?;
        row.map(|(principal_id, profile, tenant, data_labels_json)| {
            Ok(TaskOwner {
                task_id: task_id.to_string(),
                principal_id: PrincipalId::new(principal_id)?,
                profile: GatewayProfileId::new(profile)?,
                tenant: tenant.map(TenantId::new).transpose()?,
                data_labels: data_labels_from_json(&data_labels_json)?,
            })
        })
        .transpose()
    }

    pub fn load_predictions(&self) -> Result<Vec<Prediction>> {
        let conn = self.conn.lock();
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
        let conn = self.conn.lock();
        let json = conn
            .query_row(
                "SELECT prediction_json FROM predictions WHERE prediction_id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(json.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    pub fn record_artifact(&self, metadata: &ArtifactMetadata) -> Result<()> {
        let metadata_json = serde_json::to_string(metadata)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO artifacts (sha256, metadata_json, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(sha256) DO UPDATE SET
                metadata_json = excluded.metadata_json,
                updated_at = excluded.updated_at
            "#,
            params![metadata.sha256.as_str(), metadata_json, chrono::Utc::now()],
        )?;
        Ok(())
    }

    pub fn artifact(&self, sha256: &str) -> Result<Option<ArtifactMetadata>> {
        let conn = self.conn.lock();
        let json = conn
            .query_row(
                "SELECT metadata_json FROM artifacts WHERE sha256 = ?1",
                params![sha256],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(json.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    pub fn list_artifacts(&self) -> Result<Vec<ArtifactMetadata>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT metadata_json FROM artifacts ORDER BY updated_at")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut artifacts = Vec::new();
        for row in rows {
            artifacts.push(serde_json::from_str(&row?)?);
        }
        Ok(artifacts)
    }

    pub fn delete_artifact_metadata(&self, sha256: &str) -> Result<u64> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM artifact_owners WHERE sha256 = ?1",
            params![sha256],
        )?;
        let deleted = conn.execute("DELETE FROM artifacts WHERE sha256 = ?1", params![sha256])?;
        Ok(u64::try_from(deleted)?)
    }

    pub fn record_artifact_owner(&self, owner: &ArtifactOwner) -> Result<()> {
        let data_labels_json = data_labels_to_json(&owner.data_labels)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO artifact_owners (
                sha256, task_id, principal_id, profile, tenant, data_labels_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ON CONFLICT(sha256, task_id, principal_id) DO UPDATE SET
                profile = excluded.profile,
                tenant = excluded.tenant,
                data_labels_json = excluded.data_labels_json,
                updated_at = excluded.updated_at
            "#,
            params![
                owner.sha256.as_str(),
                owner.task_id.as_str(),
                owner.principal_id.as_str(),
                owner.profile.as_str(),
                owner.tenant.as_ref().map(TenantId::as_str),
                data_labels_json,
                chrono::Utc::now()
            ],
        )?;
        Ok(())
    }

    pub fn artifact_owners(&self, sha256: &str) -> Result<Vec<ArtifactOwner>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT task_id, principal_id, profile, tenant, data_labels_json FROM artifact_owners WHERE sha256 = ?1",
        )?;
        let rows = stmt.query_map(params![sha256], |row| {
            let task_id: String = row.get(0)?;
            let principal_id: String = row.get(1)?;
            let profile: String = row.get(2)?;
            let tenant: Option<String> = row.get(3)?;
            let data_labels_json: String = row.get(4)?;
            Ok((task_id, principal_id, profile, tenant, data_labels_json))
        })?;
        let mut owners = Vec::new();
        for row in rows {
            let (task_id, principal_id, profile, tenant, data_labels_json) = row?;
            owners.push(ArtifactOwner {
                sha256: sha256.to_string(),
                task_id,
                principal_id: PrincipalId::new(principal_id)?,
                profile: GatewayProfileId::new(profile)?,
                tenant: tenant.map(TenantId::new).transpose()?,
                data_labels: data_labels_from_json(&data_labels_json)?,
            });
        }
        Ok(owners)
    }

    pub fn record_usage(&self, record: &UsageRecord) -> Result<()> {
        self.analytics.record_usage(record)
    }

    pub fn usage_records(&self, task_id: &str) -> Result<Vec<UsageRecord>> {
        self.analytics.usage_records(task_id)
    }

    pub fn has_actual_usage(&self, task_id: &str, provider_job_id: &str) -> Result<bool> {
        self.analytics.has_actual_usage(task_id, provider_job_id)
    }

    pub fn usage_task_ids(&self) -> Result<Vec<String>> {
        self.analytics.usage_task_ids()
    }

    pub fn delete_usage_records_before(&self, cutoff: DateTime<Utc>) -> Result<u64> {
        self.analytics.delete_usage_records_before(cutoff)
    }
}

fn data_labels_to_json(labels: &BTreeSet<DataLabelId>) -> Result<String> {
    let labels = labels.iter().map(DataLabelId::as_str).collect::<Vec<_>>();
    Ok(serde_json::to_string(&labels)?)
}

fn data_labels_from_json(value: &str) -> Result<BTreeSet<DataLabelId>> {
    let labels = serde_json::from_str::<Vec<String>>(value)?;
    labels
        .into_iter()
        .map(DataLabelId::new)
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "veoveo-media-state-{label}-{}.duckdb",
            std::process::id()
        ))
    }

    fn owner(task_id: &str) -> TaskOwner {
        TaskOwner {
            task_id: task_id.to_string(),
            principal_id: PrincipalId::new("https://idp.example.com#user-1").unwrap(),
            profile: GatewayProfileId::new("default").unwrap(),
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            data_labels: BTreeSet::from([
                DataLabelId::new("cui").unwrap(),
                DataLabelId::new("pii").unwrap(),
            ]),
        }
    }

    fn task(task_id: &str, status: TaskStatus, timestamp: DateTime<Utc>) -> Task {
        Task::new(
            task_id.to_string(),
            status,
            timestamp.to_rfc3339(),
            timestamp.to_rfc3339(),
        )
    }

    fn prediction(prediction_id: &str) -> Prediction {
        Prediction {
            id: prediction_id.to_string(),
            model: "model-1".to_string(),
            outputs: Vec::new(),
            urls: None,
            status: "completed".to_string(),
            created_at: None,
            error: None,
            execution_time: None,
            timings: None,
            input: None,
        }
    }

    #[test]
    fn task_owner_round_trips() {
        let path = temp_path("task-owner");
        let state = DuckdbState::open(&path).unwrap();
        let owner = owner("task-1");

        state.record_task_owner(&owner).unwrap();

        assert_eq!(state.task_owner("task-1").unwrap(), Some(owner.clone()));
        assert_eq!(state.load_task_owners().unwrap(), vec![owner]);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn delete_terminal_tasks_before_prunes_terminal_state_only() {
        let path = temp_path("task-gc");
        let state = DuckdbState::open(&path).unwrap();
        let cutoff = Utc::now();
        let old = cutoff - chrono::TimeDelta::days(2);
        let fresh = cutoff + chrono::TimeDelta::days(2);
        let old_task = task("old-task", TaskStatus::Completed, old);
        let working_task = task("working-task", TaskStatus::Working, old);
        let fresh_task = task("fresh-task", TaskStatus::Completed, fresh);

        state
            .record_task(&old_task, None, None, Some("prediction-old"))
            .unwrap();
        state.record_task_owner(&owner("old-task")).unwrap();
        state
            .record_prediction(&prediction("prediction-old"))
            .unwrap();
        state
            .record_task(&working_task, None, None, Some("prediction-working"))
            .unwrap();
        state.record_task_owner(&owner("working-task")).unwrap();
        state
            .record_prediction(&prediction("prediction-working"))
            .unwrap();
        state
            .record_task(&fresh_task, None, None, Some("prediction-fresh"))
            .unwrap();
        state.record_task_owner(&owner("fresh-task")).unwrap();
        state
            .record_prediction(&prediction("prediction-fresh"))
            .unwrap();

        assert_eq!(
            state.delete_terminal_tasks_before(cutoff).unwrap(),
            MediaStateGcSummary {
                tasks_deleted: 1,
                task_owners_deleted: 1,
                predictions_deleted: 1,
            }
        );
        assert!(state.task_owner("old-task").unwrap().is_none());
        assert!(state.prediction("prediction-old").unwrap().is_none());
        assert!(state.task_owner("working-task").unwrap().is_some());
        assert!(state.prediction("prediction-working").unwrap().is_some());
        assert!(state.task_owner("fresh-task").unwrap().is_some());
        assert!(state.prediction("prediction-fresh").unwrap().is_some());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn artifact_owner_round_trips() {
        let path = temp_path("artifact-owner");
        let state = DuckdbState::open(&path).unwrap();
        let owner = ArtifactOwner {
            sha256: "a".repeat(64),
            task_id: "task-1".to_string(),
            principal_id: PrincipalId::new("https://idp.example.com#user-1").unwrap(),
            profile: GatewayProfileId::new("default").unwrap(),
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            data_labels: BTreeSet::from([DataLabelId::new("itar").unwrap()]),
        };

        state.record_artifact_owner(&owner).unwrap();

        assert_eq!(state.artifact_owners(&"a".repeat(64)).unwrap(), vec![owner]);

        let _ = std::fs::remove_file(path);
    }
}
