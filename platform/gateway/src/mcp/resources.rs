use futures::{StreamExt, stream};
use rmcp::{
    model::{
        ErrorData as McpError, ListResourceTemplatesResult, ListResourcesResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
        ResourceTemplate, TaskPayload,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{
    GATEWAY_TASK_RESOURCE_TEMPLATE, GatewayAction, GatewayDiscoverySurface,
    GatewayResourceProjection, GatewayTaskStatus, GatewayTaskStatusDocument, paginate,
    parse_gateway_task_resource_uri,
};

use crate::mcp_support::{
    mcp_internal, mcp_invalid_params, project_app_resource_dependencies,
    project_gateway_resource_uri_for_upstream, project_listed_resource,
    project_listed_resource_uri, project_read_resource_result, project_resource_template_uri,
    resource_read_action, upstream_error,
};

use super::tools::{project_detailed_task_resource_uris, rewrite_detailed_task_id};
use super::{
    GATEWAY_PAGE_SIZE, GatewayMcp,
    discovery::{DiscoveryCacheKey, MAX_CONCURRENT_DISCOVERY, isolate_discovery_failures},
    invocation_authorization_fingerprint,
};

impl GatewayMcp {
    pub(super) async fn handle_list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let subject = self.authenticated(&context)?;
        let profile_server_list = self.profile_servers();
        let profile_servers = profile_server_list.iter().cloned().collect();
        let snapshot = self.catalog.snapshot();
        let catalog = snapshot.catalog().clone();
        let catalog_generation = snapshot.generation();
        let authorization_fingerprint =
            invocation_authorization_fingerprint(&subject.actor, &subject.authority)?;
        let results = stream::iter(profile_server_list.into_iter().map(|server_slug| {
            let catalog = catalog.clone();
            let profile_servers = &profile_servers;
            let context = &context;
            let subject = &subject;
            async move {
                let key = DiscoveryCacheKey {
                    catalog_generation,
                    principal: subject.actor.id.clone(),
                    authorization_fingerprint,
                    server: server_slug.clone(),
                };
                if let Some(resources) = self.discovery.resources(&key).await {
                    return (server_slug, Ok::<_, McpError>(resources));
                }
                let result = async {
                    let manifest = catalog.server(&server_slug).ok_or_else(|| {
                        mcp_internal(format!("unknown profile server `{server_slug}`"))
                    })?;
                    let upstream_resources = self
                        .idempotent_upstream_request(
                            &server_slug,
                            context.peer.clone(),
                            subject,
                            |upstream| async move { upstream.list_all_resources().await },
                        )
                        .await?;
                    let mut resources = Vec::new();
                    for mut resource in upstream_resources {
                        let projection =
                            self.project_upstream_resource(&server_slug, &resource.uri)?;
                        project_listed_resource_uri(manifest, &mut resource)?;
                        project_listed_resource(&mut resource, &projection);
                        project_app_resource_dependencies(
                            manifest,
                            &mut resource,
                            profile_servers,
                            &subject.actor.scopes,
                            &subject.actor.data_labels,
                        )?;
                        if !self
                            .allows_resource(
                                context,
                                GatewayAction::ResourcesList,
                                projection.server.clone(),
                                &resource.uri,
                            )
                            .await?
                        {
                            continue;
                        }
                        resources.push(resource);
                    }
                    self.discovery.store_resources(key, resources.clone()).await;
                    Ok(resources)
                }
                .await;
                (server_slug, result)
            }
        }))
        .buffer_unordered(MAX_CONCURRENT_DISCOVERY)
        .collect::<Vec<_>>()
        .await;
        let (mut resources, degradation, errors) =
            isolate_discovery_failures(GatewayDiscoverySurface::Resources, results);
        for (server, error) in errors {
            tracing::warn!(%server, %error, "isolated upstream resource discovery failure");
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        let page = paginate(resources, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: degradation.into_meta(),
        })
    }

    pub(super) async fn handle_list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let subject = self.authenticated(&context)?;
        let snapshot = self.catalog.snapshot();
        let catalog = snapshot.catalog().clone();
        let catalog_generation = snapshot.generation();
        let authorization_fingerprint =
            invocation_authorization_fingerprint(&subject.actor, &subject.authority)?;
        let results = stream::iter(self.profile_servers().into_iter().map(|server_slug| {
            let catalog = catalog.clone();
            let context = &context;
            let subject = &subject;
            async move {
                let key = DiscoveryCacheKey {
                    catalog_generation,
                    principal: subject.actor.id.clone(),
                    authorization_fingerprint,
                    server: server_slug.clone(),
                };
                if let Some(templates) = self.discovery.resource_templates(&key).await {
                    return (server_slug, Ok::<_, McpError>(templates));
                }
                let result = async {
                    let manifest = catalog.server(&server_slug).ok_or_else(|| {
                        mcp_internal(format!("unknown profile server `{server_slug}`"))
                    })?;
                    let upstream_templates = self
                        .idempotent_upstream_request(
                            &server_slug,
                            context.peer.clone(),
                            subject,
                            |upstream| async move { upstream.list_all_resource_templates().await },
                        )
                        .await?;
                    let mut templates = Vec::new();
                    for mut template in upstream_templates {
                        project_resource_template_uri(manifest, &mut template)?;
                        if !self
                            .allows_resource(
                                context,
                                GatewayAction::ResourcesTemplatesList,
                                server_slug.clone(),
                                &template.uri_template,
                            )
                            .await?
                        {
                            continue;
                        }
                        templates.push(template);
                    }
                    self.discovery
                        .store_resource_templates(key, templates.clone())
                        .await;
                    Ok(templates)
                }
                .await;
                (server_slug, result)
            }
        }))
        .buffer_unordered(MAX_CONCURRENT_DISCOVERY)
        .collect::<Vec<_>>()
        .await;
        let (mut templates, degradation, errors) =
            isolate_discovery_failures(GatewayDiscoverySurface::ResourceTemplates, results);
        for (server, error) in errors {
            tracing::warn!(%server, %error, "isolated upstream resource-template discovery failure");
        }
        if self.client_allows_task_projection(&subject)? {
            templates.push(
                ResourceTemplate::new(GATEWAY_TASK_RESOURCE_TEMPLATE, "task status")
                    .with_title("Gateway task status")
                    .with_description(
                        "Current status and terminal result for one authorized canonical task.",
                    )
                    .with_mime_type("application/json"),
            );
        }
        templates.sort_by(|left, right| left.uri_template.cmp(&right.uri_template));
        let page = paginate(templates, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: degradation.into_meta(),
        })
    }

    pub(super) async fn handle_read_resource(
        &self,
        mut request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if let Some(task_id) = parse_gateway_task_resource_uri(&request.uri) {
            return self
                .read_task_status_resource(task_id, &request.uri, &context)
                .await;
        }
        let server = self.server_for_resource(&request.uri)?;
        let projection = self.project_resource_for_upstream(&request.uri)?;
        let subject = self
            .authorize_projected_resource(&context, resource_read_action(&request.uri), &projection)
            .await?;
        let catalog = self.catalog.current();
        let manifest = catalog
            .server(&server)
            .ok_or_else(|| mcp_internal(format!("unknown resource server `{server}`")))?;
        let upstream_resources = self
            .idempotent_upstream_request(
                &server,
                context.peer.clone(),
                &subject,
                |upstream| async move { upstream.list_all_resources().await },
            )
            .await?;
        let Some(upstream_uri) =
            project_gateway_resource_uri_for_upstream(manifest, &request.uri, &upstream_resources)?
        else {
            return Err(mcp_invalid_params(format!(
                "resource URI is not exposed: {}",
                request.uri
            )));
        };
        let projection = GatewayResourceProjection {
            server,
            gateway_uri: projection.gateway_uri,
            upstream_uri,
        };
        request.uri = projection.upstream_uri.to_string();
        let mut result = self
            .idempotent_upstream_request(
                &projection.server,
                context.peer.clone(),
                &subject,
                |upstream| {
                    let request = request.clone();
                    async move { upstream.read_resource(request).await }
                },
            )
            .await?;
        project_read_resource_result(&mut result, &projection)?;
        Ok(result)
    }

    async fn read_task_status_resource(
        &self,
        task_id: &str,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let subject = self.authenticated(context)?;
        if !self.client_allows_task_projection(&subject)? {
            return Err(mcp_invalid_params(format!(
                "resource URI is not exposed: {uri}"
            )));
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
            .map_err(upstream_error)?
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
