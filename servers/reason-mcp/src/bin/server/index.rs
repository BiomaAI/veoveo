use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rmcp::{ErrorData as McpError, model::CompletionInfo};
use serde::{Deserialize, Serialize};
use veoveo_platform_store::{RecordId, deterministic_principal_id, deterministic_tenant_id};
use veoveo_reason_mcp::{contract::AnalysisView, uris};
use veoveo_task_runtime::{TaskOwner, TaskPageCursor, TaskRuntime};

use super::{analysis_view, internal};

const PAGE_SIZE: usize = 100;
const CURSOR_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnalysisCursor {
    version: u8,
    collection: String,
    position: TaskPageCursor,
}

#[derive(Serialize)]
pub(super) struct AnalysisPage {
    analyses: Vec<AnalysisView>,
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

pub(super) fn parse_collection(uri: &str) -> Result<Option<Option<AnalysisCursor>>, McpError> {
    if uri == uris::ANALYSES_URI {
        return Ok(Some(None));
    }
    let Some(query) = uri.strip_prefix("reason://analyses?") else {
        return Ok(None);
    };
    let invalid = || McpError::invalid_params("invalid Reason analyses cursor", None);
    let encoded = query
        .strip_prefix("cursor=")
        .filter(|value| {
            !value.is_empty() && value.len() <= 1024 && !value.contains(['&', '=', '?', '#'])
        })
        .ok_or_else(invalid)?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| invalid())?;
    let cursor: AnalysisCursor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if cursor.version != CURSOR_VERSION || cursor.collection != uris::ANALYSES_URI {
        return Err(invalid());
    }
    Ok(Some(Some(cursor)))
}

fn encode_cursor(position: TaskPageCursor) -> Result<String, McpError> {
    serde_json::to_vec(&AnalysisCursor {
        version: CURSOR_VERSION,
        collection: uris::ANALYSES_URI.to_owned(),
        position,
    })
    .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
    .map_err(internal)
}

pub(super) async fn analyses_page(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    after: Option<&AnalysisCursor>,
) -> Result<AnalysisPage, McpError> {
    let page = tasks
        .list_page_for_owner(
            owner,
            &["analyze_recording"],
            after.map(|cursor| &cursor.position),
            PAGE_SIZE,
        )
        .await
        .map_err(internal)?;
    Ok(AnalysisPage {
        analyses: page
            .items
            .iter()
            .map(analysis_view)
            .collect::<Result<_, _>>()?,
        limit: PAGE_SIZE,
        next_cursor: page.next_cursor.map(encode_cursor).transpose()?,
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
    // Only these repository-owned expressions enter SQL. User input is bound.
    let fields: &[&str] = match domain {
        CompletionDomain::Analyses => &["type::string(record::id(id))"],
        CompletionDomain::Artifacts => &[
            "result.structuredContent.results_artifact.artifact_id",
            "result.structuredContent.annotations_artifact.artifact_id",
            "result.structuredContent.source_clip_artifact.artifact_id",
        ],
    };
    let mut values = BTreeSet::new();
    for field in fields {
        let statement = format!(
            "SELECT VALUE candidate FROM (SELECT {field} AS candidate FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND (request.owner.tenant_key ?? NONE) = $tenant_key AND request.owner.data_labels ALLINSIDE $data_labels AND task_type = 'analyze_recording' AND {field} CONTAINS $needle GROUP BY candidate ORDER BY candidate ASC LIMIT $limit);"
        );
        let mut response = tasks
            .platform_store()
            .client()
            .query(statement)
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
