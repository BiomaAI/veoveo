//! Domain declarations without MCP or engine dependencies.
use crate::contract::{DuckDbDocument, DuckDbResource};
use std::sync::LazyLock;
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_types::{ResourceAddress, ResourceScheme};

pub static SCHEME: LazyLock<ResourceScheme> =
    LazyLock::new(|| ResourceScheme::parse("duckdb").expect("declared DuckDB scheme"));
pub const DBS_ROOT_URI: &str = "duckdb://dbs";
pub const DBS_TEMPLATE: &str = "duckdb://dbs{?cursor}";
pub const WORKBENCH_APP_URI: &str = "ui://duckdb/workbench.html";
pub const DB_TEMPLATE: &str = "duckdb://db/{db_id}";
pub const ARTIFACT_TEMPLATE: &str = "duckdb://artifact/{artifact_id}";
pub const DOCS_URI: &str = "duckdb://docs";
pub const CONTRACT_URI: &str = "duckdb://contract";
pub const DOC_TEMPLATE: &str = "duckdb://docs/{doc_id}";

pub fn doc_uri(doc: DuckDbDocument) -> String {
    DuckDbResource::Document(doc)
        .to_uri()
        .expect("declared document URI")
        .to_string()
}
pub fn artifact_uri(id: ArtifactId) -> ArtifactUri {
    ArtifactUri::presented(&SCHEME, id)
}
