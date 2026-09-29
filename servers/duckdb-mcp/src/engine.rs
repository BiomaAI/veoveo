//! DuckDB MCP adapter over the shared Veoveo DuckDB sandbox.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use duckdb::Connection;
use serde_json::Value;
use veoveo_duckdb_runtime as runtime;

use crate::contract::DuckDbColumn;

pub use runtime::{
    AttachSpec, EngineSettings, SpatialAxisPolicy, TrustedExtension, quote_sql_literal,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileExchange {
    Denied,
    ExchangeDir(PathBuf),
}

impl From<&FileExchange> for runtime::FileAccess {
    fn from(value: &FileExchange) -> Self {
        match value {
            FileExchange::Denied => Self::Denied,
            FileExchange::ExchangeDir(directory) => Self::RequestDirectory(directory.clone()),
        }
    }
}

pub fn open_connection(
    db_path: &Path,
    read_only: bool,
    attach: &[AttachSpec],
    exchange: &FileExchange,
    settings: &EngineSettings,
) -> Result<Connection> {
    runtime::open_connection(db_path, read_only, attach, &exchange.into(), settings)
}

pub fn verify_spatial(settings: &EngineSettings) -> Result<()> {
    let conn = runtime::open_in_memory(&runtime::FileAccess::Denied, settings)
        .context("opening DuckDB spatial verification connection")?;
    runtime::verify_spatial_axis_policy(&conn, SpatialAxisPolicy::Native)?;
    let point: String = conn
        .query_row("SELECT ST_AsText(ST_Point(1, 2))", [], |row| row.get(0))
        .context("executing DuckDB Spatial verification query")?;
    ensure!(
        point == "POINT (1 2)",
        "DuckDB Spatial verification returned `{point}`"
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryRows {
    pub columns: Vec<DuckDbColumn>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: u64,
    pub truncated: bool,
}

pub fn run_query(conn: &Connection, sql: &str, row_cap: u64, byte_cap: u64) -> Result<QueryRows> {
    let rows = runtime::run_query(
        conn,
        sql,
        runtime::QueryLimits::interactive(row_cap, byte_cap),
    )?;
    Ok(QueryRows {
        columns: rows
            .columns
            .into_iter()
            .map(|column| DuckDbColumn {
                name: column.name,
                type_name: column.type_name,
            })
            .collect(),
        rows: rows.rows,
        row_count: rows.row_count,
        truncated: rows.truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_query_results_satisfy_shape_and_observed_count_admission() {
        for (sql, row_cap, byte_cap, returned, observed, truncated) in [
            ("SELECT 42 AS answer WHERE false", 10, 1024, 0, 0, false),
            (
                "SELECT range AS answer FROM range(3)",
                10,
                1024,
                3,
                3,
                false,
            ),
            ("SELECT range AS answer FROM range(3)", 1, 1024, 1, 2, true),
            ("SELECT 42 AS answer", 10, 1, 0, 1, true),
            ("SELECT NULL AS answer", 10, 1024, 1, 1, false),
        ] {
            let conn = Connection::open_in_memory().unwrap();
            let rows = run_query(&conn, sql, row_cap, byte_cap).unwrap();
            let output = crate::contract::DuckDbQueryOutput::inline(
                rows.columns,
                rows.rows,
                rows.row_count,
                rows.truncated,
            )
            .unwrap();
            assert_eq!(output.rows().len(), returned);
            assert_eq!(output.row_count(), observed);
            assert_eq!(output.truncated(), truncated);
        }
    }

    #[test]
    fn duckdb_mcp_spatial_verification_retains_native_axis_policy() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let mut settings = EngineSettings::new(root.path().join("spill"));
        settings
            .trusted_extensions
            .push(TrustedExtension::new("spatial", PathBuf::from(extension)).unwrap());
        settings.spatial_axis_policy = SpatialAxisPolicy::Native;

        verify_spatial(&settings).unwrap();
    }
}
