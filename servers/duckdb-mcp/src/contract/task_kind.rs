//! Code-owned Task operations, independent of the database and MCP runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(task_type)]
pub enum DuckDbTaskKind {
    #[vocabulary(rename = "query")]
    Query,
    #[vocabulary(rename = "execute")]
    Execute,
    #[vocabulary(rename = "ingest")]
    Ingest,
    #[vocabulary(rename = "export")]
    Export,
}
