use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactPut, ComplianceMetadata, PlanOutput, UsageKind, UsageRecord, now_utc,
    set_related_task_meta,
};
use veoveo_optimization_mcp::{planning::PlanRun, state::TaskOwner};

use super::{app_state::AppState, ownership::artifact_owner_from_task};

pub(super) async fn plan_result(
    state: &AppState,
    task_id: &str,
    owner: &TaskOwner,
    mut run: PlanRun,
) -> anyhow::Result<CallToolResult> {
    if let Some(artifact) = run.duckdb.take() {
        run.output.duckdb_artifact =
            Some(store_artifact(state, owner, artifact, "plan duckdb").await?);
    }
    if let Some(artifact) = run.rrd.take() {
        run.output.rrd_artifact =
            Some(store_artifact(state, owner, artifact, "plan rerun rrd").await?);
    }
    record_usage(state, task_id, &run.output)?;

    let mut blocks = vec![ContentBlock::text(format!(
        "plan completed with status {:?}; selected {} of {} option(s)",
        run.output.status, run.output.summary.selected, run.output.summary.options
    ))];
    if let Some(artifact) = &run.output.duckdb_artifact {
        blocks.push(artifact_link(
            artifact,
            "plan DuckDB",
            "DuckDB snapshot containing selected options and plan summary.",
        ));
    }
    if let Some(artifact) = &run.output.rrd_artifact {
        blocks.push(artifact_link(
            artifact,
            "plan RRD",
            "Rerun recording containing plan metrics, selections, and provenance.",
        ));
    }

    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(&run.output)?);
    set_related_task_meta(&mut result.meta, task_id);
    Ok(result)
}

async fn store_artifact(
    state: &AppState,
    owner: &TaskOwner,
    artifact: veoveo_optimization_mcp::planning::PlanArtifactBytes,
    title: &str,
) -> anyhow::Result<ArtifactMetadata> {
    let mut put = ArtifactPut::new(artifact.bytes);
    put.mime_type = Some(artifact.mime_type.to_string());
    put.filename = Some(artifact.filename.to_string());
    put.compliance = ComplianceMetadata {
        tenant_id: owner.tenant.clone(),
        owner_id: Some(owner.principal_id.clone()),
        data_labels: owner.data_labels.clone(),
        ..Default::default()
    };
    put.metadata = artifact.metadata;
    let mut metadata = state.artifacts.put(put).await?;
    let artifact_owner = artifact_owner_from_task(&metadata.sha256, owner);
    state.durable.record_artifact_owner(&artifact_owner)?;
    metadata.compliance.owner_id = Some(owner.principal_id.clone());
    metadata.compliance.tenant_id = owner.tenant.clone();
    metadata.compliance.data_labels = owner.data_labels.clone();
    tracing::debug!(
        artifact_sha256 = metadata.sha256,
        title,
        "stored plan artifact"
    );
    Ok(metadata.without_download_url())
}

fn artifact_link(artifact: &ArtifactMetadata, title: &str, description: &str) -> ContentBlock {
    let mut resource = Resource::new(artifact.artifact_uri.clone(), title.to_string())
        .with_title(title.to_string())
        .with_description(description.to_string());
    if let Some(mime_type) = &artifact.mime_type {
        resource = resource.with_mime_type(mime_type.clone());
    }
    ContentBlock::ResourceLink(resource)
}

fn record_usage(state: &AppState, task_id: &str, output: &PlanOutput) -> anyhow::Result<()> {
    state.durable.record_usage(&UsageRecord {
        task_id: task_id.to_string(),
        source_id: None,
        provider_job_id: None,
        model_id: "optimization/good_lp-microlp".to_string(),
        kind: UsageKind::Actual,
        quantity: Some(output.summary.options as f64),
        unit: Some("option".to_string()),
        amount: None,
        currency: None,
        recorded_at: now_utc(),
        metadata: serde_json::json!({
            "selected": output.summary.selected,
            "tasks": output.summary.tasks,
            "agents": output.summary.agents,
            "status": output.status,
            "solver": output.solver,
        }),
    })?;
    Ok(())
}
