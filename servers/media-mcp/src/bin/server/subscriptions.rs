use rmcp::{ErrorData as McpError, model::SubscriptionFilter};
use veoveo_media_mcp::{contract::MediaSubscriptionResource, reads::MediaReads};
use veoveo_task_runtime::{TaskOwner, TaskRuntime};

pub(super) async fn authorize(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    filter: &SubscriptionFilter,
) -> Result<(), McpError> {
    let reads = MediaReads::new(tasks).map_err(internal)?;
    for uri in filter.resource_subscriptions.iter().flatten() {
        let resource = MediaSubscriptionResource::parse(uri)
            .ok_or_else(|| McpError::invalid_params("resource is not subscribable", None))?;
        let allowed = match resource {
            MediaSubscriptionResource::UsageIndex(_)
            | MediaSubscriptionResource::PredictionsIndex(_) => true,
            MediaSubscriptionResource::Prediction(uri) => reads
                .prediction(owner, &uri)
                .await
                .map_err(internal)?
                .is_some(),
            MediaSubscriptionResource::TaskUsage(uri) => reads
                .task_visible(owner, uri.task_id())
                .await
                .map_err(internal)?,
        };
        if !allowed {
            return Err(McpError::invalid_params("resource is not available", None));
        }
    }
    Ok(())
}
fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}
