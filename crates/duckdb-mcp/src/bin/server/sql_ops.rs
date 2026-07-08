//! The four DuckDB operations behind the tools: query, execute, ingest,
//! export. Each opens a hardened engine connection inside `spawn_blocking`,
//! registers an interrupt handle, and is cancelled on timeout.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context as _, Result, bail};
use rmcp::ErrorData as McpError;
use serde_json::json;
use veoveo_duckdb_mcp::{
    contract::{
        DuckDbDatabaseId, DuckDbExecuteOutput, DuckDbExecuteRequest, DuckDbExportFormat,
        DuckDbExportOutput, DuckDbExportRequest, DuckDbExportSelection, DuckDbIngestMode,
        DuckDbIngestOutput, DuckDbIngestRequest, DuckDbQueryOutput, DuckDbQueryOutputMode,
        DuckDbQueryRequest,
    },
    engine::{self, AttachSpec, FileExchange},
    state::TaskOwner,
};
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactPut, ComplianceMetadata, DuckDbSource, GatewayInternalIdentity,
    PlaneCaller, duckdb_quote_identifier, duckdb_quote_literal, duckdb_read_function_sql,
    duckdb_read_options_sql,
};

use super::{
    app_state::AppState,
    ownership::{database_writable, resolve_readable_database, resolve_writable_database},
};

const MAX_INGEST_FETCH_BYTES: usize = 256 * 1024 * 1024;
const INTERRUPT_UNWIND_TIMEOUT: Duration = Duration::from_secs(10);

/// Lets blocking engine work publish its interrupt handle so a timeout can
/// cancel the running statement instead of abandoning the thread.
pub(super) struct EngineWatch {
    sender: Mutex<Option<tokio::sync::oneshot::Sender<Arc<duckdb::InterruptHandle>>>>,
}

impl EngineWatch {
    pub(super) fn register(&self, conn: &duckdb::Connection) {
        if let Some(sender) = self.sender.lock().expect("engine watch lock").take() {
            let _ = sender.send(conn.interrupt_handle());
        }
    }
}

async fn run_engine_blocking<T: Send + 'static>(
    label: &'static str,
    timeout_ms: u64,
    work: impl FnOnce(&EngineWatch) -> Result<T> + Send + 'static,
) -> Result<T, McpError> {
    let (sender, mut receiver) = tokio::sync::oneshot::channel();
    let watch = Arc::new(EngineWatch {
        sender: Mutex::new(Some(sender)),
    });
    let work_watch = watch.clone();
    let mut join = tokio::task::spawn_blocking(move || work(&work_watch));
    match tokio::time::timeout(Duration::from_millis(timeout_ms), &mut join).await {
        Ok(joined) => joined
            .map_err(|err| McpError::internal_error(format!("{label} worker failed: {err}"), None))?
            .map_err(|err| McpError::invalid_params(format!("{label} failed: {err:#}"), None)),
        Err(_) => {
            if let Ok(handle) = receiver.try_recv() {
                handle.interrupt();
            }
            let _ = tokio::time::timeout(INTERRUPT_UNWIND_TIMEOUT, join).await;
            Err(McpError::invalid_params(
                format!("{label} exceeded {timeout_ms}ms and was interrupted"),
                None,
            ))
        }
    }
}

fn require_data_file(db_id: &DuckDbDatabaseId, file_path: &str) -> Result<PathBuf, McpError> {
    let path = PathBuf::from(file_path);
    if path.exists() {
        Ok(path)
    } else {
        Err(McpError::invalid_params(
            format!("database `{db_id}` has no data yet"),
            None,
        ))
    }
}

fn fresh_exchange_dir(state: &AppState) -> PathBuf {
    state
        .dirs
        .exchange_dir
        .join(uuid::Uuid::new_v4().to_string())
}

async fn cleanup_exchange_dir(dir: &PathBuf) {
    if let Err(err) = tokio::fs::remove_dir_all(dir).await {
        tracing::warn!(dir = %dir.display(), "failed to clean exchange dir: {err}");
    }
}

pub(super) async fn put_op_artifact(
    state: &AppState,
    caller: &PlaneCaller,
    owner: &TaskOwner,
    bytes: Vec<u8>,
    mime_type: &str,
    filename: String,
    metadata: serde_json::Value,
) -> Result<ArtifactMetadata, McpError> {
    let mut put = ArtifactPut::new(bytes);
    put.mime_type = Some(mime_type.to_string());
    put.filename = Some(filename);
    // Carry the caller's data labels as artifact classification; the plane
    // stamps tenant + owner itself from the verified identity, and records the
    // owner Admin grant in the ledger (no local artifact_owner row).
    put.compliance = ComplianceMetadata {
        data_labels: owner.data_labels.clone(),
        ..Default::default()
    };
    put.metadata = metadata;
    state
        .artifacts
        .put(caller, put)
        .await
        .map_err(|err| McpError::internal_error(format!("artifact write failed: {err:#}"), None))
}

