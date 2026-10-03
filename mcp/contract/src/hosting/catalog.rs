//! Discovery results generated from the checked setup.
//!
//! Every hosted server lists its resources, templates, and tools the same way:
//! authenticated, sorted, in pages of [`CATALOG_PAGE_SIZE`], complete, private,
//! and cached for [`PRIVATE_CATALOG_TTL_MS`](crate::PRIVATE_CATALOG_TTL_MS).

use rmcp::{
    ErrorData, RoleServer,
    model::{
        CacheScope, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, ResultType, Tool,
    },
    service::RequestContext,
};

use crate::{
    PRIVATE_CATALOG_TTL_MS, paginate,
    server_contract::{McpServerContract, McpServerSetup},
};

use super::auth::gateway_identity;

/// Items per discovery page.
pub const CATALOG_PAGE_SIZE: usize = 100;

fn page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<crate::Page<T>, ErrorData> {
    paginate(items, request, CATALOG_PAGE_SIZE)
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))
}

impl<C: McpServerContract> McpServerSetup<C> {
    /// `resources/list` from the checked resource declarations.
    pub fn list_resources(
        &self,
        request: Option<&PaginatedRequestParams>,
        context: &RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        gateway_identity(context)?;
        let resources = self
            .resources()
            .iter()
            .map(|resource| resource.descriptor().clone())
            .collect();
        let page = page(resources, request)?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }

    /// `resources/templates/list` from the checked template declarations.
    pub fn list_resource_templates(
        &self,
        request: Option<&PaginatedRequestParams>,
        context: &RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        gateway_identity(context)?;
        let templates = self
            .resource_templates()
            .iter()
            .map(|template| template.descriptor().clone())
            .collect();
        let page = page(templates, request)?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }

    /// `tools/list` for the server's tools, sorted by name.
    pub fn list_tools(
        &self,
        mut tools: Vec<Tool>,
        request: Option<&PaginatedRequestParams>,
        context: &RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        gateway_identity(context)?;
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = page(tools, request)?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }
}
