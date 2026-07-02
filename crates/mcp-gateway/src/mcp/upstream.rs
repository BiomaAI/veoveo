use rmcp::{
    ClientHandler,
    model::{
        ClientInfo, Implementation, Notification, ServerNotification, TaskStatusNotificationParam,
    },
    service::{NotificationContext, Peer, RoleClient, RoleServer},
};
use veoveo_mcp_contract::{GatewayProfileId, PrincipalId, ServerSlug, UpstreamTaskId};

use crate::{
    GatewayState,
    mcp_support::{project_upstream_resource_for_owner, task_mapping_allows_principal},
};

#[derive(Debug, Clone)]
pub(super) struct GatewayUpstreamHandler {
    profile_id: GatewayProfileId,
    principal_id: PrincipalId,
    upstream_server: ServerSlug,
    state: GatewayState,
    downstream: Peer<RoleServer>,
}

impl GatewayUpstreamHandler {
    pub(super) fn new(
        profile_id: GatewayProfileId,
        principal_id: PrincipalId,
        upstream_server: ServerSlug,
        state: GatewayState,
        downstream: Peer<RoleServer>,
    ) -> Self {
        Self {
            profile_id,
            principal_id,
            upstream_server,
            state,
            downstream,
        }
    }
}

impl ClientHandler for GatewayUpstreamHandler {
    fn get_info(&self) -> ClientInfo {
        let mut info = ClientInfo::default();
        info.client_info =
            Implementation::new("veoveo-gateway-upstream", env!("CARGO_PKG_VERSION"));
        info
    }

    async fn on_progress(
        &self,
        params: rmcp::model::ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        if let Err(err) = self.downstream.notify_progress(params).await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward progress notification: {err}"
            );
        }
    }

    async fn on_resource_updated(
        &self,
        mut params: rmcp::model::ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        match project_upstream_resource_for_owner(
            &self.state,
            &self.profile_id,
            &self.principal_id,
            &self.upstream_server,
            &params.uri,
        ) {
            Ok(Some(projection)) => {
                params.uri = projection.gateway_uri.to_string();
            }
            Ok(None) => {
                tracing::warn!(
                    upstream_server = %self.upstream_server,
                    upstream_uri = %params.uri,
                    "dropped resource update notification for unmapped upstream resource"
                );
                return;
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
        if let Err(err) = self.downstream.notify_resource_updated(params).await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward resource update notification: {err}"
            );
        }
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<RoleClient>) {
        if let Err(err) = self.downstream.notify_resource_list_changed().await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward resource list notification: {err}"
            );
        }
    }

    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        if let Err(err) = self.downstream.notify_tool_list_changed().await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward tool list notification: {err}"
            );
        }
    }

    async fn on_prompt_list_changed(&self, _context: NotificationContext<RoleClient>) {
        if let Err(err) = self.downstream.notify_prompt_list_changed().await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward prompt list notification: {err}"
            );
        }
    }

    async fn on_task_status(
        &self,
        mut params: TaskStatusNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let Ok(upstream_task_id) = UpstreamTaskId::new(params.task.task_id.clone()) else {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "dropped task status notification with invalid upstream task id"
            );
            return;
        };
        let mapping = match self
            .state
            .task_mapping_by_upstream(&self.upstream_server, &upstream_task_id)
        {
            Ok(Some(mapping)) => mapping,
            Ok(None) => {
                tracing::warn!(
                    upstream_server = %self.upstream_server,
                    upstream_task_id = %upstream_task_id,
                    "dropped task status notification for unknown gateway task mapping"
                );
                return;
            }
            Err(err) => {
                tracing::warn!(
                    upstream_server = %self.upstream_server,
                    "failed to read gateway task mapping for notification: {err}"
                );
                return;
            }
        };
        if !task_mapping_allows_principal(&self.profile_id, &mapping, &self.principal_id) {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                upstream_task_id = %upstream_task_id,
                "dropped task status notification for another gateway principal"
            );
            return;
        }
        params.task.task_id = mapping.gateway_task_id.to_string();
        let notification = ServerNotification::TaskStatusNotification(Notification::new(params));
        if let Err(err) = self.downstream.send_notification(notification).await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward task status notification: {err}"
            );
        }
    }
}
