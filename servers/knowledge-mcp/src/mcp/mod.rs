//! Authenticated hosted MCP methods. The HTTP adapter inserts verified identities.
mod completion;
mod resources;
mod setup;
mod subscriptions;
mod tools;
use crate::{
    authority::{RequestAuthority, authorize},
    contract::KnowledgeScope,
    embed::Embeddings,
};
use rmcp::{
    ErrorData, RoleServer, handler::server::router::tool::ToolRouter, model::*,
    service::RequestContext,
};
pub use setup::{KnowledgeContract, SETUP};
use std::sync::Arc;
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, PolicyTarget,
    hosting::{
        DomainAddress, DomainRead, DomainServer, Listing, SubscriptionListener, gateway_identity,
    },
    server_contract::McpServerSetup,
};
use veoveo_platform_store::PlatformStore;

pub struct KnowledgeMcp<E> {
    pub(crate) store: PlatformStore,
    pub(crate) catalog_registry: veoveo_gateway_contract::CatalogRegistry,
    pub(crate) embeddings: Arc<E>,
    changes: tokio::sync::watch::Sender<Option<veoveo_platform_store::ResourceInvalidation>>,
    tool_router: Arc<ToolRouter<Self>>,
}
impl<E> Clone for KnowledgeMcp<E> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            catalog_registry: self.catalog_registry.clone(),
            embeddings: self.embeddings.clone(),
            changes: self.changes.clone(),
            tool_router: self.tool_router.clone(),
        }
    }
}
impl<E: Embeddings + 'static> KnowledgeMcp<E> {
    pub fn new(
        store: PlatformStore,
        embeddings: Arc<E>,
        catalog_registry: veoveo_gateway_contract::CatalogRegistry,
    ) -> Self {
        Self {
            store,
            catalog_registry,
            embeddings,
            changes: tokio::sync::watch::channel(None).0,
            tool_router: Arc::new(Self::declared_tool_router()),
        }
    }
    async fn authority(
        &self,
        context: &RequestContext<RoleServer>,
        scope: KnowledgeScope,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> Result<(GatewayInternalIdentity, RequestAuthority), ErrorData> {
        let identity = gateway_identity(context)?;
        let authority = authorize(
            &self.store,
            &self.catalog_registry,
            &identity,
            scope,
            action,
            target,
        )
        .await
        .map_err(error)?;
        Ok((identity, authority))
    }
}
impl<E: Embeddings + 'static> DomainServer for KnowledgeMcp<E> {
    type Contract = KnowledgeContract;

    fn setup() -> &'static McpServerSetup<KnowledgeContract> {
        &SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    /// Tools this caller's scopes and current gateway policy admit. Policy can
    /// change at any time, so the list is never reused.
    async fn list_tools(
        &self,
        tools: Vec<Tool>,
        context: &RequestContext<RoleServer>,
    ) -> Result<Listing<Tool>, ErrorData> {
        let identity = gateway_identity(context)?;
        let scope = if SETUP.has_scope(&identity.actor.scopes, KnowledgeScope::Search) {
            KnowledgeScope::Search
        } else {
            KnowledgeScope::Embed
        };
        let authority =
            crate::authority::authenticate(&self.store, &self.catalog_registry, &identity, scope)
                .await
                .map_err(error)?;
        let tools = tools
            .into_iter()
            .filter(|tool| {
                tools::scope(&tool.name)
                    .is_some_and(|required| SETUP.has_scope(&identity.actor.scopes, required))
                    && authority.allows(
                        &identity,
                        GatewayAction::ToolsList,
                        &PolicyTarget::Tool {
                            server: "knowledge".parse().unwrap(),
                            tool: tool.name.as_ref().parse().expect("declared tool"),
                        },
                    )
            })
            .collect();
        Ok(Listing::all(tools).no_store())
    }

    /// Resources current gateway policy admits for this caller; never reused.
    async fn list_resources(
        &self,
        declared: Vec<Resource>,
        _cursor: Option<&str>,
        context: &RequestContext<RoleServer>,
    ) -> Result<Listing<Resource>, ErrorData> {
        let identity = gateway_identity(context)?;
        let authority = crate::authority::authenticate(
            &self.store,
            &self.catalog_registry,
            &identity,
            KnowledgeScope::Read,
        )
        .await
        .map_err(error)?;
        let resources = declared
            .into_iter()
            .filter(|resource| {
                veoveo_types::ResourceUri::new(&resource.uri).is_ok_and(|uri| {
                    authority.allows(
                        &identity,
                        GatewayAction::ResourcesList,
                        &PolicyTarget::Resource {
                            server: "knowledge".parse().unwrap(),
                            uri,
                        },
                    )
                })
            })
            .collect();
        Ok(Listing::all(resources).no_store())
    }

    /// Templates current gateway policy admits for this caller; never reused.
    async fn list_resource_templates(
        &self,
        declared: Vec<ResourceTemplate>,
        context: &RequestContext<RoleServer>,
    ) -> Result<Listing<ResourceTemplate>, ErrorData> {
        let identity = gateway_identity(context)?;
        let authority = crate::authority::authenticate(
            &self.store,
            &self.catalog_registry,
            &identity,
            KnowledgeScope::Read,
        )
        .await
        .map_err(error)?;
        let templates = declared
            .into_iter()
            .filter(|template| {
                veoveo_types::ResourceTemplateUri::new(&template.uri_template).is_ok_and(|uri| {
                    authority.allows(
                        &identity,
                        GatewayAction::ResourcesTemplatesList,
                        &PolicyTarget::ResourceTemplate {
                            server: "knowledge".parse().unwrap(),
                            uri,
                        },
                    )
                })
            })
            .collect();
        Ok(Listing::all(templates).no_store())
    }

    /// Documents follow the same current policy as every Knowledge read.
    async fn authorize_documents(
        &self,
        identity: &GatewayInternalIdentity,
        address: &veoveo_types::ResourceUri,
    ) -> Result<(), ErrorData> {
        authorize(
            &self.store,
            &self.catalog_registry,
            identity,
            KnowledgeScope::Read,
            GatewayAction::ResourcesRead,
            &PolicyTarget::Resource {
                server: "knowledge".parse().unwrap(),
                uri: address.clone(),
            },
        )
        .await
        .map(|_| ())
        .map_err(error)
    }

    async fn read(
        &self,
        address: DomainAddress<KnowledgeContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, ErrorData> {
        tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.read(address, &request.uri, context),
        )
        .await
        .map_err(|_| error(crate::ServiceError::Deadline))?
        .map(DomainRead::no_store)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, ErrorData> {
        tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.completion(request, context),
        )
        .await
        .map_err(|_| error(crate::ServiceError::Deadline))?
    }
}

/// Knowledge subscriptions follow the catalog observer and re-authorize each
/// snapshot.
pub struct KnowledgeListener<E> {
    pub(crate) server: KnowledgeMcp<E>,
}

impl<E: Embeddings + 'static> SubscriptionListener for KnowledgeListener<E> {
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        let mut accepted = SubscriptionFilter::builder();
        if let Some(resources) = &requested.resource_subscriptions {
            accepted = accepted.resource_subscriptions(resources.clone());
        }
        if requested.resources_list_changed == Some(true) {
            accepted = accepted.resources_list_changed();
        }
        Some(accepted.build())
    }

    async fn listen(&self, context: rmcp::service::SubscriptionContext) -> Result<(), ErrorData> {
        self.server.listen_catalog(context).await
    }
}

fn error(error: crate::ServiceError) -> ErrorData {
    match error {
        crate::ServiceError::AccessChanged => {
            ErrorData::invalid_request("Knowledge access changed or is not permitted.", None)
        }
        crate::ServiceError::Contract(_) | crate::ServiceError::EmbeddingInput(_) => {
            ErrorData::invalid_params(error.to_string(), None)
        }
        _ => ErrorData::internal_error(error.to_string(), None),
    }
}
