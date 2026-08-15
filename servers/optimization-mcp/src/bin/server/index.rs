use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rmcp::ErrorData as McpError;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_optimization_mcp::domain::{
    OptimizationSolutionUri, OptimizationToolOutput, ProblemId, RunId,
};
use veoveo_platform_store::{
    RecordId, TaskId, TaskRecord, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id,
};
use veoveo_task_runtime::TaskSnapshot;

use super::{
    app_state::AppState,
    ownership::{optional_task_owner, task_owner_allows},
    records::{
        OPTIMIZE_ROUTE_SCENARIOS_TASK, OPTIMIZE_ROUTES_TASK, OptimizationTaskRequest,
        SOLVE_CONVEX_TASK, SOLVE_MILP_TASK,
    },
};

pub(super) const OPTIMIZATION_INDEX_CURSOR_VERSION: u8 = 1;
pub(super) const OPTIMIZATION_INDEX_PAGE_SIZE: usize = 100;
const INSTALLATION_TENANT: &str = "installation";
const USAGE_INDEX_CURSOR_VERSION: u8 = 1;

const SOLVE_TASK_TYPES: [&str; 4] = [
    OPTIMIZE_ROUTES_TASK,
    OPTIMIZE_ROUTE_SCENARIOS_TASK,
    SOLVE_CONVEX_TASK,
    SOLVE_MILP_TASK,
];

const TASK_PAGE_QUERY: &str = "SELECT * FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND task_type IN $task_types ORDER BY created_at ASC, id ASC LIMIT $limit;";
const TASK_PAGE_AFTER_QUERY: &str = "SELECT * FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND task_type IN $task_types AND (created_at > $after_created_at OR (created_at = $after_created_at AND id > $after_task)) ORDER BY created_at ASC, id ASC LIMIT $limit;";
const SOLUTION_PAGE_QUERY: &str = "SELECT * FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND task_type IN $task_types AND status = 'succeeded' AND result != NONE ORDER BY created_at ASC, id ASC LIMIT $limit;";
const SOLUTION_PAGE_AFTER_QUERY: &str = "SELECT * FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND task_type IN $task_types AND status = 'succeeded' AND result != NONE AND (created_at > $after_created_at OR (created_at = $after_created_at AND id > $after_task)) ORDER BY created_at ASC, id ASC LIMIT $limit;";
const PROBLEM_COMPLETION_QUERY: &str = "SELECT VALUE request.input.common.problem_id FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND task_type IN $task_types AND request.input.common.problem_id CONTAINS $needle GROUP BY request.input.common.problem_id ORDER BY request.input.common.problem_id ASC LIMIT $limit;";
const RUN_COMPLETION_QUERY: &str = "SELECT VALUE request.input.common.run_id FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND task_type IN $task_types AND request.input.common.run_id CONTAINS $needle GROUP BY request.input.common.run_id ORDER BY request.input.common.run_id ASC LIMIT $limit;";
const SOLUTION_COMPLETION_QUERY: &str = "SELECT VALUE result.structuredContent.result_uri FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND task_type IN $task_types AND status = 'succeeded' AND result.structuredContent.result_uri CONTAINS $needle GROUP BY result.structuredContent.result_uri ORDER BY result.structuredContent.result_uri ASC LIMIT $limit;";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OptimizationCollection {
    Problems,
    Runs,
    Solutions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OptimizationCompletionDomain {
    Problems,
    Runs,
    Solutions,
}

#[derive(Debug)]
pub(super) struct OptimizationCompletionPage {
    pub values: Vec<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OptimizationIndexCursor {
    version: u8,
    collection: OptimizationCollection,
    created_at: DateTime<Utc>,
    task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OptimizationCollectionRequest {
    pub collection: OptimizationCollection,
    pub cursor: Option<OptimizationIndexCursor>,
}

#[derive(Debug)]
pub(super) struct VisibleOptimizationTask {
    pub snapshot: TaskSnapshot,
    pub request: OptimizationTaskRequest,
    pub output: Option<OptimizationToolOutput>,
}

#[derive(Debug)]
pub(super) struct VisibleOptimizationTaskPage {
    pub items: Vec<VisibleOptimizationTask>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OptimizationUsageCursor {
    version: u8,
    task_id: TaskId,
}

#[derive(Debug, Serialize)]
pub(super) struct OptimizationUsageIndexEntry {
    task_id: String,
    usage_uri: String,
}

#[derive(Debug, Serialize)]
pub(super) struct OptimizationUsageIndexPage {
    usage: Vec<OptimizationUsageIndexEntry>,
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(super) enum OptimizationIndexError {
    #[error("invalid Optimization index cursor")]
    InvalidCursor,
    #[error("invalid Optimization collection URI")]
    InvalidCollectionUri,
}

pub(super) fn encode_index_cursor(
    cursor: &OptimizationIndexCursor,
) -> Result<String, serde_json::Error> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(cursor)?))
}

pub(super) fn decode_index_cursor(
    collection: OptimizationCollection,
    value: &str,
) -> Result<OptimizationIndexCursor, OptimizationIndexError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| OptimizationIndexError::InvalidCursor)?;
    let cursor: OptimizationIndexCursor =
        serde_json::from_slice(&bytes).map_err(|_| OptimizationIndexError::InvalidCursor)?;
    if cursor.version != OPTIMIZATION_INDEX_CURSOR_VERSION || cursor.collection != collection {
        return Err(OptimizationIndexError::InvalidCursor);
    }
    Ok(cursor)
}

