use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use duckdb::{
    AccessMode, Config, Connection,
    types::{TimeUnit, ValueRef},
};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct EngineSettings {
    pub memory_limit: String,
    pub threads: u32,
    /// DuckDB can always use its temp directory, so it must not contain
    /// request inputs or other sensitive files.
    pub spill_dir: PathBuf,
    /// Signed extensions selected and loaded by the embedding service before
    /// external access and configuration changes are disabled.
    pub trusted_extensions: Vec<TrustedExtension>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedExtension {
    name: String,
    path: PathBuf,
}

impl TrustedExtension {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Result<Self> {
        let name = name.into();
        let mut chars = name.chars();
        if !chars.next().is_some_and(|first| first.is_ascii_lowercase())
            || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        {
            bail!("invalid trusted DuckDB extension name `{name}`");
        }
        let path = path.into();
        if !path.is_absolute() {
            bail!("trusted DuckDB extension `{name}` requires an absolute path");
        }
        Ok(Self { name, path })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl EngineSettings {
    pub fn new(spill_dir: impl Into<PathBuf>) -> Self {
        Self {
            memory_limit: "512MB".to_string(),
            threads: 2,
            spill_dir: spill_dir.into(),
            trusted_extensions: Vec::new(),
        }
    }

    fn validate(&self) -> Result<()> {
        if self.threads == 0 || self.threads > 64 {
            bail!("DuckDB threads must be in 1..=64");
        }
        let limit = self.memory_limit.trim();
        if limit.is_empty()
            || !limit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte.is_ascii_alphabetic() || byte == b'.')
        {
            bail!("invalid DuckDB memory limit");
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AttachSpec {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAccess {
    Denied,
    /// One directory created for this request. SQL may read and write only
    /// within it; all network access remains disabled.
    RequestDirectory(PathBuf),
}

pub fn open_connection(
    db_path: &Path,
    read_only: bool,
    attach: &[AttachSpec],
    files: &FileAccess,
    settings: &EngineSettings,
) -> Result<Connection> {
    settings.validate()?;
    prepare_directories(files, settings)?;
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
    configure(&conn, attach, files, settings)?;
    Ok(conn)
}

pub fn open_in_memory(files: &FileAccess, settings: &EngineSettings) -> Result<Connection> {
    settings.validate()?;
    prepare_directories(files, settings)?;
    let conn = Connection::open_in_memory().context("opening in-memory DuckDB")?;
    configure(&conn, &[], files, settings)?;
    Ok(conn)
}

fn prepare_directories(files: &FileAccess, settings: &EngineSettings) -> Result<()> {
    std::fs::create_dir_all(&settings.spill_dir)
        .with_context(|| format!("creating spill directory {}", settings.spill_dir.display()))?;
    if let FileAccess::RequestDirectory(directory) = files {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("creating request directory {}", directory.display()))?;
        let spill = settings.spill_dir.canonicalize()?;
        let request = directory.canonicalize()?;
        if spill == request || spill.starts_with(&request) || request.starts_with(&spill) {
            bail!("DuckDB spill and request directories must be disjoint");
        }
    }
    Ok(())
}

fn configure(
    conn: &Connection,
    attach: &[AttachSpec],
    files: &FileAccess,
    settings: &EngineSettings,
) -> Result<()> {
    // The service performs attachments before configuration is locked. SQL
    // cannot add another attachment after external access is disabled.
    for spec in attach {
        if !valid_catalog_name(&spec.name) {
            bail!("invalid attached catalog name `{}`", spec.name);
        }
        let path = quote_sql_literal(spec.path.to_string_lossy().as_ref());
        conn.execute_batch(&format!("ATTACH {path} AS {} (READ_ONLY);", spec.name))
            .with_context(|| format!("attaching database `{}`", spec.name))?;
    }

    let spill_dir = quote_sql_literal(settings.spill_dir.to_string_lossy().as_ref());
    let memory_limit = quote_sql_literal(settings.memory_limit.trim());
    conn.execute_batch(&format!(
        "SET memory_limit = {memory_limit};\n\
         SET threads = {};\n\
         SET temp_directory = {spill_dir};",
        settings.threads
    ))
    .context("applying DuckDB resource limits")?;
    conn.execute_batch(
        "SET allow_community_extensions = false;\n\
         SET autoinstall_known_extensions = false;\n\
         SET autoload_known_extensions = false;",
    )
    .context("restricting DuckDB extension loading")?;
    for extension in &settings.trusted_extensions {
        load_trusted_extension(conn, extension)?;
    }
    if let FileAccess::RequestDirectory(directory) = files {
        let directory = quote_sql_literal(directory.to_string_lossy().as_ref());
        conn.execute_batch(&format!("SET allowed_directories = [{directory}];"))
            .context("allowing request-local DuckDB directory")?;
    }
    conn.execute_batch(
        "SET enable_external_access = false;\n\
         SET lock_configuration = true;",
    )
    .context("locking down DuckDB configuration")?;
    Ok(())
}

fn load_trusted_extension(conn: &Connection, extension: &TrustedExtension) -> Result<()> {
    let path = extension.path().canonicalize().with_context(|| {
        format!(
            "locating trusted DuckDB extension `{}` at {}",
            extension.name(),
            extension.path().display()
        )
    })?;
    if !path.is_file() {
        bail!(
            "trusted DuckDB extension `{}` is not a file: {}",
            extension.name(),
            path.display()
        );
    }
    let path = path.to_str().with_context(|| {
        format!(
            "trusted DuckDB extension `{}` path is not UTF-8",
            extension.name()
        )
    })?;
    conn.execute_batch(&format!("LOAD {};", quote_sql_literal(path)))
        .with_context(|| format!("loading trusted DuckDB extension `{}`", extension.name()))?;
    Ok(())
}

fn valid_catalog_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[derive(Debug, Clone, Copy)]
pub struct QueryLimits {
    pub max_rows: u64,
    pub max_bytes: u64,
    /// DuckDB MCP reports the exact row count. Interactive agent queries stop
    /// immediately at the cap so an unbounded result cannot burn CPU.
    pub count_remaining_rows: bool,
}

impl QueryLimits {
    pub const fn interactive(max_rows: u64, max_bytes: u64) -> Self {
        Self {
            max_rows,
            max_bytes,
            count_remaining_rows: false,
        }
    }

    pub const fn exact_count(max_rows: u64, max_bytes: u64) -> Self {
        Self {
            max_rows,
            max_bytes,
            count_remaining_rows: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryColumn {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryRows {
    pub columns: Vec<QueryColumn>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: u64,
    pub truncated: bool,
    pub materialized_bytes: u64,
}

pub fn run_query(conn: &Connection, sql: &str, limits: QueryLimits) -> Result<QueryRows> {
    validate_single_statement(sql)?;
    if limits.max_rows == 0 || limits.max_bytes == 0 {
        bail!("DuckDB query row and byte caps must be greater than zero");
    }
    let mut stmt = conn.prepare(sql.trim()).context("preparing query")?;
    let mut raw = stmt.query([]).context("executing query")?;
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    let mut row_count = 0_u64;
    let mut materialized_bytes = 0_u64;
    let mut truncated = false;

    while let Some(row) = raw.next().context("reading query row")? {
        if columns.is_empty() {
            let statement = row.as_ref();
            for index in 0..statement.column_count() {
                columns.push(QueryColumn {
                    name: statement.column_name(index).map(ToOwned::to_owned)?,
                    type_name: "NULL".to_string(),
                });
            }
        }
        row_count += 1;
        if truncated {
            if !limits.count_remaining_rows {
                break;
            }
            continue;
        }
        if rows.len() as u64 >= limits.max_rows {
            truncated = true;
            if !limits.count_remaining_rows {
                break;
            }
            continue;
        }

        let mut values = Vec::with_capacity(columns.len());
        let mut row_bytes = 2_u64;
        for (index, column) in columns.iter_mut().enumerate() {
            let value_ref = row.get_ref(index).context("reading column value")?;
            let estimated = estimate_value_ref_bytes(value_ref);
            if materialized_bytes
                .saturating_add(row_bytes)
                .saturating_add(estimated)
                > limits.max_bytes
            {
                truncated = true;
                break;
            }
            let (value, type_name) = value_ref_to_json(value_ref)?;
            if column.type_name == "NULL" && type_name != "NULL" {
                column.type_name = type_name;
            }
            row_bytes = row_bytes.saturating_add(estimate_json_bytes(&value));
            values.push(value);
        }
        if truncated {
            if !limits.count_remaining_rows {
                break;
            }
            continue;
        }
        materialized_bytes = materialized_bytes.saturating_add(row_bytes);
        rows.push(values);
    }
    if truncated && row_count == rows.len() as u64 {
        truncated = false;
    }
    Ok(QueryRows {
        columns,
        rows,
        row_count,
        truncated,
        materialized_bytes,
    })
}

/// Execute one arbitrary statement under DuckDB's native read-only
/// transaction enforcement. This is stronger and more expressive than a
/// keyword blacklist: CTEs, windows, macros, comments, and string contents do
/// not need special cases, while mutations fail inside DuckDB.
pub fn run_read_only_query(conn: &Connection, sql: &str, limits: QueryLimits) -> Result<QueryRows> {
    validate_single_statement(sql)?;
    conn.execute_batch("BEGIN TRANSACTION READ ONLY")
        .context("starting read-only DuckDB transaction")?;
    match run_query(conn, sql, limits) {
        Ok(rows) => {
            conn.execute_batch("COMMIT")
                .context("committing read-only DuckDB transaction")?;
            Ok(rows)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

/// Validate that DuckDB receives exactly one statement. Semicolons in quoted
/// values and comments are ignored; a single trailing terminator is accepted.
pub fn validate_single_statement(sql: &str) -> Result<()> {
    #[derive(Clone, Copy)]
    enum State {
        Normal,
        Single,
        Double,
        LineComment,
        BlockComment(u32),
    }

    let bytes = sql.as_bytes();
    let mut state = State::Normal;
    let mut index = 0;
    let mut saw_content = false;
    let mut terminated = false;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        match state {
            State::Normal => match (byte, next) {
                (b'\'', _) => {
                    if terminated {
                        bail!("expected one DuckDB statement");
                    }
                    saw_content = true;
                    state = State::Single;
                }
                (b'"', _) => {
                    if terminated {
                        bail!("expected one DuckDB statement");
                    }
                    saw_content = true;
                    state = State::Double;
                }
                (b'-', Some(b'-')) => {
                    state = State::LineComment;
                    index += 1;
                }
                (b'/', Some(b'*')) => {
                    state = State::BlockComment(1);
                    index += 1;
                }
                (b';', _) => {
                    if terminated || !saw_content {
                        bail!("expected one DuckDB statement");
                    }
                    terminated = true;
                }
                _ if byte.is_ascii_whitespace() => {}
                _ => {
                    if terminated {
                        bail!("expected one DuckDB statement");
                    }
                    saw_content = true;
                }
            },
            State::Single => {
                if byte == b'\'' {
                    if next == Some(b'\'') {
                        index += 1;
                    } else {
                        state = State::Normal;
                    }
                }
            }
            State::Double => {
                if byte == b'"' {
                    if next == Some(b'"') {
                        index += 1;
                    } else {
                        state = State::Normal;
                    }
                }
            }
            State::LineComment => {
                if byte == b'\n' {
                    state = State::Normal;
                }
            }
            State::BlockComment(depth) => match (byte, next) {
                (b'/', Some(b'*')) => {
                    state = State::BlockComment(depth + 1);
                    index += 1;
                }
                (b'*', Some(b'/')) => {
                    state = if depth == 1 {
                        State::Normal
                    } else {
                        State::BlockComment(depth - 1)
                    };
                    index += 1;
                }
                _ => {}
            },
        }
        index += 1;
    }
    if !saw_content {
        bail!("DuckDB statement must not be empty");
    }
    if matches!(
        state,
        State::Single | State::Double | State::BlockComment(_)
    ) {
        bail!("unterminated DuckDB quote or comment");
    }
    Ok(())
}

fn estimate_value_ref_bytes(value: ValueRef<'_>) -> u64 {
    match value {
        ValueRef::Null => 4,
        ValueRef::Boolean(_) => 5,
        ValueRef::Text(bytes) => bytes.len() as u64 + 2,
        ValueRef::Blob(bytes) | ValueRef::Geometry(bytes) => {
            (bytes.len() as u64).saturating_mul(4).saturating_add(2) / 3 + 48
        }
        _ => 64,
    }
}

fn estimate_json_bytes(value: &Value) -> u64 {
    match value {
        Value::Null => 4,
        Value::Bool(_) => 5,
        Value::Number(_) => 24,
        Value::String(text) => text.len() as u64 + 2,
        other => serde_json::to_string(other)
            .map(|text| text.len() as u64)
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
        ValueRef::UHugeInt(v) => (json!(v.to_string()), "UHUGEINT".to_string()),
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
        ValueRef::Geometry(bytes) => (
            json!({
                "base64": base64_encode(bytes),
                "byte_len": bytes.len(),
            }),
            "GEOMETRY".to_string(),
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
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("Unix epoch is valid");
    let date = epoch
        .checked_add_signed(chrono::Duration::days(days as i64))
        .with_context(|| format!("date out of range: {days} days"))?;
    Ok(date.to_string())
}

fn unit_to_micros(unit: TimeUnit, raw: i64) -> Result<i64> {
    match unit {
        TimeUnit::Second => raw.checked_mul(1_000_000),
        TimeUnit::Millisecond => raw.checked_mul(1_000),
        TimeUnit::Microsecond => Some(raw),
        TimeUnit::Nanosecond => Some(raw / 1_000),
    }
    .context("time value out of range")
}

pub fn quote_sql_literal(value: &str) -> String {
    if value.contains('\0') {
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
            trusted_extensions: Vec::new(),
        };
        let request = dir.path().join("request");
        (dir.path().join("test.duckdb"), settings, request)
    }

    #[test]
    fn arbitrary_analytical_sql_still_works() {
        let dir = TempDir::new().unwrap();
        let (path, settings, _) = setup(&dir);
        let conn = open_connection(&path, false, &[], &FileAccess::Denied, &settings).unwrap();
        conn.execute_batch(
            "CREATE TABLE t (grp VARCHAR, value INTEGER);\n\
             INSERT INTO t VALUES ('a', 1), ('a', 3), ('b', 7);",
        )
        .unwrap();
        let rows = run_read_only_query(
            &conn,
            "WITH ranked AS (SELECT grp, value, row_number() OVER (PARTITION BY grp ORDER BY value DESC) AS rank FROM t) SELECT grp, value FROM ranked WHERE rank = 1 ORDER BY grp",
            QueryLimits::interactive(100, 1_000_000),
        )
        .unwrap();
        assert_eq!(
            rows.rows,
            vec![vec![json!("a"), json!(3)], vec![json!("b"), json!(7)]]
        );
    }

    #[test]
    fn read_only_transaction_blocks_mutation() {
        let dir = TempDir::new().unwrap();
        let (path, settings, _) = setup(&dir);
        let conn = open_connection(&path, false, &[], &FileAccess::Denied, &settings).unwrap();
        conn.execute_batch("CREATE TABLE t (value INTEGER)")
            .unwrap();
        assert!(
            run_read_only_query(
                &conn,
                "INSERT INTO t VALUES (1) RETURNING value",
                QueryLimits::interactive(10, 1_000),
            )
            .is_err()
        );
        let count: i64 = conn
            .query_row("SELECT count(*) FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn external_access_and_configuration_changes_are_blocked() {
        let dir = TempDir::new().unwrap();
        let (path, settings, _) = setup(&dir);
        let conn = open_connection(&path, false, &[], &FileAccess::Denied, &settings).unwrap();
        assert!(
            run_query(
                &conn,
                "SELECT * FROM read_text('/proc/self/environ')",
                QueryLimits::interactive(10, 10_000),
            )
            .is_err()
        );
        assert!(
            conn.execute_batch("SET enable_external_access = true")
                .is_err()
        );
        assert!(conn.execute_batch("INSTALL httpfs").is_err());
        assert!(conn.execute_batch("LOAD httpfs").is_err());
        assert!(
            conn.execute_batch("ATTACH '/tmp/foreign.duckdb' AS foreign")
                .is_err()
        );
    }

    #[test]
    fn trusted_extension_requires_an_absolute_path() {
        let error = TrustedExtension::new("spatial", "spatial.duckdb_extension").unwrap_err();
        assert!(error.to_string().contains("requires an absolute path"));
        assert!(TrustedExtension::new("Spatial", "/tmp/spatial.duckdb_extension").is_err());
    }

    #[test]
    fn missing_trusted_extension_fails_connection_setup() {
        let dir = TempDir::new().unwrap();
        let (path, mut settings, _) = setup(&dir);
        settings.trusted_extensions.push(
            TrustedExtension::new("spatial", dir.path().join("missing.duckdb_extension")).unwrap(),
        );
        let error = open_connection(&path, false, &[], &FileAccess::Denied, &settings).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("locating trusted DuckDB extension")
        );
    }

    #[test]
    fn configured_spatial_extension_supports_geometry_rtree_and_mvt() {
        let Some(path) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let dir = TempDir::new().unwrap();
        let (db_path, mut settings, _) = setup(&dir);
        settings
            .trusted_extensions
            .push(TrustedExtension::new("spatial", PathBuf::from(path)).unwrap());
        let conn = open_connection(&db_path, false, &[], &FileAccess::Denied, &settings).unwrap();
        let point: String = conn
            .query_row("SELECT ST_AsText(ST_Point(1, 2))", [], |row| row.get(0))
            .unwrap();
        assert_eq!(point, "POINT (1 2)");
        conn.execute_batch(
            "CREATE TABLE feature (id INTEGER, geom GEOMETRY);\n\
             INSERT INTO feature VALUES (1, ST_Point(1, 2)), (2, ST_Point(20, 20));\n\
             CREATE INDEX feature_geom_rtree ON feature USING RTREE (geom);",
        )
        .unwrap();
        let matches: i64 = conn
            .query_row(
                "SELECT count(*) FROM feature WHERE ST_Intersects(geom, ST_MakeEnvelope(0, 0, 5, 5))",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(matches, 1);
        let tile: Vec<u8> = conn
            .query_row(
                "WITH bounds AS (SELECT ST_TileEnvelope(0, 0, 0) AS geom),
                 tile_rows AS (
                   SELECT id, ST_AsMVTGeom(
                     feature.geom, ST_Extent(bounds.geom), 4096, 256, true
                   ) AS geom
                   FROM feature, bounds
                   WHERE ST_Intersects(feature.geom, bounds.geom)
                 )
                 SELECT ST_AsMVT(
                   {'id': id, 'geom': geom}, 'feature', 4096, 'geom', 'id'
                 ) FROM tile_rows",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!tile.is_empty());
        assert!(conn.execute_batch("INSTALL httpfs").is_err());
    }

    #[test]
    fn request_directory_is_the_only_file_surface() {
        let dir = TempDir::new().unwrap();
        let (path, settings, request) = setup(&dir);
        std::fs::create_dir_all(&request).unwrap();
        let csv = request.join("input.csv");
        std::fs::write(&csv, "value\n1\n2\n").unwrap();
        let conn = open_connection(
            &path,
            false,
            &[],
            &FileAccess::RequestDirectory(request),
            &settings,
        )
        .unwrap();
        let sql = format!(
            "SELECT sum(value) FROM read_csv({})",
            quote_sql_literal(csv.to_string_lossy().as_ref())
        );
        assert_eq!(
            run_query(&conn, &sql, QueryLimits::interactive(10, 1_000))
                .unwrap()
                .rows[0][0],
            json!("3")
        );
        assert!(
            run_query(
                &conn,
                "SELECT * FROM read_csv('/etc/hosts')",
                QueryLimits::interactive(10, 1_000)
            )
            .is_err()
        );
    }

    #[test]
    fn byte_cap_is_checked_before_materializing_a_blob() {
        let dir = TempDir::new().unwrap();
        let (path, settings, _) = setup(&dir);
        let conn = open_connection(&path, false, &[], &FileAccess::Denied, &settings).unwrap();
        let rows = run_query(
            &conn,
            "SELECT repeat('x', 10000000)",
            QueryLimits::interactive(10, 1024),
        )
        .unwrap();
        assert!(rows.rows.is_empty());
        assert!(rows.truncated);
    }

    #[test]
    fn statement_scanner_handles_literals_and_rejects_chains() {
        assert!(validate_single_statement("SELECT ';' AS value; -- done").is_ok());
        assert!(validate_single_statement("WITH t AS (SELECT 1) SELECT * FROM t").is_ok());
        assert!(validate_single_statement("SELECT 1; DELETE FROM t").is_err());
        assert!(validate_single_statement("SELECT 1 /* ; */").is_ok());
    }
}
