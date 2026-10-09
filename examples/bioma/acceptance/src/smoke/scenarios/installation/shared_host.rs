//! Installed CPU Task delivery and one selected server-process drain.
use super::*;
use serde::Serialize;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    num::NonZeroU64,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};
use veoveo_duckdb_mcp::{
    DuckDbDatabaseSchema, DuckDbDatabaseUri, DuckDbQueryOutput, DuckDbQueryOutputMode,
    DuckDbQueryRequest,
};
use veoveo_testing_support::installed::restart::{DrainReceipt, SelectedDrainIdentity};
use veoveo_types::CanonicalTaskId;

#[path = "shared_host/drain.rs"]
mod drain;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Stage {
    Admitted,
    ResourceVerified,
    DispatchUnresolved,
    TaskObserved,
    CompletedDelivered,
    RestartUnresolved,
    RestartObserved,
    Passed,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Connections {
    NotOpened,
    Open,
    Closed,
    Unresolved,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt<'a> {
    schema: &'static str,
    database: &'a DuckDbDatabaseId,
    stage: Stage,
    task_id: Option<CanonicalTaskId>,
    selected_process: Option<SelectedDrainIdentity>,
    drain: Option<DrainReceipt>,
    completed_state_delivered: bool,
    completed_payload_retained: bool,
    subscription_cleanup: Option<bool>,
    connections: Connections,
    failed: bool,
}

pub(crate) async fn run(
    installation: &InstalledTarget,
    database: &DuckDbDatabaseId,
    input_path: &Path,
    output: &Path,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    let input: drain::DrainInput = private_input(input_path)?;
    ensure!(
        output.is_absolute(),
        "installed Host receipt requires an absolute path"
    );
    let mut evidence = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output)
        .context("installed Host receipt requires a new private file")?;
    let mut receipt = Receipt {
        schema: "veoveo.ai/installed-cpu-host/v1",
        database,
        stage: Stage::Admitted,
        task_id: None,
        selected_process: None,
        drain: None,
        completed_state_delivered: false,
        completed_payload_retained: false,
        subscription_cleanup: None,
        connections: Connections::NotOpened,
        failed: false,
    };
    persist(&mut evidence, &receipt)?;
    let mut connection = None;
    let result = async {
        let (selected, arguments) = tokio::time::timeout_at(deadline, async {
            let token = tokio::time::timeout(Duration::from_secs(15), installation.token())
                .await
                .context("installed CPU Host OAuth admission deadline")?
                .map_err(|_| anyhow!("installed CPU Host OAuth admission failed"))?;
            let connected = tokio::time::timeout(
                Duration::from_secs(15),
                connect_mcp_client(installation.operator.resource.as_str(), &token),
            )
            .await
            .context("installed CPU Host MCP admission deadline")?
            .map_err(|_| anyhow!("installed CPU Host MCP admission failed"))?;
            connection = Some(connected);
            let client = connection.as_ref().expect("connected client");
            receipt.connections = Connections::Open;
            let selected = drain::select(installation, client, &input).await?;
            receipt.selected_process = Some(selected.identity());
            let uri = DuckDbDatabaseUri::new(database.clone());
            let schema: DuckDbDatabaseSchema =
                serde_json::from_value(read_mcp_resource_json(client, &uri.to_string()).await?)
                    .context("owned DuckDB resource did not return its database schema")?;
            ensure!(
                &schema.db_id == database,
                "owned DuckDB resource identity changed"
            );
            receipt.stage = Stage::ResourceVerified;
            persist(&mut evidence, &receipt)?;
            let arguments = serde_json::to_value(query_request(database)?)?;
            Ok::<_, anyhow::Error>((selected, arguments))
        })
        .await
        .context("installed CPU Host preflight exceeded 180 seconds")??;
        let client = connection.as_ref().expect("connected client");
        receipt.stage = Stage::DispatchUnresolved;
        persist(&mut evidence, &receipt)?;
        let mut listener_closed = false;
        let completed = complete_tool_with_notification(
            client,
            "duckdb__query",
            arguments,
            deadline.min(tokio::time::Instant::now() + Duration::from_secs(30)),
            Duration::from_secs(30),
            |task_id| {
                receipt.task_id = Some(task_id.clone());
                receipt.stage = Stage::TaskObserved;
                persist(&mut evidence, &receipt)
            },
            &mut listener_closed,
        )
        .await;
        receipt.subscription_cleanup = Some(listener_closed);
        let (task_id, payload) = completed?;
        let original = require_query_payload(payload)?;
        tokio::time::timeout_at(deadline, async {
            receipt.stage = Stage::CompletedDelivered;
            receipt.completed_state_delivered = true;
            persist(&mut evidence, &receipt)?;
            receipt.stage = Stage::RestartUnresolved;
            persist(&mut evidence, &receipt)?;
            receipt.drain = Some(selected.restart().await?);
            receipt.stage = Stage::RestartObserved;
            persist(&mut evidence, &receipt)?;
            let current = client
                .get_task(GetTaskParams::new(task_id.to_string()))
                .await?;
            ensure!(
                current.task.task.task_id == task_id.as_str()
                    && current.task.status() == TaskStatus::Completed,
                "restarted server changed the exact completed Task"
            );
            require_retained_query_payload(
                &original,
                task_payload(client, task_id.as_str()).await?,
            )?;
            receipt.completed_payload_retained = true;
            receipt.stage = Stage::Passed;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("installed CPU Host restart/read exceeded 180 seconds")?
    }
    .await;
    if let Some(client) = connection {
        receipt.connections = Connections::Unresolved;
        if matches!(
            tokio::time::timeout(Duration::from_secs(10), client.cancel()).await,
            Ok(Ok(()))
        ) {
            receipt.connections = Connections::Closed;
        }
    }
    receipt.failed = result.is_err() || !matches!(receipt.connections, Connections::Closed);
    let written = persist(&mut evidence, &receipt);
    result?;
    written?;
    ensure!(
        matches!(receipt.connections, Connections::Closed),
        "installed CPU Host client cleanup unresolved"
    );
    Ok(())
}

fn query_request(database: &DuckDbDatabaseId) -> Result<DuckDbQueryRequest> {
    Ok(
        DuckDbQueryRequest::builder(database.clone(), "SELECT 1 AS answer".parse()?)
            .row_limit(NonZeroU64::new(1).expect("positive limit"))
            .timeout_ms(NonZeroU64::new(1000).expect("positive timeout"))
            .output(DuckDbQueryOutputMode::Inline {})
            .build()?,
    )
}
fn require_query_payload(payload: rmcp::model::CallToolResult) -> Result<DuckDbQueryOutput> {
    ensure!(
        payload.is_error != Some(true),
        "query Task returned a tool error"
    );
    let result: DuckDbQueryOutput = serde_json::from_value(
        payload
            .structured_content
            .context("query Task omitted typed output")?,
    )?;
    ensure!(
        result.artifact().is_none()
            && !result.truncated()
            && result.row_count() == 1
            && result.columns().len() == 1
            && result.columns()[0].name == "answer"
            && result.rows() == [vec![serde_json::json!(1)]],
        "query Task must return exactly one inline answer equal to 1"
    );
    Ok(result)
}
fn require_retained_query_payload(
    original: &DuckDbQueryOutput,
    payload: rmcp::model::CallToolResult,
) -> Result<()> {
    let current = require_query_payload(payload)?;
    ensure!(
        &current == original,
        "restarted server changed the stored completed query payload"
    );
    Ok(())
}
fn private_input<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    ensure!(
        path.is_absolute(),
        "installed Host fixture input requires an absolute path"
    );
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.mode() & 0o077 == 0 && metadata.len() <= 65536,
        "installed Host fixture input must be a private regular file within 64 KiB"
    );
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 65536,
        "installed Host input exceeds 64 KiB"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn persist(file: &mut File, receipt: &Receipt<'_>) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(receipt)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.set_len(bytes.len() as u64)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_duckdb_mcp::DuckDbColumn;

    fn payload(rows: Vec<Vec<Value>>, count: u64, truncated: bool) -> rmcp::model::CallToolResult {
        let result = DuckDbQueryOutput::inline(
            vec![DuckDbColumn {
                name: "answer".into(),
                type_name: "INTEGER".into(),
            }],
            rows,
            count,
            truncated,
        )
        .unwrap();
        let mut payload = rmcp::model::CallToolResult::success(vec![]);
        payload.structured_content = Some(serde_json::to_value(result).unwrap());
        payload
    }

    #[test]
    fn completed_query_requires_one_exact_untruncated_inline_answer() {
        assert!(require_query_payload(payload(vec![vec![serde_json::json!(1)]], 1, false)).is_ok());
        assert!(
            require_query_payload(payload(vec![vec![serde_json::json!(2)]], 1, false)).is_err()
        );
        assert!(require_query_payload(payload(vec![], 0, false)).is_err());
        assert!(require_query_payload(payload(vec![vec![serde_json::json!(1)]], 2, true)).is_err());
        let mut rejected = payload(vec![vec![serde_json::json!(1)]], 1, false);
        rejected.is_error = Some(true);
        assert!(require_query_payload(rejected).is_err());
    }

    #[test]
    fn retained_payload_rejects_changed_column_type_even_when_answer_is_unchanged() {
        let original =
            require_query_payload(payload(vec![vec![serde_json::json!(1)]], 1, false)).unwrap();
        assert!(
            require_retained_query_payload(
                &original,
                payload(vec![vec![serde_json::json!(1)]], 1, false)
            )
            .is_ok()
        );
        let changed = DuckDbQueryOutput::inline(
            vec![DuckDbColumn {
                name: "answer".into(),
                type_name: "BIGINT".into(),
            }],
            vec![vec![serde_json::json!(1)]],
            1,
            false,
        )
        .unwrap();
        let mut wire = rmcp::model::CallToolResult::success(vec![]);
        wire.structured_content = Some(serde_json::to_value(changed).unwrap());
        assert!(require_query_payload(wire.clone()).is_ok());
        assert!(require_retained_query_payload(&original, wire).is_err());
    }

    #[test]
    fn private_fixture_refuses_permissive_permissions_and_oversized_input() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.json");
        std::fs::write(
            &path,
            br#"{"deployment":"duckdb-mcp","pod":"owned-pod","container":"duckdb-mcp"}"#,
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(private_input::<drain::DrainInput>(&path).is_ok());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(private_input::<drain::DrainInput>(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&path, vec![b' '; 65537]).unwrap();
        assert!(private_input::<drain::DrainInput>(&path).is_err());
    }
}
