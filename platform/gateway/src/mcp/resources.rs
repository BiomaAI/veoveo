use super::http_response::RequestError;
use rmcp::{
    model::{
        ErrorData as McpError, ListResourceTemplatesResult, ListResourcesResult,
        PaginatedRequestParams, ReadResourceResult, ResourceContents, ResourceTemplate,
        TaskPayload,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_gateway_contract::{GatewayDiscoveryFailure, GatewayDiscoverySurface};
use veoveo_mcp_contract::{
    GATEWAY_TASK_RESOURCE_TEMPLATE, GatewayAction, GatewayDiscoveryDegradation, GatewayTaskStatus,
    GatewayTaskStatusDocument,
};

use crate::mcp_support::{
    mcp_internal, mcp_invalid_params, project_app_resource_dependencies,
    project_app_tool_dependencies, project_listed_resource, project_listed_resource_uri,
    project_resource_template_uri, resource_policy_target, resource_template_policy_target,
};

use super::tools::{project_detailed_task_resource_uris, rewrite_detailed_task_id};
use super::{
    GatewayMcp,
    discovery::{AdmittedCatalog, DiscoveredResource, DiscoveryCacheKey},
};

impl GatewayMcp {
    pub(super) async fn handle_list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let subject = self.authenticated(&context)?;
        let (resources, degradation, denied) =
            self.available_resources(context, subject.clone()).await?;
        self.record_discovery(
            &subject,
            veoveo_audit_contract::DiscoveryKind::Resources,
            resources
                .iter()
                .map(|resource| resource.uri.clone())
                .collect(),
            denied,
        )
        .await?;
        let page = super::catalog_pages::page(resources, request.as_ref())?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: degradation.into_meta(),
        })
    }

    /// Return the per-server results already known for this caller and start each
    /// missing discovery independently. A non-responsive server therefore never
    /// participates in the latency of this list response. Successful background
    /// completions publish a matching list-change event through the shared cache.
    pub(super) async fn available_resources(
        &self,
        context: RequestContext<RoleServer>,
        subject: crate::AuthenticatedSubject,
    ) -> Result<(Vec<rmcp::model::Resource>, GatewayDiscoveryDegradation, u32), McpError> {
        let profile_server_list = self.profile_servers();
        let profile_servers = profile_server_list
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let snapshot = self.catalog.snapshot();
        let catalog = snapshot.catalog().clone();
        let catalog_generation = snapshot.generation();
        let authorization_fingerprint = super::discovery_authorization_fingerprint(&subject)?;
        let mut keys = Vec::new();
        let mut cached_at_start = std::collections::BTreeMap::new();
        for server_slug in profile_server_list {
            let key = DiscoveryCacheKey {
                catalog_generation,
                principal: subject.actor.id.clone(),
                authorization_fingerprint,
                server: server_slug.clone(),
            };
            keys.push(key.clone());
            if self.discovery_watch_active(&key).await
                && let Some(items) = self.discovery.resources(&key).await
            {
                cached_at_start.insert(key, items);
                continue;
            }
            let Some(fetch) = self
                .discovery
                .begin(GatewayDiscoverySurface::Resources, key.clone())
                .await
            else {
                continue;
            };
            let gateway = self.clone();
            let catalog = catalog.clone();
            let profile_servers = profile_servers.clone();
            let context = context.clone();
            let subject = subject.clone();
            tokio::spawn(async move {
                let result = async {
                    gateway
                        .ensure_discovery_watch(&key, context.peer.clone(), &subject)
                        .await?;
                    gateway
                        .discover_resources_for_server(
                            &catalog,
                            &profile_servers,
                            &server_slug,
                            &context,
                            &subject,
                        )
                        .await
                }
                .await;
                match result {
                    Ok(discovered) => {
                        gateway
                            .discovery
                            .finish_resource_routes(
                                fetch.with_denied(discovered.denied),
                                discovered.items,
                            )
                            .await;
                    }
                    Err(error) => {
                        gateway
                            .discovery
                            .finish_failure(GatewayDiscoverySurface::Resources, fetch)
                            .await;
                        tracing::warn!(
                            server = %server_slug,
                            %error,
                            "isolated upstream resource discovery failure"
                        );
                    }
                }
            });
        }
        self.discovery
            .settle(GatewayDiscoverySurface::Resources, &keys)
            .await;
        let mut resources = Vec::new();
        let mut failures = Vec::new();
        let mut denied = 0u32;
        for key in keys {
            let cached = match cached_at_start.remove(&key) {
                Some(items) => Some(items),
                None if self.discovery_watch_active(&key).await => {
                    self.discovery.resources(&key).await
                }
                None => None,
            };
            if let Some(mut cached) = cached {
                denied = denied.saturating_add(cached.denied);
                resources.append(&mut cached.items);
            } else {
                let code = self
                    .discovery
                    .missing_code(GatewayDiscoverySurface::Resources, &key)
                    .await;
                failures.push(GatewayDiscoveryFailure {
                    server: key.server,
                    surface: GatewayDiscoverySurface::Resources,
                    code,
                });
            }
        }
        Ok((
            resources,
            GatewayDiscoveryDegradation::new(failures),
            denied,
        ))
    }

    pub(super) async fn discover_resources_for_server(
        &self,
        catalog: &crate::GatewayCatalog,
        profile_servers: &std::collections::BTreeSet<veoveo_mcp_contract::ServerSlug>,
        server_slug: &veoveo_mcp_contract::ServerSlug,
        context: &RequestContext<RoleServer>,
        subject: &crate::AuthenticatedSubject,
    ) -> Result<AdmittedCatalog<DiscoveredResource>, McpError> {
        let started = std::time::Instant::now();
        let manifest = catalog
            .server(server_slug)
            .ok_or_else(|| mcp_internal(format!("unknown profile server `{server_slug}`")))?;
        let upstream_resources = self
            .idempotent_upstream_request(
                server_slug,
                context.peer.clone(),
                subject,
                |upstream| async move { upstream.list_all_resources().await },
            )
            .await
            .map_err(RequestError::into_protocol)?;
        let upstream_ms = started.elapsed().as_millis();
        let mut resources = Vec::with_capacity(upstream_resources.len());
        let mut targets = Vec::with_capacity(upstream_resources.len());
        for mut resource in upstream_resources {
            let projection = self.project_upstream_resource(server_slug, &resource.uri)?;
            project_listed_resource_uri(manifest, &mut resource)?;
            project_listed_resource(&mut resource, &projection);
            project_app_resource_dependencies(
                manifest,
                &mut resource,
                profile_servers,
                &subject.actor.scopes,
                &subject.actor.data_labels,
            )?;
            project_app_tool_dependencies(
                manifest,
                &mut resource,
                profile_servers,
                &subject.actor.scopes,
                &subject.actor.data_labels,
            )?;
            targets.push(resource_policy_target(projection.server, &resource.uri)?);
            resources.push(DiscoveredResource {
                resource,
                upstream_uri: projection.upstream_uri,
                listed: false,
            });
        }
        let authorization_started = std::time::Instant::now();
        let allowed = self
            .allows_catalog_targets(context, GatewayAction::ResourcesList, targets)
            .await?;
        if started.elapsed() >= std::time::Duration::from_millis(100) {
            tracing::info!(server = %server_slug, surface = "resources",
                count = resources.len(), upstream_ms,
                authorization_ms = authorization_started.elapsed().as_millis(),
                total_ms = started.elapsed().as_millis(), "slow MCP catalog discovery");
        }
        let denied = allowed
            .iter()
            .filter(|allowed| !**allowed)
            .count()
            .try_into()
            .map_err(|_| mcp_internal("catalog exceeds audit count range"))?;
        Ok(AdmittedCatalog {
            denied,
            items: resources
                .into_iter()
                .zip(allowed)
                .map(|(mut resource, allowed)| {
                    resource.listed = allowed;
                    resource
                })
                .collect(),
        })
    }

    pub(super) async fn handle_list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let subject = self.authenticated(&context)?;
        let (mut templates, degradation, denied) = self
            .available_resource_templates(context, subject.clone())
            .await?;
        if self.client_allows_task_projection(&subject).await? {
            templates.push(
                ResourceTemplate::new(GATEWAY_TASK_RESOURCE_TEMPLATE, "task status")
                    .with_title("Gateway task status")
                    .with_description(
                        "Current status and terminal result for one authorized canonical task.",
                    )
                    .with_mime_type("application/json"),
            );
        }
        self.record_discovery(
            &subject,
            veoveo_audit_contract::DiscoveryKind::ResourceTemplates,
            templates
                .iter()
                .map(|template| template.uri_template.clone())
                .collect(),
            denied,
        )
        .await?;
        let page = super::catalog_pages::page(templates, request.as_ref())?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: degradation.into_meta(),
        })
    }

    async fn available_resource_templates(
        &self,
        context: RequestContext<RoleServer>,
        subject: crate::AuthenticatedSubject,
    ) -> Result<(Vec<ResourceTemplate>, GatewayDiscoveryDegradation, u32), McpError> {
        let snapshot = self.catalog.snapshot();
        let catalog = snapshot.catalog().clone();
        let catalog_generation = snapshot.generation();
        let authorization_fingerprint = super::discovery_authorization_fingerprint(&subject)?;
        let mut keys = Vec::new();
        let mut cached_at_start = std::collections::BTreeMap::new();
        for server_slug in self.profile_servers() {
            let key = DiscoveryCacheKey {
                catalog_generation,
                principal: subject.actor.id.clone(),
                authorization_fingerprint,
                server: server_slug.clone(),
            };
            keys.push(key.clone());
            if self.discovery_watch_active(&key).await
                && let Some(items) = self.discovery.resource_templates(&key).await
            {
                cached_at_start.insert(key, items);
                continue;
            }
            let Some(fetch) = self
                .discovery
                .begin(GatewayDiscoverySurface::ResourceTemplates, key.clone())
                .await
            else {
                continue;
            };
            let gateway = self.clone();
            let catalog = catalog.clone();
            let context = context.clone();
            let subject = subject.clone();
            tokio::spawn(async move {
                let result = async {
                    gateway
                        .ensure_discovery_watch(&key, context.peer.clone(), &subject)
                        .await?;
                    gateway
                        .discover_resource_templates_for_server(
                            &catalog,
                            &server_slug,
                            &context,
                            &subject,
                        )
                        .await
                }
                .await;
                match result {
                    Ok(discovered) => {
                        gateway
                            .discovery
                            .finish_resource_templates(
                                fetch.with_denied(discovered.denied),
                                discovered.items,
                            )
                            .await;
                    }
                    Err(error) => {
                        gateway
                            .discovery
                            .finish_failure(GatewayDiscoverySurface::ResourceTemplates, fetch)
                            .await;
                        tracing::warn!(
                            server = %server_slug,
                            %error,
                            "isolated upstream resource-template discovery failure"
                        );
                    }
                }
            });
        }
        self.discovery
            .settle(GatewayDiscoverySurface::ResourceTemplates, &keys)
            .await;
        let mut templates = Vec::new();
        let mut failures = Vec::new();
        let mut denied = 0u32;
        for key in keys {
            let cached = match cached_at_start.remove(&key) {
                Some(items) => Some(items),
                None if self.discovery_watch_active(&key).await => {
                    self.discovery.resource_templates(&key).await
                }
                None => None,
            };
            if let Some(mut cached) = cached {
                denied = denied.saturating_add(cached.denied);
                templates.append(&mut cached.items);
            } else {
                let code = self
                    .discovery
                    .missing_code(GatewayDiscoverySurface::ResourceTemplates, &key)
                    .await;
                failures.push(GatewayDiscoveryFailure {
                    server: key.server,
                    surface: GatewayDiscoverySurface::ResourceTemplates,
                    code,
                });
            }
        }
        Ok((
            templates,
            GatewayDiscoveryDegradation::new(failures),
            denied,
        ))
    }

    async fn discover_resource_templates_for_server(
        &self,
        catalog: &crate::GatewayCatalog,
        server_slug: &veoveo_mcp_contract::ServerSlug,
        context: &RequestContext<RoleServer>,
        subject: &crate::AuthenticatedSubject,
    ) -> Result<AdmittedCatalog<ResourceTemplate>, McpError> {
        let manifest = catalog
            .server(server_slug)
            .ok_or_else(|| mcp_internal(format!("unknown profile server `{server_slug}`")))?;
        let (upstream_templates, knowledge_source) = self
            .idempotent_upstream_request(
                server_slug,
                context.peer.clone(),
                subject,
                |upstream| async move {
                    let declared = upstream.peer_info().is_some_and(|info| {
                        veoveo_mcp_knowledge_extension::client::supports(&info.capabilities)
                    });
                    Ok((upstream.list_all_resource_templates().await?, declared))
                },
            )
            .await
            .map_err(RequestError::into_protocol)?;
        let mut templates = Vec::with_capacity(upstream_templates.len());
        let mut targets = Vec::with_capacity(upstream_templates.len());
        for mut template in upstream_templates {
            if let Some(descriptor) = veoveo_mcp_knowledge_extension::client::collection(&template)
                .map_err(|_| mcp_internal("invalid upstream knowledge declaration"))?
            {
                let root = veoveo_mcp_knowledge_extension::enumeration_uri(&descriptor, None)
                    .map_err(|_| mcp_internal("invalid upstream knowledge enumeration"))?;
                if !knowledge_source
                    || descriptor.collection().server() != server_slug
                    || root
                        .components()
                        .map_err(|_| mcp_internal("invalid knowledge resource scheme"))?
                        .scheme()
                        != manifest.uri_scheme.as_str()
                {
                    return Err(mcp_internal(
                        "upstream knowledge declaration does not match its discovered owner",
                    ));
                }
            }
            project_resource_template_uri(manifest, &mut template)?;
            targets.push(resource_template_policy_target(
                server_slug.clone(),
                &template.uri_template,
            )?);
            templates.push(template);
        }
        let allowed = self
            .allows_catalog_targets(context, GatewayAction::ResourcesTemplatesList, targets)
            .await?;
        let denied = allowed
            .iter()
            .filter(|allowed| !**allowed)
            .count()
            .try_into()
            .map_err(|_| mcp_internal("catalog exceeds audit count range"))?;
        Ok(AdmittedCatalog {
            denied,
            items: templates
                .into_iter()
                .zip(allowed)
                .filter_map(|(template, allowed)| allowed.then_some(template))
                .collect(),
        })
    }

    pub(super) async fn read_task_status_resource(
        &self,
        task_id: &str,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, RequestError> {
        let subject = self.authenticated(context)?;
        if !self.client_allows_task_projection(&subject).await? {
            return Err(mcp_invalid_params(format!("resource URI is not exposed: {uri}")).into());
        }
        let route = self
            .authorize_canonical_task_for_subject(&subject, GatewayAction::TasksGet, task_id)
            .await?;
        let upstream = self
            .upstream_with_tasks(&route.server, context.peer.clone(), &route.subject, true)
            .await?;
        let mut detailed = upstream
            .peer
            .get_task(rmcp::model::GetTaskParams::new(route.task_id))
            .await
            .map_err(RequestError::from)?
            .task;
        let catalog = self.catalog.current();
        let manifest = catalog
            .server(&route.server)
            .ok_or_else(|| mcp_internal(format!("unknown task server `{}`", route.server)))?;
        project_detailed_task_resource_uris(manifest, &mut detailed)?;
        rewrite_detailed_task_id(&mut detailed, task_id);
        let status = GatewayTaskStatus::from_task(&detailed.task)
            .map_err(|error| mcp_internal(format!("failed to project task status: {error}")))?;
        let result = match detailed.payload {
            TaskPayload::Completed { result } => Some(serde_json::Value::Object(result)),
            _ => None,
        };
        let document = GatewayTaskStatusDocument {
            task: status,
            result,
        };
        let text = serde_json::to_string(&document)
            .map_err(|error| mcp_internal(format!("failed to encode task status: {error}")))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, uri.to_owned()).with_mime_type("application/json"),
        ]))
    }
}
