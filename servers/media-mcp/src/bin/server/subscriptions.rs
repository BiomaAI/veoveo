use rmcp::ErrorData as McpError;
use veoveo_media_mcp::{
    contract::{MediaResource, MediaSubscriptionResource},
    reads::MediaReads,
};
use veoveo_task_runtime::{TaskOwner, TaskRuntime};

/// Authorizes subscriptions to typed Media addresses for one task owner.
pub(super) async fn authorize(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    addresses: Vec<MediaResource>,
) -> Result<(), McpError> {
    let reads = MediaReads::new(tasks).map_err(internal)?;
    for address in addresses {
        let resource = MediaSubscriptionResource::from_resource(address)
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
