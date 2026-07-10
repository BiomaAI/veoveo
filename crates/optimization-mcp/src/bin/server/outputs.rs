use std::collections::BTreeMap;

use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactPut, ArtifactWriteIdempotencyKey, ComplianceMetadata,
    IssuedArtifactWriteCapability, now_utc,
};
use veoveo_optimization_mcp::{contract::PlanOutput, planning::PlanRun, state::TaskOwner};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, OpenObject};
use veoveo_task_runtime::TaskId;

use super::app_state::AppState;

pub(super) async fn plan_result(
    state: &AppState,
    capability: Option<&IssuedArtifactWriteCapability>,
    task_id: &str,
    owner: &TaskOwner,
    mut run: PlanRun,
) -> anyhow::Result<CallToolResult> {
    if let Some(artifact) = run.duckdb.take() {
        run.output.duckdb_artifact = Some(
            store_artifact(
                state,
                require_capability(capability)?,
                owner,
                artifact,
                "duckdb",
                "plan duckdb",
            )
            .await?,
        );
    }
    if let Some(artifact) = run.rrd.take() {
        run.output.rrd_artifact = Some(
            store_artifact(
                state,
                require_capability(capability)?,
                owner,
                artifact,
                "rerun_rrd",
                "plan rerun rrd",
            )
            .await?,
        );
    }
    record_usage(state, task_id, &run.output).await?;

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
    Ok(result)
}

async fn store_artifact(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    owner: &TaskOwner,
    artifact: veoveo_optimization_mcp::planning::PlanArtifactBytes,
    idempotency_suffix: &str,
    title: &str,
) -> anyhow::Result<ArtifactMetadata> {
    let mut put = ArtifactPut::new(artifact.bytes);
    put.mime_type = Some(artifact.mime_type.to_string());
    put.filename = Some(artifact.filename.to_string());
    // The plane stamps tenant + owner from the verified identity and records the
    // owner grant; carry the caller's labels as artifact classification.
    put.compliance = ComplianceMetadata {
        data_labels: owner.data_labels.clone(),
        ..Default::default()
    };
    put.metadata = artifact.metadata;
    let metadata = state
        .artifacts
        .put_with_capability(
            capability,
            ArtifactWriteIdempotencyKey::new(format!("optimization:{idempotency_suffix}"))?,
            put,
        )
        .await?;
    tracing::debug!(
        artifact_id = %metadata.artifact_id,
        title,
        "stored plan artifact"
    );
    Ok(metadata.without_download_url())
}

fn require_capability(
    capability: Option<&IssuedArtifactWriteCapability>,
) -> anyhow::Result<&IssuedArtifactWriteCapability> {
    capability.ok_or_else(|| anyhow::anyhow!("task did not reserve artifact write capability"))
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

async fn record_usage(state: &AppState, task_id: &str, output: &PlanOutput) -> anyhow::Result<()> {
    state
        .tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id: task_id.parse::<TaskId>()?,
            server: "optimization".to_owned(),
            source_id: None,
            provider_job_id: None,
            model_id: "optimization/good_lp-microlp".to_owned(),
            kind: DomainUsageKind::Actual,
            quantity: Some(output.summary.options as f64),
            unit: Some("option".to_owned()),
            amount: None,
            currency: None,
            recorded_at: now_utc(),
            metadata: OpenObject::new(BTreeMap::from([
                (
                    "selected".into(),
                    serde_json::json!(output.summary.selected),
                ),
                ("tasks".into(), serde_json::json!(output.summary.tasks)),
                ("agents".into(), serde_json::json!(output.summary.agents)),
                ("status".into(), serde_json::json!(output.status)),
                ("solver".into(), serde_json::json!(output.solver)),
            ])),
        })
        .await?;
    Ok(())
}
