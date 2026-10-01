//! Checked MCP setup from domain-owned contracts. No domain registry lives here.
use std::{collections::BTreeSet, fmt, marker::PhantomData};

use rmcp::model::{Resource, ResourceTemplate, ServerConfig};
use veoveo_types::{
    ResourceAddress, ResourceScheme, ResourceTemplateUri, ResourceUriBuilder, ScopeDefinition,
    ScopeName, UriAuthority, UriSegment,
};

use crate::{
    ServerSlug,
    docs::{DOC_ID_AGENTS, DOC_ID_DESIGN, ServerDocs},
};

/// MCP associations implemented in a server's MCP feature, above its pure contract.
/// An empty scope vocabulary is valid. This trait establishes API structure, not
/// authentication, domain authorization, recovery, or behavioral compliance.
pub trait McpServerContract {
    type Scope: ScopeDefinition + 'static;
    type Resource: ResourceAddress + Eq;

    fn slug() -> ServerSlug;
    fn scheme() -> ResourceScheme;
    fn scopes() -> &'static [Self::Scope];
    fn documents() -> &'static ServerDocs;
    fn server_config() -> ServerConfig;
    fn resources() -> Result<Vec<McpResource<Self::Resource>>, McpSetupError>;
    /// Fixed protocol declarations. Owners qualify template expansion against
    /// their typed parser; the generic policy matcher is not an RFC 6570 engine.
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError>;
}

/// One protocol descriptor bound to its domain address. The builder can reuse
/// ordinary RMCP and App-extension metadata helpers but cannot change the URI.
/// ```compile_fail
/// use veoveo_mcp_contract::server_contract::McpResource;
/// use rmcp::model::Resource;
/// McpResource::new("example://items", |uri| Resource::new(uri, "items"));
/// ```
#[derive(Debug, Clone)]
pub struct McpResource<A> {
    address: A,
    descriptor: Resource,
}

impl<A: ResourceAddress + Eq> McpResource<A> {
    pub fn new(address: A, describe: impl FnOnce(&str) -> Resource) -> Result<Self, McpSetupError> {
        let uri = address
            .to_uri()
            .map_err(|_| McpSetupError::InvalidResource)?;
        uri.components()
            .map_err(|_| McpSetupError::InvalidResource)?;
        if A::parse(&uri).map_err(|_| McpSetupError::InvalidResource)? != address {
            return Err(McpSetupError::ResourceRoundTrip);
        }
        let descriptor = describe(uri.as_str());
        if descriptor.uri != uri.as_str() {
            return Err(McpSetupError::DescriptorAddressMismatch);
        }
        if descriptor.name.trim().is_empty() {
            return Err(McpSetupError::MissingResourceName);
        }
        Ok(Self {
            address,
            descriptor,
        })
    }

    pub fn address(&self) -> &A {
        &self.address
    }

    pub fn descriptor(&self) -> &Resource {
        &self.descriptor
    }
}

/// A descriptor whose template has passed RFC 6570 admission.
/// ```compile_fail
/// use veoveo_mcp_contract::server_contract::McpResourceTemplate;
/// use veoveo_types::ResourceUri;
/// use rmcp::model::ResourceTemplate;
/// McpResourceTemplate::new(ResourceUri::new("example://items/{id}").unwrap(),
///     |uri| ResourceTemplate::new(uri, "Items"));
/// ```
#[derive(Debug, Clone)]
pub struct McpResourceTemplate {
    template: ResourceTemplateUri,
    descriptor: ResourceTemplate,
}

impl McpResourceTemplate {
    pub fn new(
        template: ResourceTemplateUri,
        describe: impl FnOnce(&str) -> ResourceTemplate,
    ) -> Result<Self, McpSetupError> {
        let descriptor = describe(template.as_str());
        if descriptor.uri_template != template.as_str() {
            return Err(McpSetupError::DescriptorTemplateMismatch);
        }
        if descriptor.name.trim().is_empty() {
            return Err(McpSetupError::InvalidTemplate);
        }
        Ok(Self {
            template,
            descriptor,
        })
    }
    pub fn template(&self) -> &ResourceTemplateUri {
        &self.template
    }
    pub fn descriptor(&self) -> &ResourceTemplate {
        &self.descriptor
    }
}

