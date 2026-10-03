//! View resource subscriptions.

use std::sync::Arc;

use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{SubscriptionHub, hosting::ResourceSubscriptions};

use super::{not_found, require_scope};
use crate::{
    contract::{ViewResource, ViewScope},
    server::AppState,
    state::ResourceOwner,
};

/// The caller's view, composition and frame collections, and individual views.
/// Layers, tiles, scenes, frames and documents do not change and refuse
/// subscription.
pub(crate) struct ViewSubscriptions {
    state: Arc<AppState>,
}

impl ViewSubscriptions {
    pub(crate) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl ResourceSubscriptions for ViewSubscriptions {
    type Address = ViewResource;

    async fn authorize(
        &self,
        addresses: Vec<ViewResource>,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = require_scope(context, ViewScope::Read)?;
        let owner = ResourceOwner::from_identity(&identity);
        for address in addresses {
            match address {
                ViewResource::Compositions | ViewResource::Views | ViewResource::Frames => {}
                ViewResource::View(view) => {
                    self.state
                        .views
                        .get_view(&owner, view.id())
                        .await
                        .map_err(|_| not_found())?;
                }
                _ => {
                    return Err(McpError::invalid_params(
                        "resource is immutable or not subscribable",
                        None,
                    ));
                }
            }
        }
        Ok(())
    }

    fn hub(&self) -> &SubscriptionHub {
        &self.state.subscriptions
    }
}
