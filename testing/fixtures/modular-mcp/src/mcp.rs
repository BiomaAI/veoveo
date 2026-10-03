use crate::contract::{
    ObservatoryDocument, ObservatoryResource, ObservatoryScope, Reading, ReadingId,
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CompleteRequestParams, CompleteResult, Implementation, ListResourceTemplatesResult,
        ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
};
use std::{collections::BTreeSet, sync::LazyLock};
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    hosting::{completion, rank_completions},
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceTemplateUri, ResourceUri, ScopeName};

pub struct ObservatoryContract;
pub static DOCUMENTS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("observatory"));
pub static SETUP: LazyLock<McpServerSetup<ObservatoryContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("fixture setup"));

impl McpServerContract for ObservatoryContract {
    type Scope = ObservatoryScope;
    type Resource = ObservatoryResource;
    fn slug() -> ServerSlug {
        ServerSlug::new("observatory").unwrap()
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::new("observatory").unwrap()
    }
    fn scopes() -> &'static [ObservatoryScope] {
        ObservatoryScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &DOCUMENTS
    }
    fn server_config() -> ServerConfig {
        let mut info = ServerConfig::default();
        info.server_info = Implementation::new("observatory", env!("CARGO_PKG_VERSION"));
        info.capabilities = ServerCapabilities::builder()
            .enable_resources()
            .enable_completions()
            .build();
        info
    }
    fn resources() -> Result<Vec<McpResource<ObservatoryResource>>, McpSetupError> {
        [
            (ObservatoryResource::Docs, "Documents", "application/json"),
            (
                ObservatoryResource::Document(ObservatoryDocument::Agents),
                "Agent manual",
                "text/markdown",
            ),
            (
                ObservatoryResource::Document(ObservatoryDocument::Design),
                "Domain design",
                "text/markdown",
            ),
            (
                ObservatoryResource::Contract,
                "Contract",
                "application/json",
            ),
            (
                ObservatoryResource::Readings,
                "Readings",
                "application/json",
            ),
        ]
        .into_iter()
        .map(|(address, name, mime)| {
            McpResource::new(address, |uri| Resource::new(uri, name).with_mime_type(mime))
        })
        .collect()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        let template = ResourceTemplateUri::new("observatory://reading/{reading_id}")
            .map_err(|_| McpSetupError::InvalidTemplate)?;
        Ok(vec![
            McpResourceTemplate::new(template, |uri| {
                ResourceTemplate::new(uri, "Reading").with_mime_type("application/json")
            })?,
            McpResourceTemplate::new(
                ResourceTemplateUri::new("observatory://docs/{doc_id}").unwrap(),
                |uri| ResourceTemplate::new(uri, "Document").with_mime_type("text/markdown"),
            )?,
        ])
    }
}

/// Synthetic grants inserted only after the test host authenticates its bearer.
#[derive(Clone)]
pub struct FixtureGrants(pub BTreeSet<ScopeName>);

#[derive(Clone)]
pub struct ObservatoryMcp;

fn authorize(context: &RequestContext<RoleServer>) -> Result<(), ErrorData> {
    let grants = context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<FixtureGrants>())
        .ok_or_else(|| ErrorData::invalid_request("fixture authentication required", None))?;
    if !SETUP.has_scope(&grants.0, ObservatoryScope::Read) {
        return Err(ErrorData::invalid_request(
            "observatory read scope required",
            None,
        ));
    }
    Ok(())
}

impl ServerHandler for ObservatoryMcp {
    fn get_info(&self) -> ServerConfig {
        SETUP.server_config().clone()
    }
    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, ErrorData> {
        authorize(&context)?;
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let template = veoveo_mcp_contract::docs::knowledge_extension::docs::member_template(
            &ObservatoryContract::scheme(),
        );
        if reference.uri != template.as_str() || request.argument.name != "doc_id" {
            return Ok(CompleteResult::default());
        }
        completion(rank_completions(
            SETUP.documents().iter().map(|doc| doc.id),
            &request.argument.value,
        ))
    }
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        authorize(&context)?;
        Ok(ListResourcesResult {
            resources: SETUP
                .resources()
                .iter()
                .map(|value| value.descriptor().clone())
                .collect(),
            next_cursor: None,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }
    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        authorize(&context)?;
        Ok(ListResourceTemplatesResult {
            resource_templates: SETUP
                .resource_templates()
                .iter()
                .map(|template| template.descriptor().clone())
                .collect(),
            next_cursor: None,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        authorize(&context)?;
        if let Some(result) = SETUP.documents().read_authorized_knowledge(
            &veoveo_types::ResourceScheme::new("observatory").unwrap(),
            &request,
            &context.meta,
        )? {
            return Ok(result);
        }
        let uri = ResourceUri::new(request.uri.clone())
            .map_err(|_| ErrorData::invalid_params("invalid resource", None))?;
        let resource = ObservatoryResource::parse(&uri)
            .map_err(|_| ErrorData::resource_not_found("unknown Observatory resource", None))?;
        let (body, mime) = match resource {
            ObservatoryResource::Docs => (
                json(&SETUP.documents().iter().collect::<Vec<_>>())?,
                "application/json",
            ),
            ObservatoryResource::Document(id) => (
                SETUP.documents().doc(id.as_str()).unwrap().body.to_owned(),
                "text/markdown",
            ),
            ObservatoryResource::Contract => (
                json(SETUP.documents().contract_declaration())?,
                "application/json",
            ),
            ObservatoryResource::Readings => (
                json(&vec![ReadingId::new("sensor-a").unwrap()])?,
                "application/json",
            ),
            ObservatoryResource::Reading(id) if id.as_str() == "sensor-a" => {
                (json(&Reading { id, value: 7 })?, "application/json")
            }
            ObservatoryResource::Reading(_) => {
                return Err(ErrorData::resource_not_found("unknown reading", None));
            }
        };
        let result = ReadResourceResult::new(vec![
            ResourceContents::text(body, request.uri).with_mime_type(mime),
        ]);
        Ok(veoveo_mcp_contract::private_resource_response(result, true))
    }
}

fn json<T: serde::Serialize>(value: &T) -> Result<String, ErrorData> {
    serde_json::to_string(value)
        .map_err(|_| ErrorData::internal_error("fixture serialization failed", None))
}
