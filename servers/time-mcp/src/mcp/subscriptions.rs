//! Time resource subscriptions.

use std::sync::Arc;

use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{SubscriptionHub, hosting::ResourceSubscriptions};

use super::{internal, require_scope};
use crate::{
    contract::{TimeResource, TimeScope},
    state::TimeApplication,
};

/// Time's mutable resources. Subscribing to the event index restores the
/// caller's event watchers, and subscribing to one event schedules it, so
/// notifications start without a separate call.
pub(crate) struct TimeSubscriptions {
    state: Arc<TimeApplication>,
}

impl TimeSubscriptions {
    pub(crate) fn new(state: Arc<TimeApplication>) -> Self {
        Self { state }
    }
}

impl ResourceSubscriptions for TimeSubscriptions {
    type Address = TimeResource;

    async fn authorize(
        &self,
        addresses: Vec<TimeResource>,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        if let Some(immutable) = addresses.iter().find(|address| !address.is_subscribable()) {
            tracing::debug!(?immutable, "rejected Time subscription");
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        let identity = require_scope(context, TimeScope::Read)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        for address in addresses {
            match address {
                TimeResource::Events { cursor: None } => {
                    self.state
                        .restore_event_watchers(&scope)
                        .await
                        .map_err(internal)?;
                }
                TimeResource::Event(event_id) => {
                    if let Some(event) = self
                        .state
                        .catalog
                        .event(&scope, &event_id)
                        .await
                        .map_err(internal)?
                    {
                        self.state
                            .schedule_event(scope.clone(), event)
                            .await
                            .map_err(internal)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn hub(&self) -> &SubscriptionHub {
        self.state.subscriptions.as_ref()
    }
}