pub(super) fn parse_collection_uri(
    uri: &str,
) -> Result<Option<OptimizationCollectionRequest>, OptimizationIndexError> {
    let (collection, root) = if uri == "optimization://problems"
        || uri.starts_with("optimization://problems?")
    {
        (OptimizationCollection::Problems, "optimization://problems")
    } else if uri == "optimization://runs" || uri.starts_with("optimization://runs?") {
        (OptimizationCollection::Runs, "optimization://runs")
    } else if uri == "optimization://solutions" || uri.starts_with("optimization://solutions?") {
        (
            OptimizationCollection::Solutions,
            "optimization://solutions",
        )
    } else {
        return Ok(None);
    };
    if uri == root {
        return Ok(Some(OptimizationCollectionRequest {
            collection,
            cursor: None,
        }));
    }
    let cursor = uri
        .strip_prefix(root)
        .and_then(|suffix| suffix.strip_prefix("?cursor="))
        .filter(|cursor| !cursor.is_empty() && !cursor.contains(['&', '=', '?', '#']))
        .ok_or(OptimizationIndexError::InvalidCollectionUri)?;
    Ok(Some(OptimizationCollectionRequest {
        collection,
        cursor: Some(decode_index_cursor(collection, cursor)?),
    }))
}

fn encode_usage_cursor(task_id: TaskId) -> String {
    URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&OptimizationUsageCursor {
            version: USAGE_INDEX_CURSOR_VERSION,
            task_id,
        })
        .expect("the controlled Optimization usage cursor serializes"),
    )
}

fn decode_usage_cursor(value: &str) -> Result<TaskId, OptimizationIndexError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| OptimizationIndexError::InvalidCursor)?;
    let cursor: OptimizationUsageCursor =
        serde_json::from_slice(&bytes).map_err(|_| OptimizationIndexError::InvalidCursor)?;
    if cursor.version != USAGE_INDEX_CURSOR_VERSION {
        return Err(OptimizationIndexError::InvalidCursor);
    }
    Ok(cursor.task_id)
}

pub(super) fn parse_usage_index_uri(
    uri: &str,
) -> Result<Option<Option<TaskId>>, OptimizationIndexError> {
    if uri == veoveo_optimization_mcp::uris::USAGE_URI {
        return Ok(Some(None));
    }
    let Some(cursor) = uri
        .strip_prefix("optimization://usage?cursor=")
        .filter(|cursor| !cursor.is_empty() && !cursor.contains(['&', '=', '?', '#']))
    else {
        return if uri.starts_with("optimization://usage?") {
            Err(OptimizationIndexError::InvalidCollectionUri)
        } else {
            Ok(None)
        };
    };
    Ok(Some(Some(decode_usage_cursor(cursor)?)))
}

pub(super) async fn load_usage_index_page(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    after: Option<TaskId>,
) -> Result<OptimizationUsageIndexPage, McpError> {
    let page = state
        .tasks
        .platform_store()
        .domain_usage_task_page("optimization", after, OPTIMIZATION_INDEX_PAGE_SIZE)
        .await
        .map_err(internal)?;
    let mut usage = Vec::with_capacity(page.task_ids.len());
    for task_id in page.task_ids {
        let task_id = task_id.to_string();
        let Some(owner) = optional_task_owner(state, &task_id).await? else {
            continue;
        };
        if task_owner_allows(&owner, identity) {
            usage.push(OptimizationUsageIndexEntry {
                usage_uri: veoveo_optimization_mcp::uris::usage_task_uri(&task_id),
                task_id,
            });
        }
    }
    Ok(OptimizationUsageIndexPage {
        usage,
        limit: OPTIMIZATION_INDEX_PAGE_SIZE,
        next_cursor: page.next_task_id.map(encode_usage_cursor),
    })
}

