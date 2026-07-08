//! Hardened in-process DuckDB execution.
//!
//! Every connection is locked down before any caller SQL runs: no external
//! access from SQL (no file paths, no httpfs), no extension auto-install, and
//! `lock_configuration` so caller SQL cannot undo any of it. Data enters and
//! leaves only through the per-task exchange directory that the server itself
//! controls, and only on connections that explicitly allow it.

use std::path::{Path, PathBuf};

use crate::contract::DuckDbColumn;
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use duckdb::{
    AccessMode, Config, Connection,
    types::{TimeUnit, ValueRef},
};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct EngineSettings {
    /// Per-connection DuckDB memory limit, e.g. `512MB`.
    pub memory_limit: String,
    /// Per-connection DuckDB thread cap.
    pub threads: u32,
    /// Spill directory for DuckDB temporary files. DuckDB implicitly allows
    /// file access to its temp directory, so this must never hold exchange
    /// payloads and must stay separate from any exchange directory.
    pub spill_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AttachSpec {
    /// SQL-visible catalog name; must be a valid `DuckDbDatabaseId`.
    pub name: String,
    pub path: PathBuf,
}

/// Whether caller SQL on this connection may touch one exchange directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileExchange {
    Denied,
    ExchangeDir(PathBuf),
}

pub fn open_connection(
    db_path: &Path,
    read_only: bool,
    attach: &[AttachSpec],
    exchange: &FileExchange,
    settings: &EngineSettings,
) -> Result<Connection> {
    let mode = if read_only {
        AccessMode::ReadOnly
    } else {
        AccessMode::ReadWrite
    };
    let config = Config::default()
        .access_mode(mode)
        .context("configuring DuckDB access mode")?;
    let conn = Connection::open_with_flags(db_path, config)
        .with_context(|| format!("opening database file {}", db_path.display()))?;
    for spec in attach {
        let path = quote_sql_literal(spec.path.to_string_lossy().as_ref());
        conn.execute_batch(&format!(
            "ATTACH {path} AS {name} (READ_ONLY);",
            name = spec.name
        ))
        .with_context(|| format!("attaching database `{}`", spec.name))?;
    }
    harden(&conn, exchange, settings)?;
    Ok(conn)
}

