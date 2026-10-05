//! Reusable findings inherit read access from the published results Artifact.
//! Task control and its owner-scoped resources keep their separate admission.
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use veoveo_artifact_contract::ArtifactId;
use veoveo_platform_store::{
    ArtifactReadScope, PlatformStore, RecordId, TaskResultRecord, task_record_id,
};
use veoveo_types::TaskTypeDefinition;

use crate::contract::{AnalysisId, FindingData, ReasonTaskKind};
pub mod observe;
pub mod summary;

pub const FINDINGS_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FindingPosition {
    pub created_at: DateTime<Utc>,
    pub analysis: AnalysisId,
}

#[derive(Clone, Copy)]
pub enum FindingSelection<'a> {
    Page(Option<&'a FindingPosition>),
    Member(AnalysisId),
    Complete(&'a str),
}

/// Bounded finding data leaves the admitted Task row with its identity. Other output
/// Artifacts can have different grants and are not part of this read grant.
#[derive(Clone, Debug, PartialEq)]
pub struct AdmittedFinding {
    pub position: FindingPosition,
    pub updated_at: DateTime<Utc>,
    pub data: FindingData,
    pub results: ArtifactId,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Select success, tenant, current Artifact visibility and provenance in SQL.
/// A page includes one lookahead row. No caller input becomes query text.
pub async fn readable_findings(
    store: &PlatformStore,
    scope: &ArtifactReadScope,
    selection: FindingSelection<'_>,
) -> Result<Vec<AdmittedFinding>> {
    let (analysis, after, limit, prefix) = match selection {
        FindingSelection::Page(after) => (None, after, FINDINGS_PAGE_SIZE + 1, None),
        FindingSelection::Member(analysis) => (Some(analysis), None, 1, None),
        FindingSelection::Complete(prefix) => {
            (None, None, FINDINGS_PAGE_SIZE + 1, Some(prefix.to_owned()))
        }
    };
    let mut response = scope
        .bind(
            store
                .client()
                .query(include_str!("../queries/knowledge/read.surql")),
        )
        .bind(("reason_server", RecordId::new("mcp_server", "reason")))
        .bind((
            "reason_task_type",
            ReasonTaskKind::AnalyzeRecording.name().to_string(),
        ))
        .bind(("prefix", prefix))
        .bind(("analysis", analysis.map(|id| task_record_id(id.task_id()))))
        .bind((
            "after_id",
            after.map(|p| task_record_id(p.analysis.task_id())),
        ))
        .bind(("after_created_at", after.map(|p| p.created_at)))
        .bind(("limit", limit as i64))
        .await?
        .check()?;
    let rows: Vec<crate::task_lookup::FindingRow> = response.take(response.num_statements() - 1)?;
    rows.into_iter()
        .map(|row| {
            let surrealdb::types::RecordIdKey::Uuid(uuid) = row.task.key.clone() else {
                anyhow::bail!("Reason lookup Task key is not a UUID");
            };
            let analysis = AnalysisId::try_from(veoveo_types::TaskId::from_uuid(uuid.into()))?;
            ensure!(
                row.task == task_record_id(analysis.task_id())
                    && row.id == RecordId::new("reason_analysis", analysis.task_id().to_string())
                    && row.lifecycle.id == row.task
                    && row.lifecycle.tenant == row.identity.tenant
                    && row.lifecycle.task_type == row.task_type
                    && row.lifecycle.created_at == row.created_at
                    && row.lifecycle.completed_at == Some(row.settlement.completed_at)
                    && row.lifecycle.updated_at >= row.created_at
                    && row.settlement.status == crate::task_lookup::Terminal::Succeeded
                    && row.settlement.outcome == crate::task_lookup::Outcome::Product,
                "Reason lookup identity or settlement disagrees with its Task"
            );
            ensure!(
                row.lifecycle.result_matches,
                "Reason finding does not match its current Task result"
            );
            let result = row
                .settlement
                .expected_result
                .ok_or_else(|| anyhow::anyhow!("Reason integrity snapshot missing"))?;
            let mut wrapper = surrealdb::types::Object::new();
            wrapper.insert("payload", result);
            let payload = <TaskResultRecord as surrealdb::types::SurrealValue>::from_value(
                surrealdb::types::Value::Object(wrapper),
            )?
            .into_payload();
            let envelope: rmcp::model::CallToolResult = serde_json::from_value(payload)?;
            let output = crate::task_product::validate(&envelope)?.ok_or_else(|| {
                anyhow::anyhow!("Reason product lookup has a no-product integrity snapshot")
            })?;
            let data = row
                .settlement
                .finding
                .ok_or_else(|| anyhow::anyhow!("Reason finding missing"))?
                .0;
            let metadata = row
                .settlement
                .expected_metadata
                .ok_or_else(|| anyhow::anyhow!("Reason publication receipt missing"))?
                .0;
            let results = row
                .settlement
                .results
                .ok_or_else(|| anyhow::anyhow!("Reason results link missing"))?;
            let link = |id: ArtifactId| {
                RecordId::new(
                    "artifact_occurrence",
                    surrealdb::types::Uuid::from(id.as_uuid()),
                )
            };
            ensure!(
                output.analysis_id() == analysis
                    && *output.pipeline_uri.id() == row.identity.pipeline_id
                    && output.finding == data
                    && link(output.results_artifact.artifact_id()) == results
                    && row.settlement.annotations
                        == Some(link(output.annotations_artifact.artifact_id()))
                    && row.settlement.source_clip
                        == output
                            .source_clip_artifact
                            .as_ref()
                            .map(|a| link(a.artifact_id()))
                    && output.results_artifact.metadata == serde_json::to_value(metadata)?,
                "Reason finding lookup disagrees with its publication receipt"
            );
            Ok(AdmittedFinding {
                position: FindingPosition {
                    created_at: row.created_at,
                    analysis,
                },
                updated_at: row.lifecycle.updated_at,
                data,
                results: output.results_artifact.artifact_id(),
                expires_at: row.lifecycle.retention_expires_at,
            })
        })
        .collect()
}
