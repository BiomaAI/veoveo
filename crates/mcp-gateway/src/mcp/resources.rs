use chrono::Utc;
use rmcp::{
    model::{
        ErrorData as McpError, ListResourceTemplatesResult, ListResourcesResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
        ResourceTemplate, SubscribeRequestParams, TaskStatus, UnsubscribeRequestParams,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{
    GATEWAY_TASK_RESOURCE_TEMPLATE, GatewayAction, GatewayResourceProjection,
    GatewayResourceSubscription, PolicyReasonCode, paginate, parse_gateway_task_resource_uri,
};

use crate::mcp_support::{
    mcp_internal, mcp_invalid_params, project_gateway_resource_uri_for_upstream,
    project_listed_resource, project_listed_resource_uri, project_read_resource_result,
    project_resource_template_uri, resource_policy_target, resource_read_action, upstream_error,
};

use super::tools::direct_task_status_document;
use super::{GATEWAY_PAGE_SIZE, GatewayMcp};

impl GatewayMcp {
    pub(super) async fn handle_list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut resources = Vec::new();
        for server_slug in self.profile_servers() {
            let catalog = self.catalog.current();
            let manifest = catalog
                .server(&server_slug)
                .ok_or_else(|| mcp_internal(format!("unknown profile server `{server_slug}`")))?;
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for mut resource in upstream
                .list_all_resources()
                .await
                .map_err(upstream_error)?
            {
                let Some(projection) = self.project_upstream_resource_for_owner(
                    &server_slug,
                    &subject.principal.id,
                    &resource.uri,
                )?
                else {
                    continue;
                };
                project_listed_resource_uri(manifest, &mut resource)?;
                project_listed_resource(&mut resource, &projection);
                if !self.allows_resource(
                    &context,
                    GatewayAction::ResourcesList,
                    projection.server.clone(),
                    &resource.uri,
                )? {
                    continue;
                }
                resources.push(resource);
            }
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        let page = paginate(resources, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    pub(super) async fn handle_list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut templates = Vec::new();
        if !self.profile_task_servers().is_empty() {
            templates.push(
                ResourceTemplate::new(GATEWAY_TASK_RESOURCE_TEMPLATE, "veoveo-task-status")
                    .with_title("Veoveo task status")
                    .with_description(
                        "Veoveo task status and result document addressed by Veoveo task id.",
                    )
                    .with_mime_type("application/json"),
            );
        }
        for server_slug in self.profile_servers() {
            let catalog = self.catalog.current();
            let manifest = catalog
                .server(&server_slug)
                .ok_or_else(|| mcp_internal(format!("unknown profile server `{server_slug}`")))?;
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for mut template in upstream
                .list_all_resource_templates()
                .await
                .map_err(upstream_error)?
            {
                project_resource_template_uri(manifest, &mut template)?;
                if !self.allows_resource(
                    &context,
                    GatewayAction::ResourcesTemplatesList,
                    server_slug.clone(),
                    &template.uri_template,
                )? {
                    continue;
                }
                templates.push(template);
            }
        }
        templates.sort_by(|left, right| left.uri_template.cmp(&right.uri_template));
        let page = paginate(templates, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    pub(super) async fn handle_read_resource(
        &self,
        mut request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if let Some(task_id) = parse_gateway_task_resource_uri(&request.uri) {
            return self
                .handle_read_gateway_task_resource(task_id, context)
                .await;
        }
        let server = self.server_for_resource(&request.uri)?;
        let projection = self.project_resource_for_upstream(&request.uri)?;
        let subject = self.authorize_projected_resource(
            &context,
            resource_read_action(&request.uri),
            &projection,
        )?;
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let catalog = self.catalog.current();
        let manifest = catalog
            .server(&server)
            .ok_or_else(|| mcp_internal(format!("unknown resource server `{server}`")))?;
        let upstream_resources = upstream
            .list_all_resources()
            .await
            .map_err(upstream_error)?;
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
            task: projection.task,
        };
        request.uri = projection.upstream_uri.to_string();
        let mut result = upstream
            .read_resource(request)
            .await
            .map_err(upstream_error)?;
        project_read_resource_result(&mut result, &projection)?;
        Ok(result)
    }

    async fn handle_read_gateway_task_resource(
        &self,
        task_id: &str,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let mapping = self.task_mapping(task_id)?;
        self.authorize_mapped_task(&context, GatewayAction::ResourcesRead, &mapping)?;
        let task = self.direct_task_status(task_id, &context).await?;
        let result = if task.status == TaskStatus::Completed {
            self.authorize_mapped_task(&context, GatewayAction::TasksResult, &mapping)?;
            Some(
                serde_json::to_value(self.direct_task_payload_result(task_id, &context).await?)
                    .map_err(|err| mcp_internal(format!("failed to encode task result: {err}")))?,
            )
        } else {
            None
        };
        let document = direct_task_status_document(&task, result)?;
        let uri = document.task.status_resource.to_string();
        let text = serde_json::to_string(&document).map_err(|err| {
            mcp_internal(format!(
                "failed to encode gateway task status resource: {err}"
            ))
        })?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::TextResourceContents {
                uri,
                mime_type: Some("application/json".to_string()),
                text,
                meta: None,
            },
        ]))
    }

    pub(super) async fn handle_subscribe(
        &self,
        mut request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let uri = request.uri.clone();
        let projection = self.project_resource_for_upstream(&uri)?;
        let resource_uri = projection.gateway_uri.clone();
        let subject = self.authorize_projected_resource(
            &context,
            GatewayAction::ResourcesSubscribe,
            &projection,
        )?;
        request.uri = projection.upstream_uri.to_string();
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        upstream.subscribe(request).await.map_err(upstream_error)?;
        let now = Utc::now();
        self.state
            .record_resource_subscription(&GatewayResourceSubscription {
                profile: self.profile_id.clone(),
                owner: subject.principal.id,
                upstream_server: projection.server,
                resource_uri,
                created_at: now,
                updated_at: now,
            })
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to persist gateway resource subscription: {err}"
                ))
            })?;
        Ok(())
    }

    pub(super) async fn handle_unsubscribe(
        &self,
        mut request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let uri = request.uri.clone();
        let projection = self.project_resource_for_upstream(&uri)?;
        let resource_uri = projection.gateway_uri.clone();
        let server = projection.server.clone();
        let subject = self.authenticated(&context)?;
        let subscription = self
            .state
            .resource_subscription(
                &self.profile_id,
                &subject.principal.id,
                &server,
                &resource_uri,
            )
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to read gateway resource subscription: {err}"
                ))
            })?;
        if subscription.is_none() {
            self.record_policy_denial(
                &subject,
                GatewayAction::ResourcesUnsubscribe,
                resource_policy_target(server.clone(), resource_uri.as_str())?,
                PolicyReasonCode::UnknownResource,
            )?;
            return Err(mcp_invalid_params("unknown gateway resource subscription"));
        }
        let subject = self.authorize_projected_resource(
            &context,
            GatewayAction::ResourcesUnsubscribe,
            &projection,
        )?;
        request.uri = projection.upstream_uri.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        upstream
            .unsubscribe(request)
            .await
            .map_err(upstream_error)?;
        self.state
            .delete_resource_subscription(
                &self.profile_id,
                &subject.principal.id,
                &server,
                &resource_uri,
            )
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to delete gateway resource subscription: {err}"
                ))
            })?;
        Ok(())
    }
}
