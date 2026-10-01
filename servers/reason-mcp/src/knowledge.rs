//! Reusable findings inherit read access from the published results Artifact.
//! Task control and its owner-scoped resources keep their separate admission.
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use veoveo_artifact_contract::ArtifactId;
use veoveo_platform_store::{
    ArtifactReadScope, OpenObject, PlatformStore, RecordId, task_record_id,
};
use veoveo_types::TaskTypeDefinition;

use crate::contract::{AnalysisId, AnalyzeRecordingOutput, ModelId, PipelineId, ReasonTaskKind};
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

/// Only identities and stored times leave the admitted Task row. Other output
/// Artifacts can have different grants and are not part of this read grant.
#[derive(Clone, Debug, PartialEq)]
pub struct AdmittedFinding {
    pub position: FindingPosition,
    pub updated_at: DateTime<Utc>,
    pub pipeline: PipelineId,
    pub model: ModelId,
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
                .query(sql(include_str!("knowledge/read.surql"))),
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
    type Row = (
        String,
        DateTime<Utc>,
        DateTime<Utc>,
        OpenObject,
        Option<DateTime<Utc>>,
    );
    let rows: Vec<Row> = response.take(0)?;
    rows.into_iter()
        .map(|(id, created_at, updated_at, output, expires_at)| {
            let analysis = AnalysisId::parse(&id)?;
            let output: AnalyzeRecordingOutput =
                serde_json::from_value(serde_json::to_value(output)?)?;
            ensure!(
                output.analysis_id() == analysis,
                "Reason finding does not match its Task"
            );
            ensure!(
                updated_at >= created_at,
                "Reason finding timestamps are inconsistent"
            );
            Ok(AdmittedFinding {
                position: FindingPosition {
                    created_at,
                    analysis,
                },
                updated_at,
                pipeline: output.pipeline_uri.id().clone(),
                model: output.model_uri.id().clone(),
                results: output.results_artifact.artifact_id(),
                expires_at,
            })
        })
        .collect()
}

fn sql(template: &str) -> String {
    template
        .replace(
            "{{FINDING_ADMISSION}}",
            include_str!("knowledge/admitted.surql"),
        )
        .replace("{{ADMISSION}}", ArtifactReadScope::ADMISSION)
}