/// A validated setup consumed by hosted handlers for initialization and discovery.
/// Construction requires typed resources and verifies the well-known documents.
/// Resource reads and tool handlers still enforce the caller's current policy.
pub struct McpServerSetup<C: McpServerContract> {
    info: ServerConfig,
    resources: Vec<McpResource<C::Resource>>,
    templates: Vec<McpResourceTemplate>,
    scopes: BTreeSet<ScopeName>,
    documents: &'static ServerDocs,
    contract: PhantomData<fn() -> C>,
}

impl<C: McpServerContract> McpServerSetup<C> {
    pub fn new() -> Result<Self, McpSetupError> {
        let slug = C::slug();
        let scheme = C::scheme();
        let mut info = C::server_config();
        let documents = C::documents();
        if info.server_info.name != slug.as_str() || documents.server() != slug.as_str() {
            return Err(McpSetupError::IdentityMismatch);
        }
        if info.capabilities.resources.is_none() {
            return Err(McpSetupError::MissingResourcesCapability);
        }
        let mut scopes = BTreeSet::new();
        for scope in C::scopes() {
            if !scopes.insert(scope.name().clone()) {
                return Err(McpSetupError::DuplicateScope);
            }
        }
        let mut resources = C::resources()?;
        resources.sort_by(|a, b| a.descriptor.uri.cmp(&b.descriptor.uri));
        let mut addresses = BTreeSet::new();
        for resource in &resources {
            if !addresses.insert(resource.descriptor.uri.as_str()) {
                return Err(McpSetupError::DuplicateResource);
            }
        }
        for id in [DOC_ID_AGENTS, DOC_ID_DESIGN] {
            if documents
                .doc(id)
                .is_none_or(|doc| doc.body.trim().is_empty())
            {
                return Err(McpSetupError::MissingDocument);
            }
        }
        for root in ["docs", "contract"] {
            let uri = well_known_builder(&scheme, root)?
                .build()
                .map_err(|_| McpSetupError::InvalidDocument)?;
            if !addresses.contains(uri.as_str()) {
                return Err(McpSetupError::MissingWellKnownResource);
            }
        }
        let mut document_ids = BTreeSet::new();
        for doc in documents.iter() {
            if doc.body.trim().is_empty() {
                return Err(McpSetupError::MissingDocument);
            }
            if !document_ids.insert(doc.id) {
                return Err(McpSetupError::InvalidDocument);
            }
            let uri = well_known_builder(&scheme, "docs")?
                .segment(UriSegment::new(doc.id).map_err(|_| McpSetupError::InvalidDocument)?)
                .build()
                .map_err(|_| McpSetupError::InvalidDocument)?;
            if !addresses.contains(uri.as_str()) {
                return Err(McpSetupError::MissingWellKnownResource);
            }
        }
        let mut templates = C::resource_templates()?;
        let docs_template = crate::docs::knowledge_extension::docs::member_template(&scheme);
        let mut found_docs = false;
        for template in &mut templates {
            if template.template == docs_template {
                documents
                    .knowledge_template(&scheme, &mut template.descriptor)
                    .map_err(|_| McpSetupError::InvalidTemplate)?;
                found_docs = true;
            }
        }
        if !found_docs {
            return Err(McpSetupError::MissingDocumentTemplate);
        }
        documents.declare_knowledge(&mut info.capabilities);
        templates.sort_by(|a, b| a.template.cmp(&b.template));
        let mut declared_templates = BTreeSet::new();
        for template in &templates {
            if !declared_templates.insert(template.template.as_str()) {
                return Err(McpSetupError::DuplicateTemplate);
            }
        }
        Ok(Self {
            info,
            resources,
            templates,
            scopes,
            documents,
            contract: PhantomData,
        })
    }

