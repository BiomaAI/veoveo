use std::sync::Arc;
use veoveo_task_runtime::TaskUsageAccess;

use futures::StreamExt;
use rmcp::{ErrorData as McpError, service::SubscriptionContext};
use tokio_util::sync::CancellationToken;
use veoveo_frames_mcp::contract::{
    FrameTaskUsageUri, FrameUsageIndexUri, FrameWorldId, FrameWorldsUri,
};
use veoveo_frames_mcp::uris;
use veoveo_platform_store::PlatformTable;
use veoveo_types::TaskId;

use super::{
    AppState,
    ownership::{frame_scope_from_identity, internal_identity, runtime_owner},
};

#[derive(Debug, PartialEq, Eq)]
enum Resource {
    Worlds,
    World(FrameWorldId),
    Usage,
    TaskUsage(TaskId),
}

fn parse_resource(uri: &str) -> Result<Resource, McpError> {
    if FrameWorldsUri::parse(uri).is_ok() {
        return Ok(Resource::Worlds);
    }
    if FrameUsageIndexUri::parse(uri).is_ok() {
        return Ok(Resource::Usage);
    }
    if let Some(world) = uris::parse_world_uri(uri) {
        return Ok(Resource::World(world.world_id()));
    }
    if let Ok(usage) = FrameTaskUsageUri::parse(uri) {
        return Ok(Resource::TaskUsage(usage.task_id()));
    }
    Err(McpError::invalid_params(
        "resource is immutable or not subscribable",
        None,
    ))
}

pub(super) async fn authorize(
    state: &AppState,
    context: &SubscriptionContext,
) -> Result<(), McpError> {
    let Some(uris) = context
        .accepted()
        .resource_subscriptions
        .as_ref()
        .filter(|uris| !uris.is_empty())
    else {
        return Ok(());
    };
    let request = context.request_context();
    let identity = internal_identity(request)?;
    let scope = frame_scope_from_identity(state, &identity).await?;
    for uri in uris {
        match parse_resource(uri)? {
            Resource::Worlds | Resource::Usage => {}
            Resource::World(world_id) => {
                if state
                    .frames
                    .get_world(&scope, &world_id)
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                    .is_none()
                {
                    return Err(McpError::resource_not_found("unknown frame world", None));
                }
            }
            Resource::TaskUsage(task_id) => {
                if !state
                    .tasks
                    .task_visible(TaskUsageAccess::Owner(&runtime_owner(&identity)), task_id)
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                {
                    return Err(McpError::resource_not_found("unknown usage task", None));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn spawn_observer(
    state: Arc<AppState>,
    cancellation: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut changes = state.tasks.platform_store().resource_changes(vec![
            PlatformTable::FrameWorld,
            PlatformTable::FrameWorldRevision,
            PlatformTable::DomainUsage,
            PlatformTable::Task,
        ]);
        loop {
            tokio::select! {
                () = cancellation.cancelled() => break,
                change = changes.next() => {
                    if change.is_none() { break; }
                    state.subscriptions.notify_resource_contents_changed().await;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscriptions_admit_mutable_worlds_and_usage() {
        assert_eq!(
            parse_resource(FrameWorldsUri::ROOT).unwrap(),
            Resource::Worlds
        );
        let cursor = veoveo_frames_mcp::contract::FrameWorldCursor::new(
            &FrameWorldId::new("world").unwrap(),
        );
        assert_eq!(
            parse_resource(FrameWorldsUri::new(Some(&cursor)).as_str()).unwrap(),
            Resource::Worlds
        );
        assert_eq!(
            parse_resource(FrameUsageIndexUri::ROOT).unwrap(),
            Resource::Usage
        );
        assert_eq!(
            parse_resource("frames://world/fixture").unwrap(),
            Resource::World(FrameWorldId::new("fixture").unwrap())
        );
        let task_id = TaskId::new();
        let usage = FrameTaskUsageUri::new(task_id).unwrap();
        assert_eq!(
            parse_resource(usage.as_str()).unwrap(),
            Resource::TaskUsage(task_id)
        );
        let cursor = veoveo_frames_mcp::contract::FrameUsageCursor::new(task_id).unwrap();
        assert_eq!(
            parse_resource(FrameUsageIndexUri::new(Some(&cursor)).as_str()).unwrap(),
            Resource::Usage
        );
        for uri in [
            uris::DOCS_URI,
            "frames://world/fixture/revision/rev-1",
            "frames://world/fixture/revision/rev-1/frame/body",
            "frames://usage/task/task-1/extra",
            "frames://usage/task/task-1",
            "frames://usage?cursor=bad",
            "frames://usage?unknown=x",
            "other://world/fixture",
            "frames://worlds?cursor=bad",
            "frames://worlds?unknown=x",
        ] {
            assert!(parse_resource(uri).is_err(), "{uri}");
        }
    }
}
