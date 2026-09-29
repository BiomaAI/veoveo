//! Authenticated resource dispatch and owner-local schema reads.
use super::{
    app_state::AppState,
    outputs::usage_record,
    ownership::{
        database_page_for_identity, internal_caller, internal_identity, resolve_readable_database,
        runtime_owner,
    },
    setup::SERVER_DOCS,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use veoveo_duckdb_mcp::{
    contract::*,
    engine::{self, FileExchange},
    uris,
    usage::DuckDbUsage,
};
use veoveo_mcp_contract::UsageReport;

fn json_resource(uri: &str, value: &impl Serialize) -> Result<ReadResourceResult, McpError> {
    let text = serde_json::to_string(value)
        .map_err(|_| McpError::internal_error("serializing DuckDB resource failed", None))?;
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type("application/json"),
    ]))
}

pub(super) async fn read(
    state: &Arc<AppState>,
    uri: &str,
    context: &RequestContext<RoleServer>,
) -> Result<ReadResourceResult, McpError> {
    let identity = internal_identity(context)?;
    let resource = DuckDbResource::parse(uri)
        .map_err(|_| McpError::invalid_params("invalid DuckDB resource address", None))?;
    match resource {
        DuckDbResource::Docs => json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>()),
        DuckDbResource::Document(id) => {
            let doc = SERVER_DOCS
                .doc(id.as_str())
                .ok_or_else(|| McpError::resource_not_found("unknown DuckDB document", None))?;
            Ok(ReadResourceResult::new(vec![
                ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
            ]))
        }
        DuckDbResource::Contract => json_resource(uri, &SERVER_DOCS.contract_declaration()),
        DuckDbResource::Workbench => Ok(ReadResourceResult::new(vec![
            veoveo_mcp_apps_extension::app_html_contents(uri, &workbench_html()),
        ])),
        DuckDbResource::Databases(cursor) => json_resource(
            uri,
            &database_page_for_identity(&state.dirs.database_dir, &identity, cursor.as_ref())
                .await?,
        ),
        DuckDbResource::Database(address) => json_resource(
            uri,
            &database_schema_document(state, &identity, address.id()).await?,
        ),
        DuckDbResource::Usage(index) => {
            let page = DuckDbUsage::new(&state.tasks)
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .page(&runtime_owner(&identity), index.cursor())
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;
            json_resource(uri, &page)
        }
        DuckDbResource::TaskUsage(address) => {
            let task_id = address.task_id();
            let records = DuckDbUsage::new(&state.tasks)
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .task(&runtime_owner(&identity), &address)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .into_iter()
                .map(|record| usage_record(task_id, record))
                .collect::<Vec<_>>();
            if records.is_empty() {
                return Err(McpError::resource_not_found("unknown usage Task", None));
            }
            json_resource(
                uri,
                &UsageReport::new(task_id.to_string(), uri).with_records(records),
            )
        }
        DuckDbResource::Artifact(id) => {
            let artifact = state
                .artifacts
                .get(&internal_caller(context)?, &id)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .ok_or_else(|| McpError::resource_not_found("unknown artifact", None))?;
            Ok(ReadResourceResult::new(vec![
                ResourceContents::blob(BASE64_STANDARD.encode(&artifact.bytes), uri)
                    .with_mime_type(
                        artifact
                            .metadata
                            .mime_type
                            .unwrap_or_else(|| "application/octet-stream".into()),
                    ),
            ]))
        }
    }
}

async fn database_schema_document(
    state: &Arc<AppState>,
    identity: &veoveo_mcp_contract::GatewayInternalIdentity,
    db_id: &DuckDbDatabaseId,
) -> Result<DuckDbDatabaseSchema, McpError> {
    let db_path = resolve_readable_database(state, identity, db_id)?;
    let settings = state.engine.clone();
    let columns = tokio::task::spawn_blocking(move || -> anyhow::Result<engine::QueryRows> {
        let conn = engine::open_connection(&db_path, true, &[], &FileExchange::Denied, &settings)?;
        engine::run_query(&conn,
            "SELECT table_name, column_name, data_type FROM information_schema.columns WHERE table_schema = 'main' ORDER BY table_name, ordinal_position",
            100_000, 8 * 1024 * 1024)
    }).await
        .map_err(|_| McpError::internal_error("schema reader worker failed", None))?
        .map_err(|_| McpError::internal_error("reading database schema failed", None))?;
    schema_from_rows(db_id.clone(), columns)
}

