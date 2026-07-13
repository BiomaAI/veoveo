//! DuckDB MCP adapter over the shared Veoveo DuckDB sandbox.

use std::path::{Path, PathBuf};

use anyhow::Result;
use duckdb::Connection;
use serde_json::Value;
use veoveo_duckdb_runtime as runtime;

use crate::contract::DuckDbColumn;

pub use runtime::{AttachSpec, EngineSettings, quote_sql_literal};

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