fn harden(conn: &Connection, exchange: &FileExchange, settings: &EngineSettings) -> Result<()> {
    let spill_dir = quote_sql_literal(settings.spill_dir.to_string_lossy().as_ref());
    let memory_limit = quote_sql_literal(&settings.memory_limit);
    conn.execute_batch(&format!(
        "SET memory_limit = {memory_limit};\n\
         SET threads = {threads};\n\
         SET temp_directory = {spill_dir};",
        threads = settings.threads,
    ))
    .context("applying DuckDB resource limits")?;
    if let FileExchange::ExchangeDir(dir) = exchange {
        let exchange_dir = quote_sql_literal(dir.to_string_lossy().as_ref());
        conn.execute_batch(&format!("SET allowed_directories = [{exchange_dir}];"))
            .context("allowing exchange directory")?;
    }
    conn.execute_batch(
        "SET enable_external_access = false;\n\
         SET autoinstall_known_extensions = false;\n\
         SET autoload_known_extensions = false;\n\
         SET lock_configuration = true;",
    )
    .context("locking down DuckDB configuration")?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryRows {
    pub columns: Vec<DuckDbColumn>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: u64,
    pub truncated: bool,
}

/// Run one read statement and collect rows, truncating at `row_cap` rows or
/// roughly `byte_cap` bytes of JSON, whichever comes first.
pub fn run_query(conn: &Connection, sql: &str, row_cap: u64, byte_cap: u64) -> Result<QueryRows> {
    let mut stmt = conn.prepare(sql).context("preparing query")?;
    let mut raw = stmt.query([]).context("executing query")?;

    let mut columns: Vec<DuckDbColumn> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut row_count: u64 = 0;
    let mut truncated = false;
    let mut approx_bytes: u64 = 0;

    while let Some(row) = raw.next().context("reading query row")? {
        if columns.is_empty() {
            let stmt = row.as_ref();
            for index in 0..stmt.column_count() {
                columns.push(DuckDbColumn {
                    name: stmt.column_name(index).map(ToOwned::to_owned)?,
                    type_name: "NULL".to_string(),
                });
            }
        }
        row_count += 1;
        if truncated {
            continue;
        }
        let mut values = Vec::with_capacity(columns.len());
        for (index, column) in columns.iter_mut().enumerate() {
            let value_ref = row.get_ref(index).context("reading column value")?;
            let (value, type_name) = value_ref_to_json(value_ref)?;
            if column.type_name == "NULL" && type_name != "NULL" {
                column.type_name = type_name;
            }
            approx_bytes += estimate_json_bytes(&value);
            values.push(value);
        }
        rows.push(values);
        if rows.len() as u64 >= row_cap || approx_bytes >= byte_cap {
            truncated = true;
        }
    }
    if truncated {
        // `row_count` kept counting past the cap so callers see the true size.
        if row_count == rows.len() as u64 {
            truncated = false;
        }
    }
    Ok(QueryRows {
        columns,
        rows,
        row_count,
        truncated,
    })
}

fn estimate_json_bytes(value: &Value) -> u64 {
    match value {
        Value::Null => 4,
        Value::Bool(_) => 5,
        Value::Number(_) => 12,
        Value::String(text) => text.len() as u64 + 2,
        other => serde_json::to_string(other)
            .map(|s| s.len() as u64)
            .unwrap_or(64),
    }
}

fn value_ref_to_json(value: ValueRef<'_>) -> Result<(Value, String)> {
    let pair = match value {
        ValueRef::Null => (Value::Null, "NULL".to_string()),
        ValueRef::Boolean(v) => (json!(v), "BOOLEAN".to_string()),
        ValueRef::TinyInt(v) => (json!(v), "TINYINT".to_string()),
        ValueRef::SmallInt(v) => (json!(v), "SMALLINT".to_string()),
        ValueRef::Int(v) => (json!(v), "INTEGER".to_string()),
        ValueRef::BigInt(v) => (json!(v), "BIGINT".to_string()),
        ValueRef::HugeInt(v) => (json!(v.to_string()), "HUGEINT".to_string()),
        ValueRef::UTinyInt(v) => (json!(v), "UTINYINT".to_string()),
        ValueRef::USmallInt(v) => (json!(v), "USMALLINT".to_string()),
        ValueRef::UInt(v) => (json!(v), "UINTEGER".to_string()),
        ValueRef::UBigInt(v) => (json!(v), "UBIGINT".to_string()),
        ValueRef::Float(v) => (json!(v), "FLOAT".to_string()),
        ValueRef::Double(v) => (json!(v), "DOUBLE".to_string()),
        ValueRef::Decimal(v) => (json!(v.to_string()), "DECIMAL".to_string()),
        ValueRef::Text(bytes) => (
            json!(String::from_utf8_lossy(bytes).into_owned()),
            "VARCHAR".to_string(),
        ),
        ValueRef::Blob(bytes) => (
            json!({
                "base64": base64_encode(bytes),
                "byte_len": bytes.len(),
            }),
            "BLOB".to_string(),
        ),
        ValueRef::Timestamp(unit, raw) => (
            json!(timestamp_to_rfc3339(unit, raw)?),
            "TIMESTAMP".to_string(),
        ),
        ValueRef::Date32(days) => (json!(date32_to_iso(days)?), "DATE".to_string()),
        ValueRef::Time64(unit, raw) => (json!(time64_to_iso(unit, raw)?), "TIME".to_string()),
        other => {
            let owned = duckdb::types::Value::from(other);
            (json!(format!("{owned:?}")), "OTHER".to_string())
        }
    };
    Ok(pair)
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.encode(bytes)
}

fn timestamp_to_rfc3339(unit: TimeUnit, raw: i64) -> Result<String> {
    let micros = unit_to_micros(unit, raw)?;
    let timestamp = DateTime::<Utc>::from_timestamp_micros(micros)
        .with_context(|| format!("timestamp out of range: {micros}us"))?;
    Ok(timestamp.to_rfc3339())
}

fn time64_to_iso(unit: TimeUnit, raw: i64) -> Result<String> {
    let micros = unit_to_micros(unit, raw)?;
    let seconds = micros.div_euclid(1_000_000);
    let sub_micros = micros.rem_euclid(1_000_000);
    let (hours, minutes, secs) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    Ok(if sub_micros == 0 {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{secs:02}.{sub_micros:06}")
    })
}

fn date32_to_iso(days: i32) -> Result<String> {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("unix epoch date");
    let date = epoch
        .checked_add_signed(chrono::Duration::days(days as i64))
        .with_context(|| format!("date out of range: {days} days"))?;
    Ok(date.to_string())
}

fn unit_to_micros(unit: TimeUnit, raw: i64) -> Result<i64> {
    let micros = match unit {
        TimeUnit::Second => raw.checked_mul(1_000_000),
        TimeUnit::Millisecond => raw.checked_mul(1_000),
        TimeUnit::Microsecond => Some(raw),
        TimeUnit::Nanosecond => Some(raw / 1_000),
    };
    micros.context("time value out of range")
}

pub fn quote_sql_literal(value: &str) -> String {
    if value.contains('\0') {
        // NUL cannot appear in a DuckDB string literal; fail closed with a
        // literal that can never match a real path.
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup(dir: &TempDir) -> (PathBuf, EngineSettings, PathBuf) {
        let settings = EngineSettings {
            memory_limit: "256MB".to_string(),
            threads: 2,
            spill_dir: dir.path().join("spill"),
        };
        let exchange_dir = dir.path().join("exchange");
        std::fs::create_dir_all(&settings.spill_dir).unwrap();
        std::fs::create_dir_all(&exchange_dir).unwrap();
        (dir.path().join("test.duckdb"), settings, exchange_dir)
    }

    #[test]
    fn query_returns_typed_rows() {
        let dir = TempDir::new().unwrap();
        let (path, settings, exchange_dir) = setup(&dir);
        let _ = &exchange_dir;
        let conn = open_connection(&path, false, &[], &FileExchange::Denied, &settings).unwrap();
        conn.execute_batch(
            "CREATE TABLE t (a INTEGER, b VARCHAR); INSERT INTO t VALUES (1, 'x'), (2, NULL);",
        )
        .unwrap();
        let result = run_query(&conn, "SELECT a, b FROM t ORDER BY a", 100, 1_000_000).unwrap();
        assert_eq!(result.row_count, 2);
        assert!(!result.truncated);
        assert_eq!(result.columns[0].name, "a");
        assert_eq!(result.columns[0].type_name, "INTEGER");
        assert_eq!(result.columns[1].type_name, "VARCHAR");
        assert_eq!(result.rows[0], vec![json!(1), json!("x")]);
        assert_eq!(result.rows[1], vec![json!(2), Value::Null]);
    }

    #[test]
    fn query_truncates_at_row_cap() {
        let dir = TempDir::new().unwrap();
        let (path, settings, exchange_dir) = setup(&dir);
        let _ = &exchange_dir;
        let conn = open_connection(&path, false, &[], &FileExchange::Denied, &settings).unwrap();
        let result = run_query(&conn, "SELECT * FROM range(10)", 3, 1_000_000).unwrap();
        assert_eq!(result.row_count, 10);
        assert_eq!(result.rows.len(), 3);
        assert!(result.truncated);
    }

    #[test]
    fn external_access_is_blocked() {
        let dir = TempDir::new().unwrap();
        let (path, settings, exchange_dir) = setup(&dir);
        let _ = &exchange_dir;
        let conn = open_connection(&path, false, &[], &FileExchange::Denied, &settings).unwrap();
        let err = run_query(&conn, "SELECT * FROM read_csv('/etc/hosts')", 10, 1_000_000)
            .unwrap_err()
            .to_string();
        let _ = err;
        assert!(
            conn.execute_batch("COPY (SELECT 1) TO '/tmp/veoveo-escape.csv';")
                .is_err()
        );
    }

    #[test]
    fn configuration_is_locked() {
        let dir = TempDir::new().unwrap();
        let (path, settings, exchange_dir) = setup(&dir);
        let _ = &exchange_dir;
        let conn = open_connection(&path, false, &[], &FileExchange::Denied, &settings).unwrap();
        assert!(
            conn.execute_batch("SET enable_external_access = true;")
                .is_err()
        );
        assert!(conn.execute_batch("SET memory_limit = '100GB';").is_err());
    }

    #[test]
    fn read_only_connection_blocks_writes() {
        let dir = TempDir::new().unwrap();
        let (path, settings, exchange_dir) = setup(&dir);
        let _ = &exchange_dir;
        {
            let conn =
                open_connection(&path, false, &[], &FileExchange::Denied, &settings).unwrap();
            conn.execute_batch("CREATE TABLE t (a INTEGER);").unwrap();
        }
        let conn = open_connection(&path, true, &[], &FileExchange::Denied, &settings).unwrap();
        assert!(conn.execute_batch("INSERT INTO t VALUES (1);").is_err());
        assert!(run_query(&conn, "SELECT count(*) FROM t", 10, 1_000).is_ok());
    }

    #[test]
    fn exchange_dir_is_gated() {
        let dir = TempDir::new().unwrap();
        let (path, settings, exchange_dir) = setup(&dir);
        let _ = &exchange_dir;
        let csv = exchange_dir.join("in.csv");
        std::fs::write(&csv, "a\n1\n2\n").unwrap();
        let csv_sql = format!(
            "SELECT * FROM read_csv({})",
            quote_sql_literal(csv.to_string_lossy().as_ref())
        );

        let denied = open_connection(&path, false, &[], &FileExchange::Denied, &settings).unwrap();
        assert!(run_query(&denied, &csv_sql, 10, 1_000).is_err());

        let allowed = open_connection(
            &path,
            false,
            &[],
            &FileExchange::ExchangeDir(exchange_dir.clone()),
            &settings,
        )
        .unwrap();
        let rows = run_query(&allowed, &csv_sql, 10, 1_000).unwrap();
        assert_eq!(rows.row_count, 2);
    }

    #[test]
    fn attached_database_is_read_only() {
        let dir = TempDir::new().unwrap();
        let (path, settings, exchange_dir) = setup(&dir);
        let _ = &exchange_dir;
        let other_path = dir.path().join("other.duckdb");
        {
            let other =
                open_connection(&other_path, false, &[], &FileExchange::Denied, &settings).unwrap();
            other
                .execute_batch("CREATE TABLE shared (a INTEGER); INSERT INTO shared VALUES (7);")
                .unwrap();
        }
        let attach = [AttachSpec {
            name: "other".to_string(),
            path: other_path,
        }];
        let conn =
            open_connection(&path, false, &attach, &FileExchange::Denied, &settings).unwrap();
        let rows = run_query(&conn, "SELECT a FROM other.shared", 10, 1_000).unwrap();
        assert_eq!(rows.rows[0][0], json!(7));
        assert!(
            conn.execute_batch("INSERT INTO other.shared VALUES (8);")
                .is_err()
        );
    }
}
