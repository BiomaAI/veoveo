//! Discovery results generated from the checked setup.
//!
//! Every hosted server lists its resources, templates, and tools the same way:
//! authenticated, complete and private. A [`Listing`] chooses who pages the list
//! and whether a client may reuse it for
//! [`PRIVATE_CATALOG_TTL_MS`](crate::PRIVATE_CATALOG_TTL_MS).

use rmcp::{
    ErrorData, RoleServer,
    model::{
        CacheScope, CompleteRequestParams, CompleteResult, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        Prompt, Reference, Resource, ResourceTemplate, ResultType, Tool,
    },
    service::RequestContext,
};

use veoveo_types::{ResourceUri, ResourceUriParts};

use crate::{
    PRIVATE_CATALOG_TTL_MS, paginate,
    server_contract::{McpServerContract, McpServerSetup},
};

use super::{
    auth::gateway_identity,
    listing::{CATALOG_PAGE_SIZE, Listing},
};

fn page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<crate::Page<T>, ErrorData> {
    paginate(items, request, CATALOG_PAGE_SIZE)
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))
}

impl<C: McpServerContract> McpServerSetup<C> {
    /// The checked resource declarations, as `resources/list` descriptors.
    pub fn declared_resources(&self) -> Vec<Resource> {
        self.resources()
            .iter()
            .map(|resource| resource.descriptor().clone())
            .collect()
    }

    /// The checked template declarations, as `resources/templates/list`
    /// descriptors.
    pub fn declared_resource_templates(&self) -> Vec<ResourceTemplate> {
        self.resource_templates()
            .iter()
            .map(|template| template.descriptor().clone())
            .collect()
    }

    /// `resources/list` for `listing`, sorted by URI when the host pages it.
    pub fn list_resources(
        &self,
        listing: Listing<Resource>,
        request: Option<&PaginatedRequestParams>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let page = listing.serve(request, |resource| resource.uri.clone())?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(page.ttl_ms),
            cache_scope: Some(page.cache_scope),
            meta: None,
        })
    }

    /// `resources/templates/list` for `listing`, sorted by template.
    pub fn list_resource_templates(
        &self,
        listing: Listing<ResourceTemplate>,
        request: Option<&PaginatedRequestParams>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let page = listing.serve(request, |template| template.uri_template.clone())?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(page.ttl_ms),
            cache_scope: Some(page.cache_scope),
            meta: None,
        })
    }

    /// `tools/list` for `listing`, sorted by name.
    pub fn list_tools(
        &self,
        listing: Listing<Tool>,
        request: Option<&PaginatedRequestParams>,
    ) -> Result<ListToolsResult, ErrorData> {
        let page = listing.serve(request, |tool| tool.name.clone())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(page.ttl_ms),
            cache_scope: Some(page.cache_scope),
            meta: None,
        })
    }

    /// `prompts/list` for the server's prompts, sorted by name.
    pub fn list_prompts(
        &self,
        mut prompts: Vec<Prompt>,
        request: Option<&PaginatedRequestParams>,
        context: &RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        gateway_identity(context)?;
        prompts.sort_by(|left, right| left.name.cmp(&right.name));
        let page = page(prompts, request)?;
        Ok(ListPromptsResult {
            prompts: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }

    /// The address of a well-known document read: `{scheme}://contract` or any
    /// `{scheme}://docs` route. `None` for every domain address.
    pub fn document_address(&self, uri: &str) -> Option<ResourceUri> {
        let parts = ResourceUriParts::parse(uri).ok()?;
        (parts.scheme() == C::scheme().as_str() && matches!(parts.authority(), "docs" | "contract"))
            .then(|| ResourceUri::new(uri).ok())
            .flatten()
    }

    /// Whether `request` completes `doc_id` on the `{scheme}://docs/{doc_id}`
    /// template, which the host answers from the embedded documents.
    pub fn is_document_completion(&self, request: &CompleteRequestParams) -> bool {
        let Reference::Resource(reference) = &request.r#ref else {
            return false;
        };
        let template = crate::docs::knowledge_extension::docs::member_template(&C::scheme());
        reference.uri == template.as_str() && request.argument.name == "doc_id"
    }

    /// Completes `doc_id` on the `{scheme}://docs/{doc_id}` template from the
    /// embedded documents. Returns `None` for every other completion request.
    pub fn complete_documents(
        &self,
        request: &CompleteRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<Option<CompleteResult>, ErrorData> {
        if !self.is_document_completion(request) {
            return Ok(None);
        }
        gateway_identity(context)?;
        let ids = self.documents().iter().map(|doc| doc.id);
        super::results::completion(super::results::rank_completions(
            ids,
            &request.argument.value,
        ))
        .map(Some)
    }
}
