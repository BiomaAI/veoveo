use std::collections::BTreeSet;
use veoveo_reason_mcp::contract::ReasonTaskKind;
use veoveo_types::TaskTypeDefinition;

use rmcp::{ErrorData as McpError, model::CompletionInfo};
use veoveo_platform_store::{RecordId, deterministic_principal_id, deterministic_tenant_id};
use veoveo_reason_mcp::contract::{AnalysisCursor, AnalysisId, AnalysisPage};
use veoveo_task_runtime::{TaskOwner, TaskPageCursor, TaskRuntime};

use super::{internal, resources::analysis_view};

const PAGE_SIZE: usize = 100;

pub(super) async fn analyses_page(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    after: Option<&AnalysisCursor>,
) -> Result<AnalysisPage, McpError> {
    let after = after.map(|cursor| TaskPageCursor {
        created_at: cursor.created_at(),
        task_id: cursor.analysis_id().task_id(),
    });
    let page = tasks
        .for_owner(owner)
        .of_type(ReasonTaskKind::AnalyzeRecording.name())
        .page(after.as_ref(), PAGE_SIZE)
        .await
        .map_err(internal)?;
    Ok(AnalysisPage {
        analyses: page
            .items
            .iter()
            .map(analysis_view)
            .collect::<Result<_, _>>()?,
        limit: PAGE_SIZE,
        next_cursor: page
            .next_cursor
            .map(|position| {
                AnalysisId::try_from(position.task_id)
                    .map(|id| AnalysisCursor::new(position.created_at, id))
            })
            .transpose()
            .map_err(internal)?,
    })
}

#[derive(Clone, Copy)]
pub(super) enum CompletionDomain {
    Analyses,
    Artifacts,
}

pub(super) async fn complete(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    domain: CompletionDomain,
    needle: &str,
) -> Result<CompletionInfo, McpError> {
    let statements: &[&str] = match domain {
        CompletionDomain::Analyses => &[include_str!(
            "../../../queries/bin/server/index/completion_task.surql"
        )],
        CompletionDomain::Artifacts => &[
            include_str!("../../../queries/bin/server/index/completion_results.surql"),
            include_str!("../../../queries/bin/server/index/completion_annotations.surql"),
            include_str!("../../../queries/bin/server/index/completion_source_clip.surql"),
        ],
    };
    let mut values = BTreeSet::new();
    for &statement in statements {
        let mut response = tasks
            .platform_store()
            .client()
            .query(statement)
            .bind((
                "task_type",
                ReasonTaskKind::AnalyzeRecording.name().to_string(),
            ))
            .bind(("server", RecordId::new("mcp_server", "reason")))
            .bind((
                "tenant",
                deterministic_tenant_id(owner.tenant_key())
                    .map_err(internal)?
                    .record_id(),
            ))
            .bind((
                "owner",
                deterministic_principal_id(owner.tenant_key(), &owner.principal_key)
                    .map_err(internal)?
                    .record_id(),
            ))
            .bind(("profile", RecordId::new("profile", owner.profile.clone())))
            .bind(("tenant_key", owner.tenant_key.clone()))
            .bind(("principal_key", owner.principal_key.clone()))
            .bind(("profile_key", owner.profile.clone()))
            .bind(("data_labels", owner.data_labels.clone()))
            .bind(("needle", needle.to_ascii_lowercase()))
            .bind(("limit", CompletionInfo::MAX_VALUES + 1))
            .await
            .map_err(internal)?
            .check()
            .map_err(internal)?;
        let candidates: Vec<String> = response.take(0).map_err(internal)?;
        values.extend(candidates);
    }
    let has_more = values.len() > CompletionInfo::MAX_VALUES;
    let total = (!has_more).then_some(values.len() as u32);
    CompletionInfo::with_pagination(
        values
            .into_iter()
            .take(CompletionInfo::MAX_VALUES)
            .collect(),
        total,
        has_more,
    )
    .map_err(internal)
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
