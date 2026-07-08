use std::{collections::BTreeSet, path::Path};

use crate::contract::DuckDbDatabaseId;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use duckdb::{OptionalExt, params};
use rmcp::model::Task;
use serde_json::Value;
use veoveo_mcp_contract::{
    ArtifactMetadata, DataLabelId, DuckDbAnalytics, GatewayProfileId, PrincipalId,
    SharedDuckDbConnection, TenantId, UsageRecord, open_duckdb,
};

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

/// Registry record for one owner-scoped mutable database file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseOwner {
    pub db_id: DuckDbDatabaseId,
    pub principal_id: PrincipalId,
    pub profile: GatewayProfileId,
    pub tenant: Option<TenantId>,
    pub data_labels: BTreeSet<DataLabelId>,
    pub file_path: String,
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
                payload_json TEXT,
                error TEXT,
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

            CREATE TABLE IF NOT EXISTS databases (
                db_id TEXT NOT NULL,
                principal_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                tenant TEXT,
                data_labels_json TEXT NOT NULL,
                file_path TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL,
                PRIMARY KEY (db_id, principal_id)
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
    ) -> Result<()> {
        let task_json = serde_json::to_string(task)?;
        let payload_json = payload.map(serde_json::to_string).transpose()?;
        let updated_at = parse_rfc3339_utc(&task.last_updated_at)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO tasks (task_id, task_json, payload_json, error, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(task_id) DO UPDATE SET
                task_json = excluded.task_json,
                payload_json = COALESCE(excluded.payload_json, tasks.payload_json),
                error = COALESCE(excluded.error, tasks.error),
                updated_at = excluded.updated_at
            "#,
            params![
                task.task_id.as_str(),
                task_json,
                payload_json,
                error,
                updated_at
            ],
        )?;
        Ok(())
    }

    pub fn load_tasks(&self) -> Result<Vec<PersistedTask>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT task_json, payload_json, error FROM tasks ORDER BY updated_at")?;
        let rows = stmt.query_map([], |row| {
            let task_json: String = row.get(0)?;
            let payload_json: Option<String> = row.get(1)?;
            let error: Option<String> = row.get(2)?;
            Ok((task_json, payload_json, error))
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            let (task_json, payload_json, error) = row?;
            tasks.push(PersistedTask {
                task: serde_json::from_str(&task_json)?,
                payload: payload_json.map(|s| serde_json::from_str(&s)).transpose()?,
                error,
            });
        }
        Ok(tasks)
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
                Utc::now()
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
            params![metadata.sha256.as_str(), metadata_json, Utc::now()],
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
                Utc::now()
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

    pub fn record_database(&self, owner: &DatabaseOwner) -> Result<()> {
        let data_labels_json = data_labels_to_json(&owner.data_labels)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO databases (
                db_id, principal_id, profile, tenant, data_labels_json, file_path, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ON CONFLICT(db_id, principal_id) DO UPDATE SET
                profile = excluded.profile,
                tenant = excluded.tenant,
                data_labels_json = excluded.data_labels_json,
                file_path = excluded.file_path,
                updated_at = excluded.updated_at
            "#,
            params![
                owner.db_id.as_str(),
                owner.principal_id.as_str(),
                owner.profile.as_str(),
                owner.tenant.as_ref().map(TenantId::as_str),
                data_labels_json,
                owner.file_path.as_str(),
                Utc::now()
            ],
        )?;
        Ok(())
    }

    pub fn databases(&self) -> Result<Vec<DatabaseOwner>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT db_id, principal_id, profile, tenant, data_labels_json, file_path FROM databases ORDER BY db_id",
        )?;
        let rows = stmt.query_map([], database_row)?;
        collect_databases(rows)
    }

    pub fn database_owners(&self, db_id: &DuckDbDatabaseId) -> Result<Vec<DatabaseOwner>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT db_id, principal_id, profile, tenant, data_labels_json, file_path FROM databases WHERE db_id = ?1",
        )?;
        let rows = stmt.query_map(params![db_id.as_str()], database_row)?;
        collect_databases(rows)
    }

    pub fn record_usage(&self, record: &UsageRecord) -> Result<()> {
        self.analytics.record_usage(record)
    }

    pub fn usage_records(&self, task_id: &str) -> Result<Vec<UsageRecord>> {
        self.analytics.usage_records(task_id)
    }

    pub fn usage_task_ids(&self) -> Result<Vec<String>> {
        self.analytics.usage_task_ids()
    }
}

type DatabaseRow = (String, String, String, Option<String>, String, String);

fn database_row(row: &duckdb::Row<'_>) -> duckdb::Result<DatabaseRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn collect_databases(
    rows: impl Iterator<Item = duckdb::Result<DatabaseRow>>,
) -> Result<Vec<DatabaseOwner>> {
    let mut databases = Vec::new();
    for row in rows {
        let (db_id, principal_id, profile, tenant, data_labels_json, file_path) = row?;
        databases.push(DatabaseOwner {
            db_id: DuckDbDatabaseId::new(db_id)?,
            principal_id: PrincipalId::new(principal_id)?,
            profile: GatewayProfileId::new(profile)?,
            tenant: tenant.map(TenantId::new).transpose()?,
            data_labels: data_labels_from_json(&data_labels_json)?,
            file_path,
        });
    }
    Ok(databases)
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
