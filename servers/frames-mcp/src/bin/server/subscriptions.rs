use std::sync::Arc;
use veoveo_task_runtime::TaskUsageAccess;

use futures::StreamExt;
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use tokio_util::sync::CancellationToken;
use veoveo_frames_mcp::contract::{FrameWorldId, FramesResource};
use veoveo_mcp_contract::{
    SubscriptionHub,
    hosting::{ResourceSubscriptions, gateway_identity},
};
use veoveo_platform_store::PlatformTable;
use veoveo_types::TaskId;

use super::{
    AppState,
    ownership::{frame_scope_from_identity, runtime_owner},
};

/// What a subscription to one Frames address observes. Revisions, frames,
/// operations, artifacts and documents are immutable and refuse subscription.
#[derive(Debug, PartialEq, Eq)]
enum Watched {
    Catalog,
    World(FrameWorldId),
    TaskUsage(TaskId),
}

fn watched(address: FramesResource) -> Result<Watched, McpError> {
    match address {
        FramesResource::Worlds(_) | FramesResource::Usage(_) => Ok(Watched::Catalog),
        FramesResource::World(world) => Ok(Watched::World(world.world_id())),
        FramesResource::TaskUsage(usage) => Ok(Watched::TaskUsage(usage.task_id())),
        FramesResource::Docs
        | FramesResource::Document(_)
        | FramesResource::Contract
        | FramesResource::WorkspaceApp
        | FramesResource::Revision(_)
        | FramesResource::Frame(_)
        | FramesResource::Operation(_)
        | FramesResource::Artifact(_) => Err(McpError::invalid_params(
            "resource is immutable or not subscribable",
            None,
        )),
    }
}

/// Frames resource changes and the caller checks for subscribing to them.
pub(super) struct FramesSubscriptions {
    state: Arc<AppState>,
}

impl FramesSubscriptions {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl ResourceSubscriptions for FramesSubscriptions {
    type Address = FramesResource;

    async fn authorize(
        &self,
        addresses: Vec<FramesResource>,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let watched = addresses
            .into_iter()
            .map(watched)
            .collect::<Result<Vec<_>, _>>()?;
        let identity = gateway_identity(context)?;
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        for watched in watched {
            match watched {
                Watched::Catalog => {}
                Watched::World(world_id) => {
                    if self
                        .state
                        .frames
                        .get_world(&scope, &world_id)
                        .await
                        .map_err(|error| McpError::internal_error(error.to_string(), None))?
                        .is_none()
                    {
                        return Err(McpError::resource_not_found("unknown frame world", None));
                    }
                }
                Watched::TaskUsage(task_id) => {
                    if !self
                        .state
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

    fn hub(&self) -> &SubscriptionHub {
        &self.state.subscriptions
    }
}

pub(super) fn spawn_observer(
    state: Arc<AppState>,
    cancellation: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut changes = state.tasks.platform_store().resource_changes(vec![
            veoveo_modules::ObservationTable::from(
                veoveo_frames_mcp::FramesObservationTable::FrameWorld,
            ),
            veoveo_modules::ObservationTable::from(
                veoveo_frames_mcp::FramesObservationTable::FrameWorldRevision,
            ),
            veoveo_modules::ObservationTable::from(PlatformTable::DomainUsage),
            veoveo_modules::ObservationTable::from(PlatformTable::Task),
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
    use veoveo_frames_mcp::{
        contract::{FrameTaskUsageUri, FrameUsageIndexUri, FrameWorldsUri},
        uris,
    };
    use veoveo_mcp_contract::hosting::requested_addresses;

    fn subscribe(uri: &str) -> Result<Watched, McpError> {
        let mut addresses = requested_addresses::<FramesResource>(Some(&[uri.to_owned()]))?;
        watched(addresses.remove(0))
    }

    #[test]
    fn subscriptions_admit_mutable_worlds_and_usage() {
        assert_eq!(subscribe(FrameWorldsUri::ROOT).unwrap(), Watched::Catalog);
        let cursor = veoveo_frames_mcp::contract::FrameWorldCursor::new(
            &FrameWorldId::new("world").unwrap(),
        );
        assert_eq!(
            subscribe(FrameWorldsUri::new(Some(&cursor)).as_str()).unwrap(),
            Watched::Catalog
        );
        assert_eq!(
            subscribe(FrameUsageIndexUri::ROOT).unwrap(),
            Watched::Catalog
        );
        assert_eq!(
            subscribe("frames://world/fixture").unwrap(),
            Watched::World(FrameWorldId::new("fixture").unwrap())
        );
        let task_id = TaskId::new();
        let usage = FrameTaskUsageUri::new(task_id).unwrap();
        assert_eq!(
            subscribe(usage.as_str()).unwrap(),
            Watched::TaskUsage(task_id)
        );
        let cursor = veoveo_frames_mcp::contract::FrameUsageCursor::new(task_id).unwrap();
        assert_eq!(
            subscribe(FrameUsageIndexUri::new(Some(&cursor)).as_str()).unwrap(),
            Watched::Catalog
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
            assert_eq!(
                subscribe(uri).unwrap_err().code,
                rmcp::model::ErrorCode::INVALID_PARAMS,
                "{uri}"
            );
        }
    }
}
