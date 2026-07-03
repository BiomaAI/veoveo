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
            if server.capabilities.tasks {
                capabilities
                    .tasks
                    .get_or_insert_with(TasksCapability::server_default);
            }
        }
        capabilities.extensions = self.auth_extension_capabilities();

        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = Implementation::new("veoveo-gateway", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Gateway profile for hosted Veoveo MCP servers. Tool names are gateway namespaced; resource URIs remain server-owned."
                .to_string(),
        );
        info
    }

    pub(super) async fn handle_initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        self.authenticated(&context)?;
        context.peer.set_peer_info(request);
        Ok(self.get_info())
    }
}
