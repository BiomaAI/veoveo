use std::{fmt::Display, future::Future, time::Duration};

use rmcp::{
    ClientHandler,
    model::{ClientInfo, Implementation},
    service::{NotificationContext, Peer, RoleClient, RoleServer},
};
use veoveo_mcp_contract::{GatewayProfileId, PrincipalId, ServerSlug};

use crate::{GatewayCatalogHandle, mcp_support::project_upstream_resource};

use super::progress::GatewayProgressTokens;

const DOWNSTREAM_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub(super) struct GatewayUpstreamHandler {
    catalog: GatewayCatalogHandle,
    profile_id: GatewayProfileId,
    principal_id: PrincipalId,
    upstream_server: ServerSlug,
    downstream: Peer<RoleServer>,
    progress_tokens: GatewayProgressTokens,
}

impl GatewayUpstreamHandler {
    pub(super) fn new(
        catalog: GatewayCatalogHandle,
        profile_id: GatewayProfileId,
        principal_id: PrincipalId,
        upstream_server: ServerSlug,
        downstream: Peer<RoleServer>,
        progress_tokens: GatewayProgressTokens,
    ) -> Self {
        Self {
            catalog,
            profile_id,
            principal_id,
            upstream_server,
            downstream,
            progress_tokens,
        }
    }
}

impl ClientHandler for GatewayUpstreamHandler {
    fn get_info(&self) -> ClientInfo {
        let mut info = ClientInfo::default();
        info.client_info = Implementation::new("veoveo-internal", env!("CARGO_PKG_VERSION"));
        info
    }

    async fn on_progress(
        &self,
        mut params: rmcp::model::ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let Some(downstream_token) = self
            .progress_tokens
            .translate(
                &self.profile_id,
                &self.principal_id,
                &self.upstream_server,
                &params.progress_token,
            )
            .await
        else {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "dropped progress notification for unknown upstream progress token"
            );
            return;
        };
        params.progress_token = downstream_token;
        let downstream = self.downstream.clone();
        forward_notification(self.upstream_server.clone(), "progress", async move {
            downstream.notify_progress(params).await
        });
    }

    async fn on_resource_updated(
        &self,
        mut params: rmcp::model::ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let catalog = self.catalog.current();
        let Some(manifest) = catalog.server(&self.upstream_server) else {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                upstream_uri = %params.uri,
                "dropped resource update notification for unknown upstream server"
            );
            return;
        };
        match project_upstream_resource(manifest, &params.uri) {
            Ok(projection) => {
                params.uri = projection.gateway_uri.to_string();
            }
            Err(err) => {
                tracing::warn!(
                    upstream_server = %self.upstream_server,
                    upstream_uri = %params.uri,
                    "failed to project resource update notification: {err}"
                );
                return;
            }
        }
        let downstream = self.downstream.clone();
        forward_notification(
            self.upstream_server.clone(),
            "resource update",
            async move { downstream.notify_resource_updated(params).await },
        );
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let downstream = self.downstream.clone();
        forward_notification(self.upstream_server.clone(), "resource list", async move {
            downstream.notify_resource_list_changed().await
        });
    }

    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let downstream = self.downstream.clone();
        forward_notification(self.upstream_server.clone(), "tool list", async move {
            downstream.notify_tool_list_changed().await
        });
    }

    async fn on_prompt_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let downstream = self.downstream.clone();
        forward_notification(self.upstream_server.clone(), "prompt list", async move {
            downstream.notify_prompt_list_changed().await
        });
    }
}

fn forward_notification<F, E>(upstream_server: ServerSlug, notification: &'static str, delivery: F)
where
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Display + Send + 'static,
{
    tokio::spawn(async move {
        match tokio::time::timeout(DOWNSTREAM_NOTIFICATION_TIMEOUT, delivery).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(
                    %upstream_server,
                    notification,
                    %error,
                    "failed to forward MCP notification"
                );
            }
            Err(_) => {
                tracing::warn!(
                    %upstream_server,
                    notification,
                    "timed out forwarding MCP notification"
                );
            }
        }
    });
}
