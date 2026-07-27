use std::collections::BTreeMap;

use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactPut, ArtifactWriteIdempotencyKey, ComplianceMetadata,
    IssuedArtifactWriteCapability, now_utc,
};
use veoveo_optimization_mcp::{
    contract::PlanOutput, plan_artifacts::PlanArtifactBytes, planning::PlanRun, state::TaskOwner,
};
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
    let plan_artifact = store_artifact(
        state,
        require_capability(capability)?,
        owner,
        run.plan_json,
        "plan-json",
        "canonical governed plan",
    )
    .await?;
    let mut output = PlanOutput {
        plan: run.plan,
        plan_artifact,
        duckdb_artifact: None,
        rrd_artifact: None,
    };
    if let Some(artifact) = run.duckdb.take() {
        output.duckdb_artifact = Some(
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
        output.rrd_artifact = Some(
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
    record_usage(state, task_id, &output).await?;

    let mut blocks = vec![ContentBlock::text(format!(
        "plan {} completed with status {:?}; produced {} assignment(s) from {} generated candidate(s)",
        output.plan.plan_id,
        output.plan.status,
        output.plan.metrics.assignments,
        output.plan.metrics.generated_candidates
    ))];
    blocks.push(ContentBlock::ResourceLink(
        Resource::new(
            output.plan.resource_uri.clone(),
            format!("plan {}", output.plan.plan_id),
        )
        .with_title("Governed spatial plan")
        .with_description("Typed immutable plan, assignments, findings, metrics, and provenance.")
        .with_mime_type("application/json"),
    ));
    blocks.push(artifact_link(
        &output.plan_artifact,
        "canonical plan JSON",
        "Canonical immutable plan artifact with the recorded plan digest.",
    ));
    if let Some(artifact) = &output.duckdb_artifact {
        blocks.push(artifact_link(
            artifact,
            "plan DuckDB",
            "DuckDB snapshot containing assignments, requirements, and the governed plan.",
        ));
    }
    if let Some(artifact) = &output.rrd_artifact {
        blocks.push(artifact_link(
            artifact,
            "plan RRD",
            "Rerun recording containing plan assignments, metrics, and provenance.",
        ));
    }

    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(&output)?);
    Ok(result)
}

async fn store_artifact(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    owner: &TaskOwner,
    artifact: PlanArtifactBytes,
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
            quantity: Some(output.plan.metrics.generated_candidates as f64),
            unit: Some("candidate".to_owned()),
            amount: None,
            currency: None,
            recorded_at: now_utc(),
            metadata: OpenObject::new(BTreeMap::from([
                (
                    "selected".into(),
                    serde_json::json!(output.plan.metrics.assignments),
                ),
                ("tasks".into(), serde_json::json!(output.plan.metrics.tasks)),
                (
                    "agents".into(),
                    serde_json::json!(output.plan.metrics.agents),
                ),
                ("status".into(), serde_json::json!(output.plan.status)),
                ("solver".into(), serde_json::json!(output.plan.solver)),
                ("plan_id".into(), serde_json::json!(output.plan.plan_id)),
                (
                    "plan_digest_sha256".into(),
                    serde_json::json!(output.plan.plan_digest_sha256),
                ),
            ])),
        })
        .await?;
    Ok(())
}