fn export_file_details(format: DuckDbExportFormat) -> (&'static str, &'static str, &'static str) {
    match format {
        DuckDbExportFormat::Parquet => (
            "parquet",
            "application/vnd.apache.parquet",
            "FORMAT PARQUET",
        ),
        DuckDbExportFormat::Csv => ("csv", "text/csv", "FORMAT CSV, HEADER"),
        DuckDbExportFormat::DuckDb => ("duckdb", "application/vnd.duckdb", ""),
    }
}

fn single_statement_sql(label: &str, sql: &str) -> Result<String, McpError> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return Err(McpError::invalid_params(
            format!("{label} SQL must not be empty"),
            None,
        ));
    }
    Ok(trimmed.to_string())
}

pub(super) async fn query_op(
    state: &Arc<AppState>,
    caller: &PlaneCaller,
    identity: &GatewayInternalIdentity,
    owner: &TaskOwner,
    request: DuckDbQueryRequest,
) -> Result<DuckDbQueryOutput, McpError> {
    let db = resolve_readable_database(state, identity, &request.db)?;
    let db_path = require_data_file(&request.db, &db.file_path)?;

    let mut attach = Vec::new();
    let mut seen = BTreeSet::from([request.db.as_str().to_string()]);
    for extra in &request.attach {
        if !seen.insert(extra.as_str().to_string()) {
            return Err(McpError::invalid_params(
                format!("duplicate attached database `{extra}`"),
                None,
            ));
        }
        let attached = resolve_readable_database(state, identity, extra)?;
        attach.push(AttachSpec {
            name: extra.as_str().to_string(),
            path: require_data_file(extra, &attached.file_path)?,
        });
    }

    let settings = state.engine.clone();
    let timeout_ms = state.clamp_timeout_ms(request.timeout_ms);
    match request.output.clone() {
        DuckDbQueryOutputMode::Inline => {
            let row_cap = request
                .row_limit
                .unwrap_or(state.caps.max_inline_rows)
                .min(state.caps.max_inline_rows)
                .max(1);
            let byte_cap = state.caps.max_inline_bytes;
            let sql = request.sql.clone();
            let rows = run_engine_blocking("query", timeout_ms, move |watch| {
                let conn = engine::open_connection(
                    &db_path,
                    true,
                    &attach,
                    &FileExchange::Denied,
                    &settings,
                )?;
                watch.register(&conn);
                engine::run_query(&conn, &sql, row_cap, byte_cap)
            })
            .await?;
            Ok(DuckDbQueryOutput {
                columns: rows.columns,
                rows: rows.rows,
                row_count: rows.row_count,
                truncated: rows.truncated,
                artifact: None,
            })
        }
        DuckDbQueryOutputMode::Artifact { format } => {
            if format == DuckDbExportFormat::DuckDb {
                return Err(McpError::invalid_params(
                    "query artifact output supports parquet or csv; use export for database snapshots",
                    None,
                ));
            }
            let select_sql = single_statement_sql("query", &request.sql)?;
            let (extension, mime_type, copy_options) = export_file_details(format);
            let exchange = fresh_exchange_dir(state);
            tokio::fs::create_dir_all(&exchange)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
            let out_path = exchange.join(format!("result.{extension}"));
            let copy_sql = format!(
                "COPY ({select_sql}) TO {} ({copy_options})",
                engine::quote_sql_literal(out_path.to_string_lossy().as_ref())
            );
            let exchange_for_engine = exchange.clone();
            let row_count = run_engine_blocking("query export", timeout_ms, move |watch| {
                let conn = engine::open_connection(
                    &db_path,
                    true,
                    &attach,
                    &FileExchange::ExchangeDir(exchange_for_engine),
                    &settings,
                )?;
                watch.register(&conn);
                let rows = conn
                    .execute(&copy_sql, [])
                    .context("copying query result")?;
                Ok(rows as u64)
            })
            .await
            .inspect_err(|_| {
                let exchange = exchange.clone();
                tokio::spawn(async move { cleanup_exchange_dir(&exchange).await });
            })?;
            let bytes = tokio::fs::read(&out_path)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
            cleanup_exchange_dir(&exchange).await;
            let artifact = put_op_artifact(
                state,
                caller,
                owner,
                bytes,
                mime_type,
                format!("{}_query.{extension}", request.db),
                json!({
                    "op": "query",
                    "db": request.db.as_str(),
                    "row_count": row_count,
                    "task_id": owner.task_id,
                }),
            )
            .await?;
            Ok(DuckDbQueryOutput {
                columns: Vec::new(),
                rows: Vec::new(),
                row_count,
                truncated: false,
                artifact: Some(artifact.without_download_url()),
            })
        }
    }
}

