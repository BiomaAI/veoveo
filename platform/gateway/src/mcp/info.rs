use rmcp::{
    handler::server::ServerHandler,
    model::{
        ErrorData as McpError, ExtensionCapabilities, Implementation, InitializeRequestParams,
        InitializeResult, JsonObject, ServerCapabilities, ServerInfo, TasksCapability,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{McpSurfaceCapabilities, ServerSlug};

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
            merge_surface_capabilities(&mut capabilities, server.capabilities);
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

fn merge_surface_capabilities(
    capabilities: &mut ServerCapabilities,
    surface: McpSurfaceCapabilities,
) {
    if surface.tools {
        let tools = capabilities.tools.get_or_insert_default();
        if surface.tools_list_changed {
            tools.list_changed = Some(true);
        }
    }
    if surface.prompts {
        let prompts = capabilities.prompts.get_or_insert_default();
        if surface.prompts_list_changed {
            prompts.list_changed = Some(true);
        }
    }
    if surface.resources || surface.resource_templates {
        let resources = capabilities.resources.get_or_insert_default();
        if surface.resource_subscriptions {
            resources.subscribe = Some(true);
        }
        if surface.resources_list_changed {
            resources.list_changed = Some(true);
        }
    }
    if surface.completions {
        capabilities.completions.get_or_insert_with(JsonObject::new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregated_capabilities_advertise_forwarded_list_change_notifications() {
        let mut capabilities = ServerCapabilities::default();
        merge_surface_capabilities(
            &mut capabilities,
            McpSurfaceCapabilities {
                tools: true,
                resources: true,
                apps: false,
                resource_templates: true,
                resource_subscriptions: true,
                prompts: true,
                completions: true,
                tasks: false,
                tools_list_changed: true,
                prompts_list_changed: true,
                resources_list_changed: true,
            },
        );

        assert_eq!(
            capabilities
                .tools
                .as_ref()
                .and_then(|tools| tools.list_changed),
            Some(true)
        );
        assert_eq!(
            capabilities
                .prompts
                .as_ref()
                .and_then(|prompts| prompts.list_changed),
            Some(true)
        );
        assert_eq!(
            capabilities
                .resources
                .as_ref()
                .and_then(|resources| resources.list_changed),
            Some(true)
        );
        assert_eq!(
            capabilities
                .resources
                .as_ref()
                .and_then(|resources| resources.subscribe),
            Some(true)
        );
        assert!(capabilities.completions.is_some());
    }

    #[test]
    fn static_surfaces_do_not_claim_list_change_notifications() {
        let mut capabilities = ServerCapabilities::default();
        merge_surface_capabilities(
            &mut capabilities,
            McpSurfaceCapabilities {
                tools: true,
                resources: true,
                apps: false,
                resource_templates: false,
                resource_subscriptions: false,
                prompts: true,
                completions: false,
                tasks: false,
                tools_list_changed: false,
                prompts_list_changed: false,
                resources_list_changed: false,
            },
        );

        assert_eq!(
            capabilities
                .tools
                .as_ref()
                .and_then(|tools| tools.list_changed),
            None
        );
        assert_eq!(
            capabilities
                .prompts
                .as_ref()
                .and_then(|prompts| prompts.list_changed),
            None
        );
        assert_eq!(
            capabilities
                .resources
                .as_ref()
                .and_then(|resources| resources.list_changed),
            None
        );
    }
}
