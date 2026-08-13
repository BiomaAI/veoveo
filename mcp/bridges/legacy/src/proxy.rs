use std::borrow::Cow;

use rmcp::{
    handler::server::ServerHandler,
    model::{
        CacheScope, CallToolRequestParams, CallToolResponse, CompleteRequestParams, CompleteResult,
        ErrorData as McpError, GetPromptRequestParams, GetPromptResponse, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse, ServerCapabilities,
        ServerInfo, ServerPeerInfo,
    },
    service::{Peer, RequestContext, RoleClient, RoleServer, ServiceError},
};

const PRIVATE_CATALOG_TTL_MS: u64 = veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS;
const PRIVATE_RESOURCE_TTL_MS: u64 = veoveo_mcp_contract::PRIVATE_RESOURCE_TTL_MS;

#[derive(Clone)]
pub(crate) struct LegacyProxy {
    legacy: Peer<RoleClient>,
    info: ServerInfo,
}

impl LegacyProxy {
    pub(crate) fn new(legacy: Peer<RoleClient>, info: ServerInfo) -> Self {
        Self { legacy, info }
    }
}

pub(crate) fn final_server_info(legacy: &ServerPeerInfo) -> ServerInfo {
    let observed = &legacy.capabilities;
    let mut capabilities = ServerCapabilities::default();
    capabilities.tools = observed.tools.as_ref().map(|_| Default::default());
    capabilities.resources = observed.resources.as_ref().map(|_| Default::default());
    capabilities.prompts = observed.prompts.as_ref().map(|_| Default::default());
    capabilities.completions = observed.completions.as_ref().map(|_| Default::default());
    let mut info = ServerInfo::new(capabilities);
    info.server_info = legacy
        .server_info
        .clone()
        .unwrap_or_else(|| rmcp::model::Implementation::new("legacy-external", "unknown"));
    info.instructions = legacy.instructions.clone();
    info
}

fn legacy_error(error: ServiceError) -> McpError {
    match error {
        ServiceError::McpError(error) if error.code.0 == -32002 => {
            McpError::invalid_params(error.message, error.data)
        }
        ServiceError::McpError(error) => error,
        other => McpError::internal_error(format!("legacy MCP request failed: {other}"), None),
    }
}

fn private_catalog<T>(result: &mut T)
where
    T: CatalogResult,
{
    result.set_cache(PRIVATE_CATALOG_TTL_MS, CacheScope::Private);
}

trait CatalogResult {
    fn set_cache(&mut self, ttl_ms: u64, scope: CacheScope);
}

macro_rules! catalog_result {
    ($($ty:ty),+ $(,)?) => {$(
        impl CatalogResult for $ty {
            fn set_cache(&mut self, ttl_ms: u64, scope: CacheScope) {
                self.result_type = Some(rmcp::model::ResultType::COMPLETE);
                self.ttl_ms = Some(ttl_ms);
                self.cache_scope = Some(scope);
            }
        }
    )+};
}

catalog_result!(
    ListToolsResult,
    ListPromptsResult,
    ListResourcesResult,
    ListResourceTemplatesResult,
);

impl ServerHandler for LegacyProxy {
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerInfo {
        self.info.clone()
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut result = self
            .legacy
            .list_tools(request)
            .await
            .map_err(legacy_error)?;
        result
            .tools
            .sort_by(|left, right| left.name.cmp(&right.name));
        private_catalog(&mut result);
        Ok(result)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.legacy
            .call_tool(request)
            .await
            .map(Into::into)
            .map_err(legacy_error)
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut result = self
            .legacy
            .list_resources(request)
            .await
            .map_err(legacy_error)?;
        result
            .resources
            .sort_by(|left, right| left.uri.cmp(&right.uri));
        private_catalog(&mut result);
        Ok(result)
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let mut result = self
            .legacy
            .list_resource_templates(request)
            .await
            .map_err(legacy_error)?;
        result
            .resource_templates
            .sort_by(|left, right| left.uri_template.cmp(&right.uri_template));
        private_catalog(&mut result);
        Ok(result)
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let mut result = self
            .legacy
            .read_resource(request)
            .await
            .map_err(legacy_error)?;
        result.result_type = Some(rmcp::model::ResultType::COMPLETE);
        result.ttl_ms = Some(PRIVATE_RESOURCE_TTL_MS);
        result.cache_scope = Some(CacheScope::Private);
        Ok(result.into())
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let mut result = self
            .legacy
            .list_prompts(request)
            .await
            .map_err(legacy_error)?;
        result
            .prompts
            .sort_by(|left, right| left.name.cmp(&right.name));
        private_catalog(&mut result);
        Ok(result)
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        self.legacy
            .get_prompt(request)
            .await
            .map(Into::into)
            .map_err(legacy_error)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        self.legacy.complete(request).await.map_err(legacy_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_capabilities_never_project_deprecated_or_extension_surfaces() {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .build();
        let mut legacy = ServerPeerInfo::new(ProtocolVersion::V_2025_11_25, capabilities);
        legacy.capabilities.logging = Some(Default::default());
        legacy.capabilities.extensions = Some(std::collections::BTreeMap::from([(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            rmcp::model::JsonObject::new(),
        )]));

        let final_info = final_server_info(&legacy);

        assert!(final_info.capabilities.tools.is_some());
        assert!(final_info.capabilities.resources.is_some());
        assert!(final_info.capabilities.logging.is_none());
        assert!(final_info.capabilities.extensions.is_none());
        assert!(
            final_info
                .capabilities
                .resources
                .is_some_and(|resources| !resources.subscribe.unwrap_or(false))
        );
    }
}