pub(super) async fn execute_op(
    state: &Arc<AppState>,
    identity: &GatewayInternalIdentity,
    request: DuckDbExecuteRequest,
) -> Result<DuckDbExecuteOutput, McpError> {
    let (db, created) =
        resolve_writable_database(state, identity, &request.db, request.create_if_missing)?;
    let db_path = PathBuf::from(&db.file_path);
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    }
    let lock = state.write_lock(&db.file_path).await;
    let _guard = lock.lock().await;
    let settings = state.engine.clone();
    let timeout_ms = state.clamp_timeout_ms(request.timeout_ms);
    let sql = request.sql.clone();
    let (statements, rows_changed) = run_engine_blocking("execute", timeout_ms, move |watch| {
        let conn = engine::open_connection(&db_path, false, &[], &FileExchange::Denied, &settings)?;
        watch.register(&conn);
        execute_sql(&conn, &sql)
    })
    .await?;
    if created {
        state
            .durable
            .record_database(&db)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    }
    Ok(DuckDbExecuteOutput {
        db: request.db,
        statements,
        rows_changed,
        db_created: created,
    })
}

fn execute_sql(conn: &duckdb::Connection, sql: &str) -> Result<(u64, u64)> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        bail!("execute SQL must not be empty");
    }
    if !trimmed.contains(';') {
        let rows_changed = conn.execute(trimmed, []).context("executing statement")? as u64;
        return Ok((1, rows_changed));
    }
    conn.execute_batch(sql).context("executing statements")?;
    let statements = sql
        .split(';')
        .filter(|part| !part.trim().is_empty())
        .count() as u64;
    // Batch execution reports no per-statement change counts.
    Ok((statements, 0))
}

