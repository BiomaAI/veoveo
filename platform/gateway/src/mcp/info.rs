use rmcp::{
    handler::server::ServerHandler,
    model::{
        ErrorData as McpError, ExtensionCapabilities, Implementation, InitializeRequestParams,
        InitializeResult, JsonObject, ServerCapabilities, ServerInfo, TasksCapability,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::ServerSlug;

use super::GatewayMcp;

impl GatewayMcp {
    pub(super) fn profile_servers(&self) -> Vec<ServerSlug> {
        self.catalog
            .current()
            .profile_servers(&self.profile_id)
            .into_iter()
            .map(|(_, server)| server.slug.clone())
            .collect()
    }

    fn auth_extension_capabilities(&self) -> Option<ExtensionCapabilities> {
        let catalog = self.catalog.current();
        let profile = catalog.profile(&self.profile_id)?;
        let extensions: ExtensionCapabilities = profile
            .auth_modes
            .iter()
            .filter_map(|mode| mode.mcp_extension_id())
            .map(|extension| (extension.to_string(), JsonObject::new()))
            .collect();
        if extensions.is_empty() {
            None
        } else {
            Some(extensions)
        }
    }

    pub(super) fn handle_get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::default();
        let catalog = self.catalog.current();
        for (_, server) in catalog.profile_servers(&self.profile_id) {
            if server.capabilities.tools {
                capabilities.tools.get_or_insert_default();
            }
            if server.capabilities.prompts {
                capabilities.prompts.get_or_insert_default();
            }
            if server.capabilities.resources || server.capabilities.resource_templates {
                let resources = capabilities.resources.get_or_insert_default();
                if server.capabilities.resource_subscriptions {
                    resources.subscribe = Some(true);
                }
                if server.capabilities.notifications {
                    resources.list_changed = Some(true);
                }
            }
            if server.capabilities.completions {
                capabilities.completions.get_or_insert_with(JsonObject::new);
            }
        }
        let mut extensions = self.auth_extension_capabilities();
        if catalog
            .profile_servers(&self.profile_id)
            .iter()
            .any(|(_, server)| server.capabilities.apps)
        {
            let (id, declaration) = veoveo_mcp_apps_extension::host_extension_capability();
            extensions.get_or_insert_default().insert(id, declaration);
        }
        capabilities.extensions = extensions;

        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = Implementation::new("veoveo", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Veoveo MCP profile for hosted servers. Tool names are namespaced at this MCP surface; resource URIs remain server-owned."
                .to_string(),
        );
        info
    }

    pub(super) async fn handle_initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        let subject = self.authenticated(&context)?;
        context.peer.set_peer_info(request);
        let mut info = self.get_info();
        if self.client_allows_task_projection(&subject)?
            && self
                .catalog
                .current()
                .profile_servers(&self.profile_id)
                .into_iter()
                .any(|(exposure, server)| {
                    exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled
                        && server.capabilities.tasks
                })
        {
            let mut tasks = TasksCapability::server_default();
            tasks.list = None;
            info.capabilities.tasks = Some(tasks);
        }
        Ok(info)
    }
}
