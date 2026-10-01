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
use rmcp::{ErrorData, RoleServer, ServerHandler, model::*, service::RequestContext};
pub use setup::{KnowledgeContract, SETUP};
use std::sync::Arc;
use veoveo_mcp_contract::{GatewayAction, GatewayInternalIdentity, PolicyTarget};
use veoveo_platform_store::PlatformStore;

pub struct KnowledgeMcp<E> {
    pub(crate) store: PlatformStore,
    pub(crate) embeddings: Arc<E>,
    changes: tokio::sync::watch::Sender<Option<veoveo_platform_store::ResourceInvalidation>>,
}
impl<E> Clone for KnowledgeMcp<E> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            embeddings: self.embeddings.clone(),
            changes: self.changes.clone(),
        }
    }
}
impl<E: Embeddings + 'static> KnowledgeMcp<E> {
    pub fn new(store: PlatformStore, embeddings: Arc<E>) -> Self {
        Self {
            store,
            embeddings,
            changes: tokio::sync::watch::channel(None).0,
        }
    }
    async fn authority(
        &self,
        context: &RequestContext<RoleServer>,
        scope: KnowledgeScope,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> Result<(GatewayInternalIdentity, RequestAuthority), ErrorData> {
        let identity = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<GatewayInternalIdentity>())
            .cloned()
            .ok_or_else(|| {
                ErrorData::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
            })?;
        let authority = authorize(&self.store, &identity, scope, action, target)
            .await
            .map_err(error)?;
        Ok((identity, authority))
    }
}
impl<E: Embeddings + 'static> ServerHandler for KnowledgeMcp<E> {
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
        self.listen_catalog(context).await
    }
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }
    fn get_info(&self) -> ServerConfig {
        SETUP.server_config().clone()
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let identity = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<GatewayInternalIdentity>())
            .ok_or_else(|| {
                ErrorData::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
            })?;
        let scope = if SETUP.has_scope(&identity.actor.scopes, KnowledgeScope::Search) {
            KnowledgeScope::Search
        } else {
            KnowledgeScope::Embed
        };
        let authority = crate::authority::authenticate(&self.store, identity, scope)
            .await
            .map_err(error)?;
        let tools = tools::definitions()
            .into_iter()
            .filter(|tool| {
                SETUP.has_scope(
                    &identity.actor.scopes,
                    tools::scope(&tool.name).expect("declared tool"),
                ) && authority.allows(
                    identity,
                    GatewayAction::ToolsList,
                    &PolicyTarget::Tool {
                        server: "knowledge".parse().unwrap(),
                        tool: tool.name.as_ref().parse().expect("declared tool"),
                    },
                )
            })
            .collect();
        let page = page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(0),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.call(request, context).await.map(Into::into)
    }
    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let identity = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<GatewayInternalIdentity>())
            .ok_or_else(|| {
                ErrorData::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
            })?;
        let authority = crate::authority::authenticate(&self.store, identity, KnowledgeScope::Read)
            .await
            .map_err(error)?;
        let page = page(
            SETUP
                .resources()
                .iter()
                .filter(|r| {
                    veoveo_types::ResourceUri::new(&r.descriptor().uri).is_ok_and(|uri| {
                        authority.allows(
                            identity,
                            GatewayAction::ResourcesList,
                            &PolicyTarget::Resource {
                                server: "knowledge".parse().unwrap(),
                                uri,
                            },
                        )
                    })
                })
                .map(|r| r.descriptor().clone())
                .collect(),
            request.as_ref(),
        )?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(0),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }
    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let identity = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<GatewayInternalIdentity>())
            .ok_or_else(|| {
                ErrorData::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
            })?;
        let authority = crate::authority::authenticate(&self.store, identity, KnowledgeScope::Read)
            .await
            .map_err(error)?;
        let page = page(
            SETUP
                .resource_templates()
                .iter()
                .filter(|r| {
                    veoveo_types::ResourceTemplateUri::new(&r.descriptor().uri_template).is_ok_and(
                        |uri| {
                            authority.allows(
                                identity,
                                GatewayAction::ResourcesTemplatesList,
                                &PolicyTarget::ResourceTemplate {
                                    server: "knowledge".parse().unwrap(),
                                    uri,
                                },
                            )
                        },
                    )
                })
                .map(|r| r.descriptor().clone())
                .collect(),
            request.as_ref(),
        )?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(0),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.read(request, context),
        )
        .await
        .map_err(|_| error(crate::ServiceError::Deadline))?
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

fn page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<veoveo_mcp_contract::pagination::Page<T>, ErrorData> {
    veoveo_mcp_contract::pagination::paginate(items, request, 100)
        .map_err(|_| ErrorData::invalid_params("invalid Knowledge catalog cursor", None))
}