fn schema_from_rows(
    db_id: DuckDbDatabaseId,
    columns: engine::QueryRows,
) -> Result<DuckDbDatabaseSchema, McpError> {
    if columns.truncated {
        return Err(McpError::invalid_params(
            "database schema exceeds the resource limit; query information_schema in bounded selections",
            None,
        ));
    }
    let mut tables: Vec<DuckDbTableSchema> = Vec::new();
    for row in columns.rows {
        let [
            Value::String(table),
            Value::String(column),
            Value::String(data_type),
        ] = row.as_slice()
        else {
            return Err(McpError::internal_error(
                "invalid database schema row",
                None,
            ));
        };
        let column = DuckDbSchemaColumn {
            name: column.clone(),
            data_type: data_type.clone(),
        };
        if let Some(current) = tables.last_mut().filter(|current| &current.name == table) {
            current.columns.push(column);
        } else {
            tables.push(DuckDbTableSchema {
                name: table.clone(),
                columns: vec![column],
            });
        }
    }
    Ok(DuckDbDatabaseSchema { db_id, tables })
}

fn workbench_html() -> String {
    veoveo_mcp_apps_extension::workbench_app_html(&veoveo_mcp_apps_extension::WorkbenchApp {
        app_id: "duckdb-workbench",
        title: "Workbench",
        subtitle: "Run owner-scoped analytical SQL and govern data movement",
        empty_message: "No DuckDB databases are visible to this identity.",
        resources: &[
            veoveo_mcp_apps_extension::WorkbenchResource {
                label: "Databases",
                uri: uris::DBS_ROOT_URI,
            },
            veoveo_mcp_apps_extension::WorkbenchResource {
                label: "Usage",
                uri: DuckDbUsageIndexUri::ROOT,
            },
        ],
        tools: &[
            veoveo_mcp_apps_extension::WorkbenchTool {
                label: "Query",
                name: "query",
                arguments_json: r#"{"db":"","sql":"SELECT 1 AS value"}"#,
            },
            veoveo_mcp_apps_extension::WorkbenchTool {
                label: "Execute",
                name: "execute",
                arguments_json: r#"{"db":"","sql":"CREATE TABLE example(value INTEGER)","create_if_missing":true}"#,
            },
            veoveo_mcp_apps_extension::WorkbenchTool {
                label: "Ingest",
                name: "ingest",
                arguments_json: "{}",
            },
            veoveo_mcp_apps_extension::WorkbenchTool {
                label: "Export",
                name: "export",
                arguments_json: "{}",
            },
        ],
        stream_result: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schemas_preserve_sql_column_order_and_fail_on_incomplete_or_malformed_rows() {
        let rows = engine::QueryRows {
            columns: vec![],
            rows: vec![
                vec!["a".into(), "z".into(), "INTEGER".into()],
                vec!["a".into(), "b".into(), "VARCHAR".into()],
            ],
            row_count: 2,
            truncated: false,
        };
        let schema = schema_from_rows("metrics".parse().unwrap(), rows.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(schema).unwrap(),
            serde_json::json!({"db_id":"metrics", "tables":[{"name":"a","columns":[{"name":"z","type":"INTEGER"},{"name":"b","type":"VARCHAR"}]}]})
        );
        let mut bad = rows.clone();
        bad.truncated = true;
        assert!(schema_from_rows("metrics".parse().unwrap(), bad).is_err());
        let mut bad = rows;
        bad.rows[0][0] = Value::Null;
        assert!(schema_from_rows("metrics".parse().unwrap(), bad).is_err());
    }
}