pub(super) async fn visible_task_page(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    request: &OptimizationCollectionRequest,
) -> Result<VisibleOptimizationTaskPage, McpError> {
    let query = match (request.collection, request.cursor.is_some()) {
        (OptimizationCollection::Solutions, false) => SOLUTION_PAGE_QUERY,
        (OptimizationCollection::Solutions, true) => SOLUTION_PAGE_AFTER_QUERY,
        (_, false) => TASK_PAGE_QUERY,
        (_, true) => TASK_PAGE_AFTER_QUERY,
    };
    let authority = QueryAuthority::new(identity).map_err(internal)?;
    let mut query = state
        .tasks
        .platform_store()
        .client()
        .query(query)
        .bind(("server", RecordId::new("mcp_server", "optimization")))
        .bind(("tenant", authority.tenant))
        .bind(("owner", authority.owner))
        .bind((
            "profile",
            RecordId::new("profile", identity.profile.to_string()),
        ))
        .bind(("work_context", authority.work_context))
        .bind(("data_labels", authority.data_labels))
        .bind(("task_types", SOLVE_TASK_TYPES.map(str::to_owned).to_vec()))
        .bind(("limit", (OPTIMIZATION_INDEX_PAGE_SIZE + 1) as i64));
    if let Some(cursor) = &request.cursor {
        query = query
            .bind(("after_created_at", cursor.created_at))
            .bind(("after_task", cursor.task_id.record_id()));
    }
    let mut response = query.await.map_err(internal)?.check().map_err(internal)?;
    let records: Vec<TaskRecord> = response.take(0).map_err(internal)?;
    let has_more = records.len() > OPTIMIZATION_INDEX_PAGE_SIZE;
    let mut items = records
        .into_iter()
        .take(OPTIMIZATION_INDEX_PAGE_SIZE)
        .map(visible_task_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if has_more {
        let last = items
            .last()
            .expect("an overfull task page has one returned item");
        Some(
            encode_index_cursor(&OptimizationIndexCursor {
                version: OPTIMIZATION_INDEX_CURSOR_VERSION,
                collection: request.collection,
                created_at: last.snapshot.created_at,
                task_id: last.snapshot.task_id,
            })
            .map_err(internal)?,
        )
    } else {
        None
    };
    items.shrink_to_fit();
    Ok(VisibleOptimizationTaskPage { items, next_cursor })
}

pub(super) async fn completion_candidates(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    domain: OptimizationCompletionDomain,
    needle: &str,
    page_size: usize,
) -> Result<OptimizationCompletionPage, McpError> {
    let statement = match domain {
        OptimizationCompletionDomain::Problems => PROBLEM_COMPLETION_QUERY,
        OptimizationCompletionDomain::Runs => RUN_COMPLETION_QUERY,
        OptimizationCompletionDomain::Solutions => SOLUTION_COMPLETION_QUERY,
    };
    let authority = QueryAuthority::new(identity).map_err(internal)?;
    let mut response = state
        .tasks
        .platform_store()
        .client()
        .query(statement)
        .bind(("server", RecordId::new("mcp_server", "optimization")))
        .bind(("tenant", authority.tenant))
        .bind(("owner", authority.owner))
        .bind((
            "profile",
            RecordId::new("profile", identity.profile.to_string()),
        ))
        .bind(("work_context", authority.work_context))
        .bind(("data_labels", authority.data_labels))
        .bind(("task_types", SOLVE_TASK_TYPES.map(str::to_owned).to_vec()))
        .bind(("needle", needle.to_ascii_lowercase()))
        .bind(("limit", page_size.saturating_add(1) as i64))
        .await
        .map_err(internal)?
        .check()
        .map_err(internal)?;
    let stored_values: Vec<String> = response.take(0).map_err(internal)?;
    let has_more = stored_values.len() > page_size;
    let values = stored_values
        .into_iter()
        .take(page_size)
        .filter_map(|value| match domain {
            OptimizationCompletionDomain::Problems => {
                ProblemId::parse(value).ok().map(|id| id.to_string())
            }
            OptimizationCompletionDomain::Runs => RunId::parse(value).ok().map(|id| id.to_string()),
            OptimizationCompletionDomain::Solutions => {
                veoveo_optimization_mcp::uris::parse_solution_uri(&value).map(|id| id.to_string())
            }
        })
        .collect();
    Ok(OptimizationCompletionPage { values, has_more })
}

pub(super) async fn find_problem_task(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    problem_id: &ProblemId,
) -> Result<Option<VisibleOptimizationTask>, McpError> {
    find_task(
        state,
        identity,
        "SELECT * FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND request.input.common.problem_id = $identity LIMIT 2;",
        problem_id.to_string(),
    )
    .await
}

pub(super) async fn find_run_task(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    run_id: &RunId,
) -> Result<Option<VisibleOptimizationTask>, McpError> {
    find_task(
        state,
        identity,
        "SELECT * FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND request.input.common.run_id = $identity LIMIT 2;",
        run_id.to_string(),
    )
    .await
}

pub(super) async fn find_solution_task(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    solution_uri: &OptimizationSolutionUri,
) -> Result<Option<VisibleOptimizationTask>, McpError> {
    find_task(
        state,
        identity,
        "SELECT * FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $work_context AND request.owner.data_labels ALLINSIDE $data_labels AND result.structuredContent.result_uri = $identity LIMIT 2;",
        solution_uri.to_string(),
    )
    .await
}

async fn find_task(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    statement: &'static str,
    domain_identity: String,
) -> Result<Option<VisibleOptimizationTask>, McpError> {
    let authority = QueryAuthority::new(identity).map_err(internal)?;
    let mut response = state
        .tasks
        .platform_store()
        .client()
        .query(statement)
        .bind(("server", RecordId::new("mcp_server", "optimization")))
        .bind(("tenant", authority.tenant))
        .bind(("owner", authority.owner))
        .bind((
            "profile",
            RecordId::new("profile", identity.profile.to_string()),
        ))
        .bind(("work_context", authority.work_context))
        .bind(("data_labels", authority.data_labels))
        .bind(("identity", domain_identity))
        .await
        .map_err(internal)?
        .check()
        .map_err(internal)?;
    let records: Vec<TaskRecord> = response.take(0).map_err(internal)?;
    if records.len() > 1 {
        return Err(McpError::internal_error(
            "duplicate canonical Optimization identity",
            None,
        ));
    }
    records
        .into_iter()
        .next()
        .map(visible_task_from_record)
        .transpose()
}

fn visible_task_from_record(record: TaskRecord) -> Result<VisibleOptimizationTask, McpError> {
    let snapshot = TaskSnapshot::try_from(record).map_err(internal)?;
    let request = serde_json::from_value::<OptimizationTaskRequest>(snapshot.request.clone())
        .map_err(internal)?;
    let output = snapshot
        .result
        .as_ref()
        .and_then(|result| result.get("structuredContent"))
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .map_err(internal)?;
    Ok(VisibleOptimizationTask {
        snapshot,
        request,
        output,
    })
}

struct QueryAuthority {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    data_labels: Vec<String>,
}

impl QueryAuthority {
    fn new(identity: &GatewayInternalIdentity) -> Result<Self, veoveo_platform_store::StoreError> {
        let tenant_key = identity
            .actor
            .tenant
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| INSTALLATION_TENANT.to_owned());
        Ok(Self {
            tenant: deterministic_tenant_id(&tenant_key)?.record_id(),
            owner: deterministic_principal_id(&tenant_key, identity.actor.id.as_str())?.record_id(),
            work_context: deterministic_work_context_id(
                &tenant_key,
                identity.authority.work_context.as_str(),
            )?
            .record_id(),
            data_labels: identity
                .actor
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect(),
        })
    }
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_cursor_round_trips_and_is_bound_to_its_collection() {
        let cursor = OptimizationIndexCursor {
            version: OPTIMIZATION_INDEX_CURSOR_VERSION,
            collection: OptimizationCollection::Runs,
            created_at: Utc::now(),
            task_id: TaskId::new(),
        };
        let encoded = encode_index_cursor(&cursor).unwrap();

        assert_eq!(
            decode_index_cursor(OptimizationCollection::Runs, &encoded).unwrap(),
            cursor
        );
        assert!(decode_index_cursor(OptimizationCollection::Problems, &encoded).is_err());
    }

    #[test]
    fn collection_uri_accepts_only_one_opaque_cursor_parameter() {
        let cursor = OptimizationIndexCursor {
            version: OPTIMIZATION_INDEX_CURSOR_VERSION,
            collection: OptimizationCollection::Solutions,
            created_at: Utc::now(),
            task_id: TaskId::new(),
        };
        let encoded = encode_index_cursor(&cursor).unwrap();
        let uri = format!("optimization://solutions?cursor={encoded}");

        let request = parse_collection_uri(&uri).unwrap().unwrap();
        assert_eq!(request.collection, OptimizationCollection::Solutions);
        assert_eq!(request.cursor, Some(cursor));
        assert!(parse_collection_uri("optimization://solutions?limit=100").is_err());
    }
}
