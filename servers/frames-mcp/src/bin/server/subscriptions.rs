use std::sync::Arc;

use futures::StreamExt;
use rmcp::{ErrorData as McpError, service::SubscriptionContext};
use tokio_util::sync::CancellationToken;
use veoveo_frames_mcp::uris;
use veoveo_mcp_contract::FrameWorldId;
use veoveo_platform_store::PlatformTable;

use super::{
    AppState,
    ownership::{frame_scope_from_identity, internal_identity, require_task_owner},
};

#[derive(Debug, PartialEq, Eq)]
enum Resource<'a> {
    Worlds,
    World(FrameWorldId),
    Usage,
    TaskUsage(&'a str),
}

fn parse_resource(uri: &str) -> Result<Resource<'_>, McpError> {
    match uri {
        uris::WORLDS_URI => return Ok(Resource::Worlds),
        uris::USAGE_ROOT_URI => return Ok(Resource::Usage),
        _ => {}
    }
    if let Some(world) = uris::parse_world_uri(uri) {
        return Ok(Resource::World(world.world_id()));
    }
    if let Some(task) = uris::parse_usage_task_uri(uri) {
        return Ok(Resource::TaskUsage(task));
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
                require_task_owner(state, request, task_id).await?;
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
        ]);
        loop {
            tokio::select! {
                () = cancellation.cancelled() => break,
                change = changes.next() => {
                    if change.is_none() { break; }
                    state.subscriptions.notify_resources_changed().await;
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
        assert_eq!(parse_resource(uris::WORLDS_URI).unwrap(), Resource::Worlds);
        assert_eq!(
            parse_resource(uris::USAGE_ROOT_URI).unwrap(),
            Resource::Usage
        );
        assert_eq!(
            parse_resource("frames://world/fixture").unwrap(),
            Resource::World(FrameWorldId::new("fixture").unwrap())
        );
        assert_eq!(
            parse_resource("frames://usage/task/task-1").unwrap(),
            Resource::TaskUsage("task-1")
        );
        for uri in [
            uris::DOCS_URI,
            "frames://world/fixture/revision/rev-1",
            "frames://world/fixture/revision/rev-1/frame/body",
            "frames://usage/task/task-1/extra",
            "other://world/fixture",
        ] {
            assert!(parse_resource(uri).is_err(), "{uri}");
        }
    }
}
