//! Shared sandbox for every in-process DuckDB used by Veoveo.
//!
//! Caller SQL remains arbitrary inside its database. Ambient file, network,
//! extension, and configuration authority is removed at the engine boundary;
//! governed inputs are materialized into one request-local directory first.

mod engine;
mod source;

pub use engine::{
    AttachSpec, EngineSettings, FileAccess, QueryColumn, QueryLimits, QueryRows, open_connection,
    open_in_memory, quote_sql_literal, run_query, run_read_only_query, validate_single_statement,
};
pub use source::{
    AuthorizedArtifact, HttpsSourcePolicy, RequestWorkspace, is_public_ip,
    materialize_authorized_artifact, materialize_https_source,
};