pub(super) async fn ingest_op(
    state: &Arc<AppState>,
    caller: &PlaneCaller,
    identity: &GatewayInternalIdentity,
    request: DuckDbIngestRequest,
) -> Result<DuckDbIngestOutput, McpError> {
    let table = request.table.trim();
    if table.is_empty() {
        return Err(McpError::invalid_params("table must not be empty", None));
    }
    let (db, created) =
        resolve_writable_database(state, identity, &request.db, request.create_db_if_missing)?;
    let db_path = PathBuf::from(&db.file_path);
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    }

    let exchange = fresh_exchange_dir(state);
    tokio::fs::create_dir_all(&exchange)
        .await
        .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    let source_expr = match materialize_source(state, caller, &request.source, &exchange).await {
        Ok(expr) => expr,
        Err(err) => {
            cleanup_exchange_dir(&exchange).await;
            return Err(err);
        }
    };
    let table_ident = duckdb_quote_identifier(table);
    let ingest_sql = match request.mode {
        DuckDbIngestMode::Create => {
            format!("CREATE TABLE {table_ident} AS SELECT * FROM {source_expr}")
        }
        DuckDbIngestMode::Append => {
            format!("INSERT INTO {table_ident} SELECT * FROM {source_expr}")
        }
        DuckDbIngestMode::Replace => {
            format!("CREATE OR REPLACE TABLE {table_ident} AS SELECT * FROM {source_expr}")
        }
    };

    let lock = state.write_lock(&db.file_path).await;
    let _guard = lock.lock().await;
    let settings = state.engine.clone();
    let timeout_ms = state.clamp_timeout_ms(None);
    let exchange_for_engine = exchange.clone();
    let count_ident = table_ident.clone();
    let result = run_engine_blocking("ingest", timeout_ms, move |watch| {
        let conn = engine::open_connection(
            &db_path,
            false,
            &[],
            &FileExchange::ExchangeDir(exchange_for_engine),
            &settings,
        )?;
        watch.register(&conn);
        let changed = conn.execute(&ingest_sql, []).context("ingesting source")?;
        // `CREATE TABLE AS SELECT` reports no change count, so read the table
        // size for create/replace; append already reports its inserted rows.
        let rows = if matches!(request.mode, DuckDbIngestMode::Append) {
            changed as u64
        } else {
            conn.query_row(&format!("SELECT count(*) FROM {count_ident}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .context("counting ingested rows")? as u64
        };
        Ok(rows)
    })
    .await;
    cleanup_exchange_dir(&exchange).await;
    let rows_ingested = result?;
    if created {
        state
            .durable
            .record_database(&db)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    }
    Ok(DuckDbIngestOutput {
        db: request.db,
        table: table.to_string(),
        rows_ingested,
        db_created: created,
    })
}

async fn materialize_source(
    state: &AppState,
    caller: &PlaneCaller,
    source: &DuckDbSource,
    exchange: &PathBuf,
) -> Result<String, McpError> {
    match source {
        DuckDbSource::Artifact {
            uri,
            format,
            options,
        } => {
            // Resolve the neutral artifact:// URI through the plane under the
            // caller's identity; the plane enforces grant + label checks. Bytes
            // are written to the sandboxed exchange dir, never fetched by SQL.
            let object =
                state.artifacts.resolve(caller, uri).await.map_err(|err| {
                    McpError::invalid_params(format!("artifact source: {err}"), None)
                })?;
            let path = exchange.join(format!("artifact-{}", object.metadata.sha256));
            tokio::fs::write(&path, &object.bytes)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
            duckdb_read_function_sql(
                &duckdb_quote_literal(path.to_string_lossy().as_ref()),
                format,
                options,
            )
            .map_err(|err| McpError::invalid_params(err.to_string(), None))
        }
        DuckDbSource::InlineCsv { csv, options, .. } => {
            let path = exchange.join("inline.csv");
            tokio::fs::write(&path, csv)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
            let options = duckdb_read_options_sql(options)
                .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
            Ok(format!(
                "read_csv({}{options})",
                duckdb_quote_literal(path.to_string_lossy().as_ref())
            ))
        }
        DuckDbSource::Uri {
            uri,
            format,
            options,
        } => {
            let path = fetch_ingest_uri(state, uri, exchange, 0).await?;
            duckdb_read_function_sql(
                &duckdb_quote_literal(path.to_string_lossy().as_ref()),
                format,
                options,
            )
            .map_err(|err| McpError::invalid_params(err.to_string(), None))
        }
        DuckDbSource::Uris {
            uris,
            format,
            options,
        } => {
            if uris.is_empty() {
                return Err(McpError::invalid_params(
                    "source.uris must not be empty",
                    None,
                ));
            }
            let mut literals = Vec::with_capacity(uris.len());
            for (index, uri) in uris.iter().enumerate() {
                let path = fetch_ingest_uri(state, uri, exchange, index).await?;
                literals.push(duckdb_quote_literal(path.to_string_lossy().as_ref()));
            }
            duckdb_read_function_sql(&format!("[{}]", literals.join(", ")), format, options)
                .map_err(|err| McpError::invalid_params(err.to_string(), None))
        }
    }
}

/// The server, not the SQL engine, fetches ingest URIs: HTTPS only, host
/// allowlisted by configuration, size-capped.
async fn fetch_ingest_uri(
    state: &AppState,
    uri: &str,
    exchange: &PathBuf,
    index: usize,
) -> Result<PathBuf, McpError> {
    let url = reqwest::Url::parse(uri).map_err(|err| {
        McpError::invalid_params(format!("invalid source uri `{uri}`: {err}"), None)
    })?;
    if url.scheme() != "https" {
        return Err(McpError::invalid_params(
            format!("source uri `{uri}` must use https"),
            None,
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| McpError::invalid_params(format!("source uri `{uri}` has no host"), None))?;
    if !state.ingest_allowlist.iter().any(|allowed| allowed == host) {
        return Err(McpError::invalid_params(
            format!("source host `{host}` is not in the ingest allowlist"),
            None,
        ));
    }
    let response = state
        .http
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|err| McpError::invalid_params(format!("fetching `{uri}` failed: {err}"), None))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|err| McpError::invalid_params(format!("reading `{uri}` failed: {err}"), None))?;
    if bytes.len() > MAX_INGEST_FETCH_BYTES {
        return Err(McpError::invalid_params(
            format!("source `{uri}` exceeds the {MAX_INGEST_FETCH_BYTES} byte ingest cap"),
            None,
        ));
    }
    let path = exchange.join(format!("source-{index}"));
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    Ok(path)
}

pub(super) async fn export_op(
    state: &Arc<AppState>,
    caller: &PlaneCaller,
    identity: &GatewayInternalIdentity,
    owner: &TaskOwner,
    request: DuckDbExportRequest,
) -> Result<DuckDbExportOutput, McpError> {
    let (extension, mime_type, copy_options) = export_file_details(request.format);
    match &request.selection {
        DuckDbExportSelection::Database => {
            if request.format != DuckDbExportFormat::DuckDb {
                return Err(McpError::invalid_params(
                    "database snapshots require format `duck_db`",
                    None,
                ));
            }
            let db = resolve_readable_database(state, identity, &request.db)?;
            if !database_writable(&db, identity) {
                return Err(McpError::invalid_request(
                    "database snapshots require ownership",
                    None,
                ));
            }
            let db_path = require_data_file(&request.db, &db.file_path)?;
            let lock = state.write_lock(&db.file_path).await;
            let _guard = lock.lock().await;
            let settings = state.engine.clone();
            let timeout_ms = state.clamp_timeout_ms(None);
            let checkpoint_path = db_path.clone();
            run_engine_blocking("snapshot checkpoint", timeout_ms, move |watch| {
                let conn = engine::open_connection(
                    &checkpoint_path,
                    false,
                    &[],
                    &FileExchange::Denied,
                    &settings,
                )?;
                watch.register(&conn);
                conn.execute_batch("CHECKPOINT;")
                    .context("checkpointing database")?;
                Ok(())
            })
            .await?;
            let bytes = tokio::fs::read(&db_path)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
            let artifact = put_op_artifact(
                state,
                caller,
                owner,
                bytes,
                mime_type,
                format!("{}_snapshot.{extension}", request.db),
                json!({
                    "op": "export",
                    "db": request.db.as_str(),
                    "selection": "database",
                    "task_id": owner.task_id,
                }),
            )
            .await?;
            Ok(DuckDbExportOutput {
                db: request.db,
                rows_exported: 0,
                artifact: artifact.without_download_url(),
            })
        }
        selection => {
            if request.format == DuckDbExportFormat::DuckDb {
                return Err(McpError::invalid_params(
                    "format `duck_db` is only valid for database snapshots",
                    None,
                ));
            }
            let select_sql = match selection {
                DuckDbExportSelection::Table { table } => {
                    let table = table.trim();
                    if table.is_empty() {
                        return Err(McpError::invalid_params("table must not be empty", None));
                    }
                    format!("SELECT * FROM {}", duckdb_quote_identifier(table))
                }
                DuckDbExportSelection::Sql { sql } => single_statement_sql("export", sql)?,
                DuckDbExportSelection::Database => unreachable!("handled above"),
            };
            let db = resolve_readable_database(state, identity, &request.db)?;
            let db_path = require_data_file(&request.db, &db.file_path)?;
            let exchange = fresh_exchange_dir(state);
            tokio::fs::create_dir_all(&exchange)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
            let out_path = exchange.join(format!("export.{extension}"));
            let copy_sql = format!(
                "COPY ({select_sql}) TO {} ({copy_options})",
                engine::quote_sql_literal(out_path.to_string_lossy().as_ref())
            );
            let settings = state.engine.clone();
            let timeout_ms = state.clamp_timeout_ms(None);
            let exchange_for_engine = exchange.clone();
            let result = run_engine_blocking("export", timeout_ms, move |watch| {
                let conn = engine::open_connection(
                    &db_path,
                    true,
                    &[],
                    &FileExchange::ExchangeDir(exchange_for_engine),
                    &settings,
                )?;
                watch.register(&conn);
                let rows = conn.execute(&copy_sql, []).context("copying export")?;
                Ok(rows as u64)
            })
            .await;
            let rows_exported = match result {
                Ok(rows) => rows,
                Err(err) => {
                    cleanup_exchange_dir(&exchange).await;
                    return Err(err);
                }
            };
            let bytes = tokio::fs::read(&out_path)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
            cleanup_exchange_dir(&exchange).await;
            let selection_label = match selection {
                DuckDbExportSelection::Table { table } => json!({"table": table}),
                DuckDbExportSelection::Sql { .. } => json!("sql"),
                DuckDbExportSelection::Database => unreachable!("handled above"),
            };
            let artifact = put_op_artifact(
                state,
                caller,
                owner,
                bytes,
                mime_type,
                format!("{}_export.{extension}", request.db),
                json!({
                    "op": "export",
                    "db": request.db.as_str(),
                    "selection": selection_label,
                    "row_count": rows_exported,
                    "task_id": owner.task_id,
                }),
            )
            .await?;
            Ok(DuckDbExportOutput {
                db: request.db,
                rows_exported,
                artifact: artifact.without_download_url(),
            })
        }
    }
}
