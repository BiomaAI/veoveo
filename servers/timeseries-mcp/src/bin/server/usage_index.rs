use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rmcp::ErrorData as McpError;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_task_runtime::TaskId;

use super::{
    app_state::AppState,
    ownership::{optional_task_owner, task_owner_allows},
};

const USAGE_INDEX_CURSOR_VERSION: u8 = 1;
pub(super) const USAGE_INDEX_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageIndexCursor {
    version: u8,
    task_id: TaskId,
}

#[derive(Debug, Serialize)]
pub(super) struct UsageIndexEntry {
    task_id: String,
    usage_uri: String,
}

#[derive(Debug, Serialize)]
pub(super) struct UsageIndexPage {
    usage: Vec<UsageIndexEntry>,
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

pub(super) fn encode_usage_cursor(task_id: TaskId) -> String {
    let cursor = UsageIndexCursor {
        version: USAGE_INDEX_CURSOR_VERSION,
        task_id,
    };
    URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&cursor).expect("the controlled Timeseries usage cursor serializes"),
    )
}

pub(super) fn decode_usage_cursor(value: &str) -> Result<TaskId, &'static str> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| "invalid Timeseries usage cursor")?;
    let cursor: UsageIndexCursor =
        serde_json::from_slice(&bytes).map_err(|_| "invalid Timeseries usage cursor")?;
    if cursor.version != USAGE_INDEX_CURSOR_VERSION {
        return Err("invalid Timeseries usage cursor");
    }
    Ok(cursor.task_id)
}

pub(super) fn parse_usage_index_uri(uri: &str) -> Result<Option<Option<TaskId>>, &'static str> {
    if uri == veoveo_timeseries_mcp::uris::USAGE_ROOT_URI {
        return Ok(Some(None));
    }
    let Some(query) = uri.strip_prefix("timeseries://usage?") else {
        return Ok(None);
    };
    let cursor = query
        .strip_prefix("cursor=")
        .filter(|cursor| !cursor.is_empty() && !cursor.contains(['&', '=', '?', '#']))
        .ok_or("invalid Timeseries usage index URI")?;
    Ok(Some(Some(decode_usage_cursor(cursor)?)))
}

pub(super) async fn load_usage_index_page(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    after: Option<TaskId>,
) -> Result<UsageIndexPage, McpError> {
    let page = state
        .tasks
        .platform_store()
        .domain_usage_task_page("timeseries", after, USAGE_INDEX_PAGE_SIZE)
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
    let mut usage = Vec::with_capacity(page.task_ids.len());
    for task_id in page.task_ids {
        let task_id = task_id.to_string();
        let Some(owner) = optional_task_owner(state, &task_id).await? else {
            continue;
        };
        if task_owner_allows(&owner, identity) {
            usage.push(UsageIndexEntry {
                usage_uri: veoveo_timeseries_mcp::uris::usage_task_uri(&task_id),
                task_id,
            });
        }
    }
    Ok(UsageIndexPage {
        usage,
        limit: USAGE_INDEX_PAGE_SIZE,
        next_cursor: page.next_task_id.map(encode_usage_cursor),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_cursor_round_trips_as_an_opaque_task_position() {
        let task_id = TaskId::new();
        let encoded = encode_usage_cursor(task_id);
        assert_eq!(decode_usage_cursor(&encoded).unwrap(), task_id);
    }

    #[test]
    fn usage_uri_accepts_only_one_opaque_cursor_parameter() {
        let task_id = TaskId::new();
        let encoded = encode_usage_cursor(task_id);
        let uri = format!("timeseries://usage?cursor={encoded}");

        assert_eq!(parse_usage_index_uri(&uri).unwrap(), Some(Some(task_id)));
        assert_eq!(
            parse_usage_index_uri("timeseries://usage").unwrap(),
            Some(None)
        );
        assert!(parse_usage_index_uri("timeseries://usage?limit=100").is_err());
    }
}
