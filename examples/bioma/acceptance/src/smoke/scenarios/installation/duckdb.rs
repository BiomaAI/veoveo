//! Installed public DuckDB consumers; retained fixtures never authorize mutation replay.
use super::*;
use base64::engine::general_purpose::STANDARD;
use rmcp::model::{ReadResourceRequestParams, ReadResourceResult, ResourceContents};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    io::{Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
};
use veoveo_artifact_contract::ArtifactMetadata;
use veoveo_duckdb_mcp::contract::*;
use veoveo_mcp_conformance::client::failure::ObservedFailure;
use veoveo_types::{CanonicalTaskId, ResourceAddress, ResourceUri, Sha256Digest, TaskId};

const PAGE_LIMIT: usize = 32;
const OPERATION_SECONDS: u64 = 600;

#[derive(Clone, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Mutation {
    Create { request: DuckDbExecuteRequest },
    Ingest { request: DuckDbIngestRequest },
    Export { request: DuckDbExportRequest },
}
impl Mutation {
    fn tool(&self) -> &'static str {
        match self {
            Self::Create { .. } => "duckdb__execute",
            Self::Ingest { .. } => "duckdb__ingest",
            Self::Export { .. } => "duckdb__export",
        }
    }
    fn arguments(&self) -> Result<Value> {
        Ok(match self {
            Self::Create { request } => serde_json::to_value(request)?,
            Self::Ingest { request } => serde_json::to_value(request)?,
            Self::Export { request } => serde_json::to_value(request)?,
        })
    }
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", content = "output", rename_all = "snake_case")]
enum Output {
    Created(DuckDbExecuteOutput),
    Ingested(DuckDbIngestOutput),
    Exported(Box<DuckDbExportOutput>),
}
impl Output {
    fn admit(mutation: &Mutation, payload: &rmcp::model::CallToolResult) -> Result<Self> {
        let value = payload
            .structured_content
            .clone()
            .context("DuckDB completed Task omitted structured output")?;
        Ok(match mutation {
            Mutation::Create { .. } => Self::Created(serde_json::from_value(value)?),
            Mutation::Ingest { .. } => Self::Ingested(serde_json::from_value(value)?),
            Mutation::Export { .. } => Self::Exported(Box::new(serde_json::from_value(value)?)),
        })
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Dispatch {
    intent: Mutation,
    dispatched: bool,
    task_id: Option<CanonicalTaskId>,
    completed_delivered: bool,
    output: Option<Output>,
    subscription_closed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Outcome {
    #[vocabulary(rename = "not_dispatched")]
    NotDispatched,
    #[vocabulary(rename = "mutation_unresolved")]
    MutationUnresolved,
    #[vocabulary(rename = "observed_failure")]
    ObservedFailure,
    Passed,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ReadOutcome {
    Pending,
    Received { digest: Sha256Digest },
    Mcp { failure: ObservedFailure },
    Transport,
    RequestFailed,
    Timeout,
    Interrupted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum CatalogMethod {
    #[vocabulary(rename = "tools/list")]
    Tools,
    #[vocabulary(rename = "resources/templates/list")]
    Templates,
}
#[derive(Serialize)]
struct CatalogRead {
    method: CatalogMethod,
    outcome: ReadOutcome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Reader {
    Operator,
    Administrator,
}
#[derive(Serialize)]
struct Read {
    reader: Reader,
    target: ResourceUri,
    outcome: ReadOutcome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum ReceiptSchema {
    #[vocabulary(rename = "veoveo.ai/installed-duckdb/v1")]
    V1,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    schema: ReceiptSchema,
    scope: InstallationScope,
    control_plane_sha256: Option<Sha256Digest>,
    operator_principal: Option<veoveo_types::PrincipalId>,
    operator_profile: Option<veoveo_types::GatewayProfileId>,
    operator_resource: Option<veoveo_gateway_contract::ProtectedResourceId>,
    administrator_principal: Option<veoveo_types::PrincipalId>,
    outcome: Outcome,
    fixture_databases: Vec<DuckDbDatabaseId>,
    dispatches: Vec<Dispatch>,
    reads: Vec<Read>,
    catalogs: Vec<CatalogRead>,
    database_pages: Vec<DuckDbDatabasePage>,
    usage_pages: Vec<DuckDbUsagePage>,
    corpus_usage: Vec<veoveo_mcp_contract::UsageReport>,
    expected_tools: Vec<veoveo_gateway_contract::GatewayToolName>,
    actual_tools: Option<Vec<veoveo_gateway_contract::GatewayToolName>>,
    expected_templates: Vec<veoveo_types::ResourceTemplateUri>,
    actual_templates: Option<Vec<veoveo_types::ResourceTemplateUri>>,
    schema_readback: Option<DuckDbDatabaseSchema>,
    artifact_metadata: Option<ArtifactMetadata>,
    artifact_sha256: Option<Sha256Digest>,
    native_export_task: Option<TaskId>,
    export_usage: Option<veoveo_mcp_contract::UsageReport>,
    foreign_database_denied: bool,
    foreign_usage_denied: bool,
    connections_closed: bool,
    subscription_closed: bool,
}
impl Receipt {
    fn new() -> Result<Self> {
        Ok(Self {
            schema: ReceiptSchema::V1,
            scope: InstallationScope::Duckdb,
            control_plane_sha256: None,
            operator_principal: None,
            operator_profile: None,
            operator_resource: None,
            administrator_principal: None,
            outcome: Outcome::NotDispatched,
            fixture_databases: vec![],
            dispatches: vec![],
            reads: vec![],
            catalogs: vec![],
            database_pages: vec![],
            usage_pages: vec![],
            corpus_usage: vec![],
            expected_tools: ["duckdb__execute", "duckdb__ingest", "duckdb__export"]
                .into_iter()
                .map(veoveo_gateway_contract::GatewayToolName::parse)
                .collect::<std::result::Result<_, _>>()?,
            actual_tools: None,
            expected_templates: [
                veoveo_duckdb_mcp::uris::DB_TEMPLATE,
                veoveo_artifact_mcp::contract::METADATA_TEMPLATE,
                veoveo_artifact_mcp::contract::ARTIFACT_TEMPLATE,
            ]
            .into_iter()
            .map(veoveo_types::ResourceTemplateUri::new)
            .collect::<std::result::Result<_, _>>()?,
            actual_templates: None,
            schema_readback: None,
            artifact_metadata: None,
            artifact_sha256: None,
            native_export_task: None,
            export_usage: None,
            foreign_database_denied: false,
            foreign_usage_denied: false,
            connections_closed: false,
            subscription_closed: true,
        })
    }
    fn settled(&self) -> bool {
        self.dispatches
            .iter()
            .all(|d| !d.dispatched || d.completed_delivered)
    }
    fn finish(&mut self, passed: bool) {
        self.outcome = if !self.settled() {
            Outcome::MutationUnresolved
        } else if passed {
            Outcome::Passed
        } else if self.dispatches.iter().any(|d| d.dispatched) {
            Outcome::ObservedFailure
        } else {
            Outcome::NotDispatched
        };
        for read in &mut self.reads {
            if matches!(read.outcome, ReadOutcome::Pending) {
                read.outcome = ReadOutcome::Interrupted;
            }
        }
        for read in &mut self.catalogs {
            if matches!(read.outcome, ReadOutcome::Pending) {
                read.outcome = ReadOutcome::Interrupted;
            }
        }
    }
}
fn persist(file: &mut std::fs::File, receipt: &Receipt) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(receipt)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "DuckDB receipt exceeds four MiB"
    );
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.set_len(bytes.len() as u64)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) async fn run(
    installation: &InstalledTarget,
    control_plane: &Path,
    path: &Path,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(OPERATION_SECONDS);
    let administrator = installation.administrator()?;
    ensure!(
        administrator.principal != installation.operator.principal
            && administrator.profile != installation.operator.profile,
        "DuckDB isolation requires distinct administrator principal/profile"
    );
    ensure!(path.is_absolute(), "DuckDB receipt requires absolute path");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .context("DuckDB receipt requires new private file")?;
    let mut receipt = Receipt::new()?;
    receipt.operator_principal = Some(installation.operator.principal.clone());
    receipt.operator_profile = Some(installation.operator.profile.clone());
    receipt.operator_resource = Some(installation.operator.resource.clone());
    receipt.administrator_principal = Some(administrator.principal.clone());
    persist(&mut file, &receipt)?;
    receipt.control_plane_sha256 = Some(admit_configuration(installation, control_plane)?);
    persist(&mut file, &receipt)?;
    let mut operator = None;
    let mut foreign = None;
    let mut notification = TaskNotificationState::default();
    let result=tokio::time::timeout_at(deadline,async {
        let token=tokio::time::timeout(Duration::from_secs(15),installation.token()).await.context("DuckDB OAuth deadline")?.map_err(|_|anyhow!("DuckDB OAuth failed"))?;
        operator=Some(tokio::time::timeout(Duration::from_secs(15),connect_mcp_client(installation.operator.resource.as_str(),&token)).await.context("DuckDB connection deadline")?.map_err(|_|anyhow!("DuckDB connection failed"))?);
        let token=tokio::time::timeout(Duration::from_secs(15),administrator.token()).await.context("DuckDB foreign OAuth deadline")?.map_err(|_|anyhow!("DuckDB foreign OAuth failed"))?;
        foreign=Some(tokio::time::timeout(Duration::from_secs(15),connect_mcp_client(administrator.resource.as_str(),&token)).await.context("DuckDB foreign connection deadline")?.map_err(|_|anyhow!("DuckDB foreign connection failed"))?);
        let client=operator.as_ref().expect("admitted");
        admit(client,&mut file,&mut receipt).await?;
        let (existing_databases, _) = databases(client,&mut file,&mut receipt).await?;
        let (existing_usage, _) = usage(client,&mut file,&mut receipt).await?;
        ensure!(existing_databases.len()<=3000 && existing_usage.len()<=3000,"DuckDB existing corpus exceeds preeffect traversal budget");
        let seed=101usize.saturating_sub(existing_databases.len()).max(101usize.saturating_sub(existing_usage.len()));
        let prefix=format!("accept_{}",uuid::Uuid::new_v4().simple());
        let primary=DuckDbDatabaseId::new(format!("{prefix}_000"))?;
        // Serial dispatch is deliberately below the four-request admission ceiling.
        for index in 0..seed {
            let db=DuckDbDatabaseId::new(format!("{prefix}_{index:03}"))?;
            ensure!(!existing_databases.contains(&db),"fresh fixture database already exists");
            receipt.fixture_databases.push(db.clone());
            let mut request=DuckDbExecuteRequest::new(db.clone(),"CREATE TABLE fixture_marker AS SELECT 1 AS ready".parse()?); request.create_if_missing=true;
            let output=dispatch(client,Mutation::Create{request},deadline,&mut notification,&mut file,&mut receipt).await?;
            ensure!(matches!(output,Output::Created(ref value) if value.db==db && value.db_created && value.statements==1),"DuckDB fixture creation output differs");
        }
        let table=DuckDbTableName::new("forecast_input")?;
        if seed==0 { ensure!(!existing_databases.contains(&primary),"fresh ingest database already exists"); receipt.fixture_databases.push(primary.clone()); }
        let mut request=DuckDbIngestRequest::new(primary.clone(),table.clone(),DuckDbTabularSource::InlineCsv{csv:"step,value\n0,1\n1,2\n2,3\n3,4\n".to_owned(),filename:Some("fixture.csv".to_owned()),options:DuckDbReadOptions::default()}.into(),DuckDbIngestMode::Create);request.create_db_if_missing=seed==0;
        let output=dispatch(client,Mutation::Ingest{request},deadline,&mut notification,&mut file,&mut receipt).await?;
        ensure!(matches!(output,Output::Ingested(ref value) if value.db==primary && value.table==table && value.rows_ingested==4 && value.db_created==(seed==0)),"DuckDB ingest output differs");
        let target=DuckDbDatabaseUri::new(primary.clone()).to_uri();
        let schema:DuckDbDatabaseSchema=json(client,&target,&mut file,&mut receipt).await?;
        receipt.schema_readback=Some(schema.clone());persist(&mut file,&receipt)?;
        require_schema(&schema,&primary,seed>0)?;
        let request=DuckDbExportRequest::Tabular{db:primary.clone(),selection:DuckDbTabularSelection::Sql{sql:"SELECT step, value FROM forecast_input ORDER BY step".parse()?},format:DuckDbTabularFormat::Csv};
        let Output::Exported(output)=dispatch(client,Mutation::Export{request},deadline,&mut notification,&mut file,&mut receipt).await? else {bail!("DuckDB export output kind differs")};
        ensure!(output.db()==&primary && output.rows_exported()==4,"DuckDB exported database/rows differ");
        let target=veoveo_artifact_mcp::contract::metadata_uri(output.artifact().artifact_id());
        let metadata:ArtifactMetadata=json(client,&target,&mut file,&mut receipt).await?;
        receipt.artifact_metadata=Some(metadata.clone());persist(&mut file,&receipt)?;
        let origin=require_metadata(&primary,&output,&metadata)?;
        let native=origin.task_id().context("DuckDB export metadata omitted native Task identity")?;
        receipt.native_export_task=Some(native);persist(&mut file,&receipt)?;
        ensure!(!existing_usage.contains(&native),"DuckDB export native Task already existed");
        let presented=DuckDbResource::Artifact(metadata.artifact_id()).to_uri()?;
        let bytes=blob(client,&presented,&mut file,&mut receipt).await?;
        let occurrence=veoveo_artifact_mcp::contract::ArtifactResource::Occurrence(metadata.artifact_id()).to_uri();
        let public_bytes=blob(client,&occurrence,&mut file,&mut receipt).await?;
        require_bytes(&metadata,&bytes,&public_bytes)?;
        receipt.artifact_sha256=Some(hex_digest(&bytes));persist(&mut file,&receipt)?;
        let usage_uri=DuckDbTaskUsageUri::new(native)?.to_uri()?;
        let report:veoveo_mcp_contract::UsageReport=json(client,&usage_uri,&mut file,&mut receipt).await?;
        receipt.export_usage=Some(report.clone());persist(&mut file,&receipt)?;
        require_usage(&report,native,&usage_uri,metadata.artifact_id(),&primary)?;
        let (final_databases,db_pages)=databases(client,&mut file,&mut receipt).await?;
        require_database_membership(&existing_databases,&receipt.fixture_databases,&final_databases,db_pages)?;
        let (final_usage,usage_pages)=usage(client,&mut file,&mut receipt).await?;
        ensure!(usage_pages>=2 && final_usage.contains(&native) && final_usage.len()>=existing_usage.len()+seed+2,"DuckDB usage did not deliver multiple complete pages or new operation records");
        let new_usage:Vec<_>=final_usage.difference(&existing_usage).copied().collect();
        ensure!(new_usage.len()==seed+2,"DuckDB concurrent usage changed the selected fixture ledger");
        let mut created=BTreeSet::new();let mut ingested=false;
        for id in new_usage {
            if id==native {continue;}
            let target=DuckDbTaskUsageUri::new(id)?.to_uri()?;
            let report:veoveo_mcp_contract::UsageReport=json(client,&target,&mut file,&mut receipt).await?;
            receipt.corpus_usage.push(report.clone());persist(&mut file,&receipt)?;
            ensure!(report.task_id==id.to_string()&&report.usage_uri==target.as_str()&&report.records.len()==1&&report.total_amount.is_none()&&report.currency.is_none()&&report.total_kind==Some(veoveo_mcp_contract::UsageKind::Actual),"DuckDB corpus usage parent/count/total differs");
            let record=&report.records[0];
            ensure!(record.task_id==id.to_string()&&record.kind==veoveo_mcp_contract::UsageKind::Actual&&record.amount.is_none()&&record.currency.is_none()&&record.provider_job_id.is_none()&&record.source_id.is_none(),"DuckDB corpus usage authority/charge differs");
            let details:DuckDbUsageDetails=serde_json::from_value(record.metadata.clone())?;
            match details {
                DuckDbUsageDetails::Execute{db,statements}=>{
                    let output=receipt.dispatches.iter().find_map(|dispatch|match &dispatch.output {Some(Output::Created(output)) if output.db==db=>Some(output),_=>None}).context("DuckDB execute usage has no retained matching creation output")?;
                    require_execute_quantity(record,output)?;
                    ensure!(statements==1&&record.model_id=="duckdb/execute"&&receipt.fixture_databases.contains(&db)&&created.insert(db),"DuckDB corpus execute usage differs");
                },
                DuckDbUsageDetails::Ingest{db,table:actual}=>{ensure!(!ingested&&db==primary&&actual==table&&record.model_id=="duckdb/ingest"&&record.quantity==Some(4.0)&&record.unit.as_deref()==Some("row"),"DuckDB source ingest usage differs");ingested=true;},
                _=>bail!("DuckDB corpus usage operation differs"),
            }
        }
        ensure!(created.len()==seed&&ingested,"DuckDB corpus usage omits a fixture mutation");
        let foreign=foreign.as_ref().expect("admitted");
        denied(foreign,&DuckDbDatabaseUri::new(primary.clone()).to_uri(),format!("unknown database `{primary}`"),&mut file,&mut receipt).await?;
        receipt.foreign_database_denied=true;persist(&mut file,&receipt)?;
        denied(foreign,&usage_uri,"unknown usage Task".to_owned(),&mut file,&mut receipt).await?;
        receipt.foreign_usage_denied=true;persist(&mut file,&receipt)?;
        Ok::<_,anyhow::Error>(())
    }).await.context("DuckDB operation exceeded600seconds; retained outcomes require inspection").and_then(|result|result);
    let cleanup = notification.close().await;
    receipt.subscription_closed = notification.listener_closed;
    let mut closed = true;
    for connection in [operator.take(), foreign.take()].into_iter().flatten() {
        if !matches!(
            tokio::time::timeout(Duration::from_secs(10), connection.cancel()).await,
            Ok(Ok(_))
        ) {
            closed = false;
        }
    }
    receipt.connections_closed = closed;
    receipt.finish(result.is_ok() && cleanup.is_ok() && closed);
    persist(&mut file, &receipt)?;
    result?;
    cleanup?;
    ensure!(closed, "DuckDB client cleanup failed");
    Ok(())
}

async fn dispatch(
    client: &SmokeMcpClient,
    intent: Mutation,
    deadline: tokio::time::Instant,
    state: &mut TaskNotificationState,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<Output> {
    let arguments = intent.arguments()?;
    let tool = intent.tool();
    let index = prepare_dispatch(file, receipt, intent.clone())?;
    let result = complete_tool_with_notification(
        client,
        tool,
        arguments,
        deadline.min(tokio::time::Instant::now() + Duration::from_secs(30)),
        Duration::from_secs(30),
        |event| {
            match event {
                TaskNotificationObservation::Admitted(id) => {
                    receipt.dispatches[index].task_id = Some(id.clone())
                }
                TaskNotificationObservation::Completed { task_id, payload } => {
                    receipt.dispatches[index].task_id = Some(task_id.clone());
                    receipt.dispatches[index].completed_delivered = true;
                    let output = Output::admit(&intent, payload);
                    if let Ok(output) = &output {
                        receipt.dispatches[index].output = Some(output.clone());
                    }
                    persist(file, receipt)?;
                    output?;
                }
            }
            persist(file, receipt)
        },
        state,
    )
    .await;
    receipt.dispatches[index].subscription_closed = state.listener_closed;
    persist(file, receipt)?;
    result?;
    receipt.dispatches[index]
        .output
        .clone()
        .context("DuckDB completed Task output missing")
}
fn prepare_dispatch(
    file: &mut std::fs::File,
    receipt: &mut Receipt,
    intent: Mutation,
) -> Result<usize> {
    ensure!(
        receipt.dispatches.len() < 103,
        "DuckDB mutation budget exhausted"
    );
    let index = receipt.dispatches.len();
    receipt.dispatches.push(Dispatch {
        intent,
        dispatched: false,
        task_id: None,
        completed_delivered: false,
        output: None,
        subscription_closed: false,
    });
    persist(file, receipt)?;
    receipt.dispatches[index].dispatched = true;
    receipt.outcome = Outcome::MutationUnresolved;
    if let Err(error) = persist(file, receipt) {
        receipt.dispatches[index].dispatched = false;
        receipt.finish(false);
        return Err(error);
    }
    Ok(index)
}
fn hex_digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
async fn fetch(
    client: &SmokeMcpClient,
    reader: Reader,
    target: &ResourceUri,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<std::result::Result<ReadResourceResult, ObservedFailure>> {
    ensure!(
        receipt.reads.len() < 256,
        "DuckDB resource budget exhausted"
    );
    let index = receipt.reads.len();
    receipt.reads.push(Read {
        reader,
        target: target.clone(),
        outcome: ReadOutcome::Pending,
    });
    persist(file, receipt)?;
    match tokio::time::timeout(
        Duration::from_secs(15),
        client.read_resource(ReadResourceRequestParams::new(target.as_str())),
    )
    .await
    {
        Ok(Ok(response)) => {
            let bytes = serde_json::to_vec(&response)?;
            receipt.reads[index].outcome = ReadOutcome::Received {
                digest: hex_digest(&bytes),
            };
            persist(file, receipt)?;
            ensure!(bytes.len() <= 65536, "DuckDB resource exceeds64KiB");
            Ok(Ok(response))
        }
        Ok(Err(rmcp::ServiceError::McpError(error))) => {
            let failure = ObservedFailure::mcp(i64::from(error.code.0), error.message.as_ref());
            receipt.reads[index].outcome = ReadOutcome::Mcp {
                failure: failure.clone(),
            };
            persist(file, receipt)?;
            Ok(Err(failure))
        }
        Ok(Err(_)) => {
            receipt.reads[index].outcome = ReadOutcome::Transport;
            persist(file, receipt)?;
            bail!("DuckDB resource transport failed")
        }
        Err(_) => {
            receipt.reads[index].outcome = ReadOutcome::Timeout;
            persist(file, receipt)?;
            bail!("DuckDB resource deadline")
        }
    }
}
async fn json<T: DeserializeOwned>(
    client: &SmokeMcpClient,
    target: &ResourceUri,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<T> {
    let response = fetch(client, Reader::Operator, target, file, receipt)
        .await?
        .map_err(|_| anyhow!("DuckDB required resource denied; see receipt"))?;
    let [ResourceContents::TextResourceContents { uri, text, .. }] = response.contents.as_slice()
    else {
        bail!("DuckDB resource must contain one JSON body")
    };
    ensure!(uri == target.as_str(), "DuckDB resource identity differs");
    serde_json::from_str(text).map_err(|_| anyhow!("DuckDB resource failed owner admission"))
}
async fn blob(
    client: &SmokeMcpClient,
    target: &ResourceUri,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<Vec<u8>> {
    let response = fetch(client, Reader::Operator, target, file, receipt)
        .await?
        .map_err(|_| anyhow!("DuckDB Artifact denied; see receipt"))?;
    let [ResourceContents::BlobResourceContents { uri, blob, .. }] = response.contents.as_slice()
    else {
        bail!("DuckDB Artifact must contain one blob")
    };
    ensure!(uri == target.as_str(), "DuckDB Artifact URI differs");
    STANDARD
        .decode(blob)
        .map_err(|_| anyhow!("DuckDB Artifact is not base64"))
}
async fn denied(
    client: &SmokeMcpClient,
    target: &ResourceUri,
    message: String,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<()> {
    let response = fetch(client, Reader::Administrator, target, file, receipt).await?;
    ensure!(
        matches!(response,Err(actual) if actual==ObservedFailure::mcp(-32602,message)),
        "DuckDB foreign read denial differs; see receipt"
    );
    Ok(())
}
async fn databases(
    client: &SmokeMcpClient,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<(BTreeSet<DuckDbDatabaseId>, usize)> {
    let mut ids = BTreeSet::new();
    let mut cursor = None;
    for page_index in 0..PAGE_LIMIT {
        let page: DuckDbDatabasePage = json(
            client,
            &DuckDbResource::Databases(cursor).to_uri()?,
            file,
            receipt,
        )
        .await?;
        receipt.database_pages.push(page.clone());
        persist(file, receipt)?;
        for item in page.items() {
            ensure!(
                ids.last().is_none_or(|last| last < item.id()) && ids.insert(item.id().clone()),
                "DuckDB catalog pages repeat/reorder identities"
            );
        }
        match page.next_cursor() {
            Some(next) => cursor = Some(next.clone()),
            None => return Ok((ids, page_index + 1)),
        }
    }
    bail!("DuckDB catalog exceeds32pages")
}
async fn usage(
    client: &SmokeMcpClient,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<(BTreeSet<TaskId>, usize)> {
    let mut ids = BTreeSet::new();
    let mut cursor = None;
    for page_index in 0..PAGE_LIMIT {
        let target = DuckDbUsageIndexUri::new(cursor.as_ref()).to_uri()?;
        let page: DuckDbUsagePage = json(client, &target, file, receipt).await?;
        receipt.usage_pages.push(page.clone());
        persist(file, receipt)?;
        for item in page.items() {
            let id = item.task_id();
            ensure!(
                ids.last().is_none_or(|last| *last < id) && ids.insert(id),
                "DuckDB usage pages repeat/reorder identities"
            );
        }
        match page.next_cursor() {
            Some(next) => cursor = Some(next.clone()),
            None => return Ok((ids, page_index + 1)),
        }
    }
    bail!("DuckDB usage exceeds32pages")
}
fn admit_configuration(installation: &InstalledTarget, path: &Path) -> Result<Sha256Digest> {
    let bytes = std::fs::read(path)?;
    let admission = veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
        .bind(veoveo_gateway_catalog::registry()?)?;
    let catalog = veoveo_mcp_gateway::GatewayCatalog::from_control_plane(
        serde_json::from_slice(&bytes)?,
        admission,
    )?;
    let slug = veoveo_types::ServerSlug::parse("duckdb")?;
    let (_, exposure, manifest) = catalog
        .profile_server(&installation.operator.profile, &slug)
        .context("DuckDB is not exposed by selected operator profile")?;
    ensure!(
        exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled && manifest.capabilities.tasks,
        "DuckDB Tasks must be enabled before fixture dispatch"
    );
    for name in ["execute", "ingest", "export"] {
        let name = veoveo_types::LocalToolName::parse(name)?;
        let exposed = match &exposure.tools {
            veoveo_mcp_contract::Exposure::All => true,
            veoveo_mcp_contract::Exposure::Listed(names) => names.contains(&name),
            veoveo_mcp_contract::Exposure::None => false,
        };
        ensure!(
            exposed && manifest.tools.contains(&name),
            "DuckDB required tool not exposed in checked profile: {name}"
        );
    }
    Ok(hex_digest(&bytes))
}
async fn catalog<T: Serialize>(
    method: CatalogMethod,
    request: impl std::future::Future<Output = Result<T>>,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<T> {
    let index = receipt.catalogs.len();
    receipt.catalogs.push(CatalogRead {
        method,
        outcome: ReadOutcome::Pending,
    });
    persist(file, receipt)?;
    match tokio::time::timeout(Duration::from_secs(30), request).await {
        Ok(Ok(value)) => {
            receipt.catalogs[index].outcome = ReadOutcome::Received {
                digest: hex_digest(&serde_json::to_vec(&value)?),
            };
            persist(file, receipt)?;
            Ok(value)
        }
        Ok(Err(error)) => {
            receipt.catalogs[index].outcome = match error.downcast_ref::<rmcp::ServiceError>() {
                Some(rmcp::ServiceError::McpError(error)) => ReadOutcome::Mcp {
                    failure: ObservedFailure::mcp(i64::from(error.code.0), error.message.as_ref()),
                },
                Some(_) => ReadOutcome::Transport,
                None => ReadOutcome::RequestFailed,
            };
            persist(file, receipt)?;
            bail!("DuckDB catalog request failed; see private receipt")
        }
        Err(_) => {
            receipt.catalogs[index].outcome = ReadOutcome::Timeout;
            persist(file, receipt)?;
            bail!("DuckDB catalog request deadline; see private receipt")
        }
    }
}
async fn admit(
    client: &SmokeMcpClient,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<()> {
    ensure!(
        client
            .peer_info()
            .is_some_and(|info| info.capabilities.supports_tasks()),
        "DuckDB gateway does not advertise Tasks"
    );
    let tools = catalog(
        CatalogMethod::Tools,
        veoveo_mcp_conformance::catalog::tools(client.peer()),
        file,
        receipt,
    )
    .await?;
    receipt.actual_tools = Some(
        tools
            .iter()
            .map(|tool| veoveo_gateway_contract::GatewayToolName::parse(tool.name.as_ref()))
            .collect::<std::result::Result<_, _>>()?,
    );
    persist(file, receipt)?;
    let templates = catalog(
        CatalogMethod::Templates,
        veoveo_mcp_conformance::catalog::templates(client.peer()),
        file,
        receipt,
    )
    .await?;
    receipt.actual_templates = Some(
        templates
            .iter()
            .map(|template| veoveo_types::ResourceTemplateUri::new(template.uri_template.clone()))
            .collect::<std::result::Result<_, _>>()?,
    );
    persist(file, receipt)?;
    require_catalogs(receipt)
}
fn require_catalogs(receipt: &Receipt) -> Result<()> {
    for expected in &receipt.expected_tools {
        ensure!(
            receipt.actual_tools.as_ref().is_some_and(|actual| actual
                .iter()
                .filter(|name| *name == expected)
                .count()
                == 1),
            "DuckDB required tool missing/duplicated: {expected}"
        );
    }
    for expected in &receipt.expected_templates {
        ensure!(
            receipt
                .actual_templates
                .as_ref()
                .is_some_and(|actual| actual.iter().filter(|uri| *uri == expected).count() == 1),
            "DuckDB required template missing/duplicated: {expected}"
        );
    }
    Ok(())
}
fn require_schema(
    schema: &DuckDbDatabaseSchema,
    db: &DuckDbDatabaseId,
    marker: bool,
) -> Result<()> {
    ensure!(
        schema.tables.len() == if marker { 2 } else { 1 },
        "DuckDB schema table inventory differs"
    );
    if marker {
        let found: Vec<_> = schema
            .tables
            .iter()
            .filter(|table| table.name == "fixture_marker")
            .collect();
        ensure!(
            found.len() == 1
                && found[0].columns.len() == 1
                && found[0].columns[0].name == "ready"
                && found[0].columns[0].data_type == "INTEGER",
            "DuckDB marker schema differs"
        );
    }
    ensure!(&schema.db_id == db, "DuckDB schema database differs");
    let tables: Vec<_> = schema
        .tables
        .iter()
        .filter(|table| table.name == "forecast_input")
        .collect();
    ensure!(
        tables.len() == 1
            && tables[0].columns.len() == 2
            && tables[0].columns[0].name == "step"
            && tables[0].columns[1].name == "value"
            && tables[0]
                .columns
                .iter()
                .all(|column| column.data_type == "BIGINT"),
        "DuckDB source schema differs"
    );
    Ok(())
}
fn require_metadata(
    db: &DuckDbDatabaseId,
    output: &DuckDbExportOutput,
    current: &ArtifactMetadata,
) -> Result<DuckDbArtifactOrigin> {
    ensure!(
        output.artifact().clone().without_download_url()
            == current
                .clone()
                .without_download_url()
                .presented_under_scheme(&veoveo_duckdb_mcp::uris::SCHEME)
            && current.mime_type.as_deref() == Some("text/csv"),
        "DuckDB current Artifact metadata differs from publication"
    );
    ensure!(
        current.artifact_uri == current.artifact_id().plane_uri(),
        "DuckDB current Artifact metadata is not neutral"
    );
    ensure!(
        output.artifact().artifact_uri
            == veoveo_artifact_contract::ArtifactUri::presented(
                &veoveo_duckdb_mcp::uris::SCHEME,
                current.artifact_id()
            ),
        "DuckDB output Artifact presentation differs"
    );
    let origin: DuckDbArtifactOrigin = serde_json::from_value(current.metadata.clone())
        .context("DuckDB Artifact origin failed admission")?;
    ensure!(
        origin.database() == db
            && matches!(
                origin.operation(),
                DuckDbArtifactOperation::ExportSql { row_count: 4 }
            )
            && origin.task_id().is_some(),
        "DuckDB Artifact provenance differs from submitted export"
    );
    Ok(origin)
}
fn require_bytes(metadata: &ArtifactMetadata, presented: &[u8], public: &[u8]) -> Result<()> {
    ensure!(
        presented == public
            && metadata.byte_len == presented.len() as u64
            && presented == b"step,value\n0,1\n1,2\n2,3\n3,4\n",
        "DuckDB exported CSV/Artifact byte handoff differs"
    );
    Ok(())
}
fn require_database_membership(
    baseline: &BTreeSet<DuckDbDatabaseId>,
    fixtures: &[DuckDbDatabaseId],
    actual: &BTreeSet<DuckDbDatabaseId>,
    pages: usize,
) -> Result<()> {
    let expected: BTreeSet<_> = baseline.iter().chain(fixtures).cloned().collect();
    ensure!(
        pages >= 2 && actual == &expected,
        "DuckDB complete catalog differs from baseline plus owned fixtures"
    );
    Ok(())
}
fn require_execute_quantity(
    record: &veoveo_mcp_contract::UsageRecord,
    output: &DuckDbExecuteOutput,
) -> Result<()> {
    ensure!(
        record.quantity == Some(output.rows_changed as f64)
            && record.unit.as_deref() == Some("row"),
        "DuckDB execute usage quantity/unit differs from retained creation output"
    );
    Ok(())
}
fn require_usage(
    report: &veoveo_mcp_contract::UsageReport,
    native: TaskId,
    target: &ResourceUri,
    artifact: veoveo_artifact_contract::ArtifactId,
    _db: &DuckDbDatabaseId,
) -> Result<()> {
    ensure!(
        report.task_id == native.to_string()
            && report.usage_uri == target.as_str()
            && report.records.len() == 1
            && report.total_amount.is_none()
            && report.currency.is_none()
            && report.total_kind == Some(veoveo_mcp_contract::UsageKind::Actual),
        "DuckDB usage parent/count/total differs"
    );
    let record = &report.records[0];
    let details: DuckDbUsageDetails = serde_json::from_value(record.metadata.clone())?;
    ensure!(
        record.task_id == native.to_string()
            && record.kind == veoveo_mcp_contract::UsageKind::Actual
            && record.model_id == "duckdb/export"
            && record.quantity == Some(4.0)
            && record.unit.as_deref() == Some("row")
            && record.amount.is_none()
            && record.currency.is_none()
            && record.source_id.is_none()
            && record.provider_job_id.is_none()
            && matches!(details,DuckDbUsageDetails::Export{db:actual_db,artifact:actual} if actual==artifact && &actual_db==_db),
        "DuckDB export usage differs"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(db: &DuckDbDatabaseId) -> Result<ArtifactMetadata> {
        let origin = DuckDbArtifactOrigin::new(
            db.clone(),
            DuckDbArtifactOperation::ExportSql { row_count: 4 },
        )
        .with_task(TaskId::new())?;
        Ok(ArtifactMetadata {
            byte_len: 27,
            mime_type: Some("text/csv".to_owned()),
            filename: Some("fixture.csv".to_owned()),
            artifact_uri: veoveo_artifact_contract::ArtifactUri::plane(
                veoveo_artifact_contract::ArtifactId::new(),
            ),
            download_url: None,
            created_at: chrono::Utc::now(),
            release_state: Default::default(),
            compliance: Default::default(),
            metadata: serde_json::to_value(origin)?,
        })
    }
    fn mutation() -> Result<Mutation> {
        let mut request = DuckDbExecuteRequest::new(
            DuckDbDatabaseId::new("owned_fixture")?,
            "CREATE TABLE fixture_marker AS SELECT 1 AS ready".parse()?,
        );
        request.create_if_missing = true;
        Ok(Mutation::Create { request })
    }
    #[test]
    fn failed_catalog_admission_retains_actual_members_and_no_dispatch() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("receipt.json");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let mut receipt = Receipt::new()?;
        receipt.actual_tools = Some(receipt.expected_tools.clone());
        receipt.actual_templates = Some(vec![veoveo_types::ResourceTemplateUri::new(
            veoveo_duckdb_mcp::uris::DB_TEMPLATE,
        )?]);
        persist(&mut file, &receipt)?;
        let error = require_catalogs(&receipt).unwrap_err();
        ensure!(
            error
                .to_string()
                .contains(veoveo_artifact_mcp::contract::METADATA_TEMPLATE),
            "failure omits required member"
        );
        let saved: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        ensure!(
            saved["actualTemplates"] == serde_json::to_value(&receipt.actual_templates)?
                && saved["actualTools"] == serde_json::to_value(&receipt.actual_tools)?,
            "failed admission lost observed catalogs"
        );
        ensure!(
            receipt.dispatches.is_empty() && receipt.outcome == Outcome::NotDispatched,
            "catalog failure dispatched mutation"
        );
        Ok(())
    }
    #[test]
    fn journal_failure_prevents_dispatch_and_does_not_invent_timeout() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("read-only");
        std::fs::write(&path, b"existing")?;
        let mut file = std::fs::File::open(path)?;
        let mut receipt = Receipt::new()?;
        ensure!(
            prepare_dispatch(&mut file, &mut receipt, mutation()?).is_err(),
            "read-only journal accepted intent"
        );
        receipt.reads.push(Read {
            reader: Reader::Operator,
            target: DuckDbResource::Databases(None).to_uri()?,
            outcome: ReadOutcome::Pending,
        });
        receipt.finish(false);
        ensure!(
            receipt.outcome == Outcome::NotDispatched
                && !receipt.dispatches[0].dispatched
                && matches!(receipt.reads[0].outcome, ReadOutcome::Interrupted),
            "journal failure invented effect/deadline"
        );
        Ok(())
    }
    #[tokio::test]
    async fn cancellation_retains_admitted_task_and_cleanup_failure_cannot_pass() -> Result<()> {
        let mut receipt = Receipt::new()?;
        let id = CanonicalTaskId::parse("opaque-retained-task")?;
        let result = tokio::time::timeout(Duration::from_millis(1), async {
            receipt.dispatches.push(Dispatch {
                intent: mutation()?,
                dispatched: true,
                task_id: Some(id.clone()),
                completed_delivered: false,
                output: None,
                subscription_closed: false,
            });
            std::future::pending::<Result<()>>().await
        })
        .await;
        ensure!(result.is_err(), "pending operation unexpectedly settled");
        receipt.finish(true);
        ensure!(
            receipt.dispatches[0].task_id.as_ref() == Some(&id)
                && receipt.outcome == Outcome::MutationUnresolved,
            "cancelled operation lost known Task"
        );
        receipt.dispatches[0].completed_delivered = true;
        receipt.finish(false);
        ensure!(
            receipt.outcome == Outcome::ObservedFailure,
            "settled cleanup failure claims passed/unresolved"
        );
        Ok(())
    }
    #[test]
    fn agreeing_wrong_provenance_or_csv_bytes_do_not_qualify() -> Result<()> {
        let db = DuckDbDatabaseId::new("owned_fixture")?;
        let mut metadata = artifact(&db)?;
        let bytes = b"step,value\n0,1\n1,2\n2,3\n3,4\n";
        metadata.byte_len = bytes.len() as u64;
        let output = DuckDbExportOutput::new(
            db.clone(),
            4,
            metadata
                .clone()
                .presented_under_scheme(&veoveo_duckdb_mcp::uris::SCHEME),
        );
        require_metadata(&db, &output, &metadata)?;
        require_bytes(&metadata, bytes, bytes)?;
        let foreign = DuckDbDatabaseId::new("different_owner_db")?;
        metadata.metadata = serde_json::to_value(
            DuckDbArtifactOrigin::new(foreign, DuckDbArtifactOperation::ExportSql { row_count: 4 })
                .with_task(TaskId::new())?,
        )?;
        let agreeing = DuckDbExportOutput::new(
            db.clone(),
            4,
            metadata
                .clone()
                .presented_under_scheme(&veoveo_duckdb_mcp::uris::SCHEME),
        );
        ensure!(
            require_metadata(&db, &agreeing, &metadata).is_err(),
            "consistent wrong database provenance accepted"
        );
        ensure!(
            require_bytes(
                &metadata,
                b"step,value\n0,9\n1,2\n2,3\n3,4\n",
                b"step,value\n0,9\n1,2\n2,3\n3,4\n"
            )
            .is_err(),
            "consistent wrong CSV accepted"
        );
        Ok(())
    }
    #[test]
    fn final_catalog_rejects_baseline_omission_and_unrelated_addition() -> Result<()> {
        let original = DuckDbDatabaseId::new("baseline")?;
        let owned = DuckDbDatabaseId::new("owned_fixture")?;
        let baseline = BTreeSet::from([original.clone()]);
        let fixtures = vec![owned.clone()];
        let expected = BTreeSet::from([original, owned.clone()]);
        require_database_membership(&baseline, &fixtures, &expected, 2)?;
        ensure!(
            require_database_membership(&baseline, &fixtures, &BTreeSet::from([owned]), 2).is_err(),
            "catalog accepted omitted baseline database"
        );
        let mut unexpected = expected;
        unexpected.insert(DuckDbDatabaseId::new("unrelated")?);
        ensure!(
            require_database_membership(&baseline, &fixtures, &unexpected, 2).is_err(),
            "catalog accepted unrelated database"
        );
        Ok(())
    }
    #[test]
    fn execute_usage_rejects_missing_or_wrong_quantity_and_unit() -> Result<()> {
        let output = DuckDbExecuteOutput {
            db: DuckDbDatabaseId::new("owned_fixture")?,
            statements: 1,
            rows_changed: 7,
            db_created: true,
        };
        let record = veoveo_mcp_contract::UsageRecord {
            task_id: TaskId::new().to_string(),
            source_id: None,
            provider_job_id: None,
            model_id: "duckdb/execute".to_owned(),
            kind: veoveo_mcp_contract::UsageKind::Actual,
            quantity: Some(7.0),
            unit: Some("row".to_owned()),
            amount: None,
            currency: None,
            recorded_at: chrono::Utc::now(),
            metadata: serde_json::to_value(DuckDbUsageDetails::Execute {
                db: output.db.clone(),
                statements: 1,
            })?,
        };
        require_execute_quantity(&record, &output)?;
        for quantity in [None, Some(6.0)] {
            let mut wrong = record.clone();
            wrong.quantity = quantity;
            ensure!(
                require_execute_quantity(&wrong, &output).is_err(),
                "invalid execute quantity accepted"
            );
        }
        for unit in [None, Some("statement".to_owned())] {
            let mut wrong = record.clone();
            wrong.unit = unit;
            ensure!(
                require_execute_quantity(&wrong, &output).is_err(),
                "invalid execute unit accepted"
            );
        }
        Ok(())
    }
    #[test]
    fn export_usage_rejects_charges_provider_and_wrong_database() -> Result<()> {
        let db = DuckDbDatabaseId::new("owned_fixture")?;
        let native = TaskId::new();
        let artifact = veoveo_artifact_contract::ArtifactId::new();
        let target = DuckDbTaskUsageUri::new(native)?.to_uri()?;
        let record = veoveo_mcp_contract::UsageRecord {
            task_id: native.to_string(),
            source_id: None,
            provider_job_id: None,
            model_id: "duckdb/export".to_owned(),
            kind: veoveo_mcp_contract::UsageKind::Actual,
            quantity: Some(4.0),
            unit: Some("row".to_owned()),
            amount: None,
            currency: None,
            recorded_at: chrono::Utc::now(),
            metadata: serde_json::to_value(DuckDbUsageDetails::Export {
                db: db.clone(),
                artifact,
            })?,
        };
        let report = veoveo_mcp_contract::UsageReport::new(native.to_string(), target.as_str())
            .with_records(vec![record]);
        require_usage(&report, native, &target, artifact, &db)?;
        let mut charged = report.clone();
        charged.records[0].amount = Some(1.0);
        ensure!(
            require_usage(&charged, native, &target, artifact, &db).is_err(),
            "charged fixture accepted"
        );
        let mut provider = report.clone();
        provider.records[0].provider_job_id = Some("unexpected-job".to_owned());
        ensure!(
            require_usage(&provider, native, &target, artifact, &db).is_err(),
            "provider fixture accepted"
        );
        let mut wrong = report;
        wrong.records[0].metadata = serde_json::to_value(DuckDbUsageDetails::Export {
            db: DuckDbDatabaseId::new("wrong_db")?,
            artifact,
        })?;
        ensure!(
            require_usage(&wrong, native, &target, artifact, &db).is_err(),
            "wrong database usage accepted"
        );
        Ok(())
    }
}
