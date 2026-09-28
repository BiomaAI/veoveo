use std::collections::BTreeSet;
use veoveo_stream_mcp::contract::StreamTaskKind;
use veoveo_types::TaskTypeDefinition;

use rmcp::{ErrorData as McpError, model::CompletionInfo};
use veoveo_platform_store::{RecordId, deterministic_principal_id, deterministic_tenant_id};
use veoveo_stream_mcp::contract::{LiveSessionsPage, RunCursor, RunId, RunPage, SessionCursor};
use veoveo_task_runtime::{TaskOwner, TaskPageCursor, TaskRuntime};

use super::{internal, live::LiveSessionManager, run_view};

const PAGE_SIZE: usize = 100;
pub(super) async fn runs_page(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    after: Option<&RunCursor>,
) -> Result<RunPage, McpError> {
    let after = after.map(|cursor| TaskPageCursor {
        created_at: cursor.created_at(),
        task_id: cursor.run_id().task_id(),
    });
    let page = tasks
        .for_owner(owner)
        .of_type(StreamTaskKind::RunRecording.name())
        .page(after.as_ref(), PAGE_SIZE)
        .await
        .map_err(internal)?;
    Ok(RunPage {
        runs: page.items.iter().map(run_view).collect::<Result<_, _>>()?,
        limit: PAGE_SIZE,
        next_cursor: page
            .next_cursor
            .map(|position| {
                RunId::try_from(position.task_id)
                    .map(|id| RunCursor::new(position.created_at, id))
                    .map_err(internal)
            })
            .transpose()?,
    })
}

pub(super) async fn sessions_page(
    live: &LiveSessionManager,
    owner: &TaskOwner,
    before: Option<&SessionCursor>,
) -> Result<LiveSessionsPage, McpError> {
    let page = live
        .page(owner, before.map(SessionCursor::session_id), PAGE_SIZE)
        .await;
    Ok(LiveSessionsPage {
        sessions: page.sessions,
        limit: PAGE_SIZE,
        next_cursor: page.next_before.map(SessionCursor::new),
    })
}

pub(super) fn bounded_completion(values: Vec<String>) -> Result<CompletionInfo, McpError> {
    let more = values.len() > CompletionInfo::MAX_VALUES;
    let total = (!more).then_some(values.len() as u32);
    CompletionInfo::with_pagination(
        values
            .into_iter()
            .take(CompletionInfo::MAX_VALUES)
            .collect(),
        total,
        more,
    )
    .map_err(internal)
}

#[derive(Clone, Copy)]
pub(super) enum CompletionDomain {
    Runs,
    Artifacts,
}

pub(super) async fn complete(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    domain: CompletionDomain,
    needle: &str,
) -> Result<CompletionInfo, McpError> {
    // Only these repository-owned expressions enter SQL. User input is bound.
    let fields: &[&str] = match domain {
        CompletionDomain::Runs => &["type::string(record::id(id))"],
        CompletionDomain::Artifacts => &[
            "result.structuredContent.results_artifact.artifact_id",
            "result.structuredContent.annotations_artifact.artifact_id",
            "result.structuredContent.source_clip_artifact.artifact_id",
        ],
    };
    let mut values = BTreeSet::new();
    for field in fields {
        let statement = format!(
            "SELECT VALUE candidate FROM (SELECT {field} AS candidate FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND (request.owner.tenant_key ?? NONE) = $tenant_key AND request.owner.data_labels ALLINSIDE $data_labels AND task_type = 'run_recording' AND {field} CONTAINS $needle GROUP BY candidate ORDER BY candidate ASC LIMIT $limit);"
        );
        let mut response = tasks
            .platform_store()
            .client()
            .query(statement)
            .bind(("server", RecordId::new("mcp_server", "stream")))
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
