use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use duckdb::{Connection, params};

use crate::{UsageKind, UsageRecord};

pub type SharedDuckDbConnection = Arc<Mutex<Connection>>;

pub fn open_duckdb(path: impl AsRef<Path>) -> Result<SharedDuckDbConnection> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating DuckDB directory {}", parent.display()))?;
    }
    let conn =
        Connection::open(path).with_context(|| format!("opening DuckDB {}", path.display()))?;
    Ok(Arc::new(Mutex::new(conn)))
}

#[derive(Clone)]
pub struct DuckDbAnalytics {
    conn: SharedDuckDbConnection,
}

impl DuckDbAnalytics {
    pub fn from_connection(conn: SharedDuckDbConnection) -> Result<Self> {
        let analytics = Self { conn };
        analytics.initialize()?;
        Ok(analytics)
    }

    fn initialize(&self) -> Result<()> {
        let conn = self.conn.lock().expect("duckdb analytics mutex poisoned");
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS usage_records (
                usage_id TEXT PRIMARY KEY,
                source_id TEXT,
                task_id TEXT NOT NULL,
                provider_job_id TEXT,
                model_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                quantity DOUBLE,
                unit TEXT,
                amount DOUBLE,
                currency TEXT,
                recorded_at TIMESTAMP NOT NULL,
                record_json TEXT NOT NULL,
                updated_at TIMESTAMP NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_usage_records_task_id
            ON usage_records(task_id);

            CREATE INDEX IF NOT EXISTS idx_usage_records_provider_job
            ON usage_records(provider_job_id);
            "#,
        )?;
        Ok(())
    }

    pub fn record_usage(&self, record: &UsageRecord) -> Result<()> {
        let record_json = serde_json::to_string(record)?;
        let kind = usage_kind_name(record.kind);
        let usage_id = usage_record_id(record);
        let conn = self.conn.lock().expect("duckdb analytics mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO usage_records (
                usage_id, source_id, task_id, provider_job_id, model_id, kind,
                quantity, unit, amount, currency, recorded_at, record_json, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(usage_id) DO UPDATE SET
                source_id = excluded.source_id,
                provider_job_id = excluded.provider_job_id,
                quantity = excluded.quantity,
                unit = excluded.unit,
                amount = excluded.amount,
                currency = excluded.currency,
                recorded_at = excluded.recorded_at,
                record_json = excluded.record_json,
                updated_at = excluded.updated_at
            "#,
            params![
                usage_id,
                record.source_id.as_deref(),
                record.task_id.as_str(),
                record.provider_job_id.as_deref(),
                record.model_id.as_str(),
                kind,
                record.quantity,
                record.unit.as_deref(),
                record.amount,
                record.currency.as_deref(),
                record.recorded_at,
                record_json,
                chrono::Utc::now()
            ],
        )?;
        Ok(())
    }

    pub fn usage_records(&self, task_id: &str) -> Result<Vec<UsageRecord>> {
        let conn = self.conn.lock().expect("duckdb analytics mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT record_json FROM usage_records WHERE task_id = ?1 ORDER BY updated_at",
        )?;
        let rows = stmt.query_map(params![task_id], |row| row.get::<_, String>(0))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(serde_json::from_str(&row?)?);
        }
        Ok(records)
    }

    pub fn usage_task_ids(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().expect("duckdb analytics mutex poisoned");
        let mut stmt =
            conn.prepare("SELECT DISTINCT task_id FROM usage_records ORDER BY task_id")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut task_ids = Vec::new();
        for row in rows {
            task_ids.push(row?);
        }
        Ok(task_ids)
    }

    pub fn has_actual_usage(&self, task_id: &str, provider_job_id: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("duckdb analytics mutex poisoned");
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_records WHERE task_id = ?1 AND provider_job_id = ?2 AND kind = 'actual'",
            params![task_id, provider_job_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

fn usage_kind_name(kind: UsageKind) -> &'static str {
    match kind {
        UsageKind::Estimate => "estimate",
        UsageKind::Actual => "actual",
    }
}

fn usage_record_id(record: &UsageRecord) -> String {
    format!(
        "{}:{}:{}:{}",
        record.task_id,
        usage_kind_name(record.kind),
        record.model_id,
        record
            .source_id
            .as_deref()
            .or(record.provider_job_id.as_deref())
            .unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::Value;

    use super::*;
    use crate::{UsageKind, UsageRecord};

    #[test]
    fn duckdb_usage_records_round_trip() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("veoveo-usage-{unique}.duckdb"));
        let conn = open_duckdb(&path).unwrap();
        let analytics = DuckDbAnalytics::from_connection(conn).unwrap();

        let record = UsageRecord {
            task_id: "task-1".into(),
            source_id: Some("billing-1".into()),
            provider_job_id: Some("job-1".into()),
            model_id: "model-1".into(),
            kind: UsageKind::Actual,
            quantity: Some(1.0),
            unit: Some("billing_record".into()),
            amount: Some(0.06),
            currency: Some("USD".into()),
            recorded_at: chrono::DateTime::parse_from_rfc3339("2026-07-02T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            metadata: Value::Null,
        };

        analytics.record_usage(&record).unwrap();
        assert!(analytics.has_actual_usage("task-1", "job-1").unwrap());
        assert_eq!(analytics.usage_task_ids().unwrap(), vec!["task-1"]);
        assert_eq!(analytics.usage_records("task-1").unwrap(), vec![record]);

        let _ = std::fs::remove_file(path);
    }
}