    pub fn server_config(&self) -> &ServerConfig {
        &self.info
    }
    pub fn resources(&self) -> &[McpResource<C::Resource>] {
        &self.resources
    }
    pub fn resource_templates(&self) -> &[McpResourceTemplate] {
        &self.templates
    }
    pub fn documents(&self) -> &'static ServerDocs {
        self.documents
    }

    pub fn read_documents(
        &self,
        request: &rmcp::model::ReadResourceRequestParams,
        context: &rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<Option<rmcp::model::ReadResourceResponse>, rmcp::ErrorData> {
        self.documents
            .read_knowledge(&C::scheme(), request, context)
    }
    pub fn scope_names(&self) -> &BTreeSet<ScopeName> {
        &self.scopes
    }

    /// Compares a domain scope with an already authenticated grant set. This
    /// membership test does not authenticate the set or authorize a resource.
    /// ```compile_fail
    /// use veoveo_mcp_contract::server_contract::{McpServerContract, McpServerSetup};
    /// use std::collections::BTreeSet;
    /// fn check<C: McpServerContract>(setup: &McpServerSetup<C>) {
    ///     setup.has_scope(&BTreeSet::new(), "example:read");
    /// }
    /// ```
    /// A different server's enum cannot be used as this server's permission.
    /// ```compile_fail
    /// use veoveo_mcp_contract::server_contract::{McpServerContract, McpServerSetup};
    /// use std::collections::BTreeSet;
    /// veoveo_types::scope_enum! { enum ForeignScope { Read => "foreign:read" } }
    /// fn check<C: McpServerContract>(setup: &McpServerSetup<C>) {
    ///     setup.has_scope(&BTreeSet::new(), ForeignScope::Read);
    /// }
    /// ```
    pub fn has_scope(&self, grants: &BTreeSet<ScopeName>, required: C::Scope) -> bool {
        self.scopes.contains(required.name()) && grants.contains(required.name())
    }
}

fn well_known_builder(
    scheme: &ResourceScheme,
    authority: &str,
) -> Result<ResourceUriBuilder, McpSetupError> {
    ResourceUriBuilder::from_components(
        scheme,
        UriAuthority::new(authority).map_err(|_| McpSetupError::InvalidDocument)?,
    )
    .map_err(|_| McpSetupError::InvalidDocument)
}

/// Errors describe setup invariants without copying resource identities or values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpSetupError {
    IdentityMismatch,
    MissingResourcesCapability,
    InvalidResource,
    ResourceRoundTrip,
    DescriptorAddressMismatch,
    MissingResourceName,
    DuplicateResource,
    DuplicateScope,
    MissingDocument,
    InvalidDocument,
    MissingWellKnownResource,
    InvalidTemplate,
    DescriptorTemplateMismatch,
    DuplicateTemplate,
    MissingDocumentTemplate,
}

impl fmt::Display for McpSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::IdentityMismatch => {
                "server slug, implementation name and document owner must agree"
            }
            Self::MissingResourcesCapability => "hosted server setup requires resource capability",
            Self::InvalidResource => "resource address must build and parse as a concrete URI",
            Self::ResourceRoundTrip => {
                "resource address does not round-trip through its owning parser"
            }
            Self::DescriptorAddressMismatch => {
                "resource metadata builder changed the typed address"
            }
            Self::MissingResourceName => "resource descriptor requires a nonblank name",
            Self::DuplicateResource => "resource discovery contains a duplicate address",
            Self::DuplicateScope => "scope vocabulary contains duplicate names",
            Self::MissingDocument => {
                "hosted server requires agent and design documents and nonempty document bodies"
            }
            Self::InvalidDocument => "document identifiers must be unique valid URI path segments",
            Self::MissingWellKnownResource => {
                "resource discovery must include docs, contract, and every embedded document"
            }
            Self::InvalidTemplate => {
                "resource template requires valid absolute RFC 6570 syntax and a nonblank name"
            }
            Self::DescriptorTemplateMismatch => {
                "resource metadata builder changed the typed template"
            }
            Self::DuplicateTemplate => "resource discovery contains a duplicate template",
            Self::MissingDocumentTemplate => "resource discovery requires the docs member template",
        })
    }
}
impl std::error::Error for McpSetupError {}
