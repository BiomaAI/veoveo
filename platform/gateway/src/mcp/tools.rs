use std::borrow::Cow;

use futures::{StreamExt, stream};
use rmcp::{
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResponse, CallToolResult, ClientRequest,
        CreateTaskResult, DetailedTask, ErrorData as McpError, ListToolsResult,
        PaginatedRequestParams, ServerResult, TaskPayload,
    },
    service::{Peer, PeerRequestOptions, RequestContext, RoleClient, RoleServer},
};
use serde_json::Value;
use veoveo_mcp_contract::{
    GatewayAction, GatewayDiscoverySurface, LocalToolName, TaskExposure, paginate,
    related_task_meta, sanitized_request_meta,
};
use veoveo_platform_store::PrincipalKind as StorePrincipalKind;

use crate::{
    AuthenticatedSubject,
    mcp_support::{
        mcp_internal, mcp_invalid_params, parse_gateway_tool, project_call_tool_resource_uris,
        project_tool_resource_metadata, unexpected_upstream_response, upstream_error,
    },
    state::GatewayTaskRouteDraft,
};

use super::{
    GATEWAY_PAGE_SIZE, GatewayMcp,
    discovery::{DiscoveryCacheKey, MAX_CONCURRENT_DISCOVERY, isolate_discovery_failures},
    invocation_authorization_fingerprint,
};

impl GatewayMcp {
    pub(super) async fn handle_list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
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
                if let Some(tools) = self.discovery.tools(&key).await {
                    return (server_slug, Ok::<_, McpError>(tools));
                }
                let result = async {
                    let manifest = catalog.server(&server_slug).ok_or_else(|| {
                        mcp_internal(format!("unknown profile server `{server_slug}`"))
                    })?;
                    let upstream_tools = self
                        .idempotent_upstream_request(
                            &server_slug,
                            context.peer.clone(),
                            subject,
                            |upstream| async move { upstream.list_all_tools().await },
                        )
                        .await?;
                    let mut tools = Vec::new();
                    for mut tool in upstream_tools {
                        let local_tool = LocalToolName::new(tool.name.as_ref().to_owned())
                            .map_err(|err| {
                                mcp_internal(format!("upstream exposed invalid tool name: {err}"))
                            })?;
                        if !self.client_allows_compatibility_helper(
                            subject,
                            &server_slug,
                            &local_tool,
                        )? {
                            continue;
                        }
                        if !self
                            .allows_tool(
                                context,
                                GatewayAction::ToolsList,
                                server_slug.clone(),
                                local_tool.clone(),
                            )
                            .await?
                        {
                            continue;
                        }
                        project_tool_resource_metadata(manifest, &mut tool)?;
                        let gateway_name = catalog
                            .project_tool_name(&server_slug, &local_tool)
                            .map_err(|err| {
                                mcp_internal(format!("failed to project tool name: {err}"))
                            })?;
                        tool.name = Cow::Owned(gateway_name.to_string());
                        tools.push(tool);
                    }
                    self.discovery.store_tools(key, tools.clone()).await;
                    Ok(tools)
                }
                .await;
                (server_slug, result)
            }
        }))
        .buffer_unordered(MAX_CONCURRENT_DISCOVERY)
        .collect::<Vec<_>>()
        .await;
        let (mut tools, degradation, errors) =
            isolate_discovery_failures(GatewayDiscoverySurface::Tools, results);
        for (server, error) in errors {
            tracing::warn!(%server, %error, "isolated upstream tool discovery failure");
        }
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = paginate(tools, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: degradation.into_meta(),
        })
    }

    pub(super) async fn handle_call_tool(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let catalog = self.catalog.current();
        let projection = parse_gateway_tool(&catalog, &request.name)?;
        let subject = self.authenticated(&context)?;
        if !self.client_allows_compatibility_helper(
            &subject,
            &projection.server,
            &projection.tool,
        )? {
            self.record_policy_denial(
                &subject,
                GatewayAction::ToolsCall,
                veoveo_mcp_contract::PolicyTarget::Tool {
                    server: projection.server.clone(),
                    tool: projection.tool.clone(),
                },
                veoveo_mcp_contract::PolicyReasonCode::UnknownTool,
            )
            .await?;
            return Err(mcp_invalid_params("unknown tool"));
        }
        let subject = self
            .authorize_tool(
                &context,
                GatewayAction::ToolsCall,
                projection.server.clone(),
                projection.tool.clone(),
            )
            .await?;
        restore_request_meta(&mut request, &context.meta);
        request.name = Cow::Owned(projection.tool.to_string());

        let downstream_tasks = context
            .meta
            .client_capabilities()
            .is_some_and(|capabilities| capabilities.supports_tasks());
        let project_tasks = downstream_tasks && self.client_allows_task_projection(&subject)?;
        let direct_adapter = self.client_uses_direct_task_call_adapter(&subject)?;
        let server_supports_tasks = catalog
            .profile_server(&self.profile_id, &projection.server)
            .is_some_and(|(_, exposure, manifest)| {
                exposure.tasks == TaskExposure::Enabled && manifest.capabilities.tasks
            });
        let effective_tasks = server_supports_tasks && (project_tasks || direct_adapter);
        let downstream_progress_token = context.meta.get_progress_token();
        let upstream = self
            .upstream_with_tasks(
                &projection.server,
                context.peer.clone(),
                &subject,
                effective_tasks,
            )
            .await?;
        let handle = upstream
            .peer
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(request)),
                PeerRequestOptions::no_options(),
            )
            .await
            .map_err(upstream_error)?;
        if let Some(downstream_token) = downstream_progress_token {
            self.progress_tokens
                .register(
                    &self.profile_id,
                    &subject.principal.id,
                    &projection.server,
                    handle.progress_token.clone(),
                    downstream_token,
                )
                .await;
        }
        let upstream_token = handle.progress_token.clone();
        let result = handle.await_response().await.map_err(upstream_error);
        self.progress_tokens
            .remove_token(
                &self.profile_id,
                &subject.principal.id,
                &projection.server,
                &upstream_token,
            )
            .await;

        match result? {
            ServerResult::CallToolResult(mut result) => {
                let manifest = catalog.server(&projection.server).ok_or_else(|| {
                    mcp_internal(format!("unknown tool server `{}`", projection.server))
                })?;
                project_call_tool_resource_uris(manifest, &mut result)?;
                Ok(CallToolResponse::Complete(result))
            }
            ServerResult::InputRequiredResult(result) => {
                Ok(CallToolResponse::InputRequired(result))
            }
            ServerResult::CreateTaskResult(created) if project_tasks => {
                let created = self
                    .project_created_task(&subject, &projection.server, created)
                    .await?;
                Ok(CallToolResponse::Task(created))
            }
            ServerResult::CreateTaskResult(created) if direct_adapter => {
                let source_task_id = created.task.task_id.clone();
                let projected = self
                    .project_created_task(&subject, &projection.server, created)
                    .await?;
                let canonical_task_id = projected.task.task_id;
                let detailed = await_terminal_task(
                    &upstream.peer,
                    source_task_id,
                    context.ct.clone(),
                    projected.task.poll_interval_ms,
                )
                .await?;
                let mut result = completed_tool_result(detailed)?;
                let manifest = catalog.server(&projection.server).ok_or_else(|| {
                    mcp_internal(format!("unknown tool server `{}`", projection.server))
                })?;
                project_call_tool_resource_uris(manifest, &mut result)?;
                result.meta = Some(related_task_meta(canonical_task_id));
                Ok(CallToolResponse::Complete(result))
            }
            ServerResult::CreateTaskResult(_) => Err(McpError::missing_required_client_capability(
                rmcp::model::ClientCapabilities::builder()
                    .enable_tasks()
                    .build(),
            )),
            other => Err(unexpected_upstream_response("tools/call", other)),
        }
    }

    async fn project_created_task(
        &self,
        subject: &AuthenticatedSubject,
        server: &veoveo_mcp_contract::ServerSlug,
        mut created: CreateTaskResult,
    ) -> Result<CreateTaskResult, McpError> {
        let authority_digest = hex::encode(invocation_authorization_fingerprint(
            &subject.actor,
            &subject.authority,
        )?);
        let owner_kind = match subject.actor.kind {
            veoveo_mcp_contract::PrincipalKind::User => StorePrincipalKind::User,
            veoveo_mcp_contract::PrincipalKind::Service => StorePrincipalKind::Service,
        };
        let (canonical, _) = self
            .state
            .create_task_route(GatewayTaskRouteDraft {
                tenant_key: subject.authority.tenant.to_string(),
                owner_key: subject.actor.id.to_string(),
                owner_issuer: subject.actor.issuer.to_string(),
                owner_subject: subject.actor.subject.to_string(),
                owner_kind,
                work_context: subject.authority.work_context.to_string(),
                profile: self.profile_id.to_string(),
                server: server.to_string(),
                source_task_id: created.task.task_id.clone(),
                authority_digest,
                ttl_ms: created.task.ttl_ms,
            })
            .await
            .map_err(|error| {
                mcp_internal(format!("failed to persist gateway task route: {error}"))
            })?;
        created.task.task_id = canonical.to_string();
        created.meta = Some(related_task_meta(canonical.to_string()));
        Ok(created)
    }
}

fn restore_request_meta(
    request: &mut CallToolRequestParams,
    context_meta: &rmcp::model::RequestMetaObject,
) {
    if context_meta.is_empty() {
        return;
    }
    request
        .meta
        .get_or_insert_with(rmcp::model::RequestMetaObject::new)
        .extend(sanitized_request_meta(context_meta));
}

async fn await_terminal_task(
    peer: &Peer<RoleClient>,
    task_id: String,
    cancellation: tokio_util::sync::CancellationToken,
    initial_poll_interval_ms: Option<u64>,
) -> Result<DetailedTask, McpError> {
    let mut poll_interval_ms = initial_poll_interval_ms.unwrap_or(1_000).clamp(100, 30_000);
    loop {
        let current = peer
            .get_task(rmcp::model::GetTaskParams::new(task_id.clone()))
            .await
            .map_err(upstream_error)?
            .task;
        if current.status().is_terminal() {
            return Ok(current);
        }
        poll_interval_ms = current
            .task
            .poll_interval_ms
            .unwrap_or(poll_interval_ms)
            .clamp(100, 30_000);
        tokio::select! {
            () = cancellation.cancelled() => {
                let _ = peer.cancel_task(rmcp::model::CancelTaskParams::new(task_id)).await;
                return Err(McpError::invalid_request("task wait was cancelled", None));
            }
            () = tokio::time::sleep(std::time::Duration::from_millis(poll_interval_ms)) => {}
        }
    }
}

pub(super) fn completed_tool_result(task: DetailedTask) -> Result<CallToolResult, McpError> {
    match task.payload {
        TaskPayload::Completed { result } => {
            serde_json::from_value(Value::Object(result)).map_err(|error| {
                mcp_internal(format!(
                    "upstream task result was not a tool result: {error}"
                ))
            })
        }
        TaskPayload::Failed { error } => {
            let error = serde_json::from_value(Value::Object(error)).map_err(|decode| {
                mcp_internal(format!("upstream task error was malformed: {decode}"))
            })?;
            Err(error)
        }
        TaskPayload::Cancelled => Err(McpError::invalid_request("task was cancelled", None)),
        TaskPayload::Working | TaskPayload::InputRequired { .. } => {
            Err(mcp_internal("task result requested before completion"))
        }
        _ => Err(mcp_internal(
            "upstream returned an unsupported task payload",
        )),
    }
}

pub(super) fn project_detailed_task_resource_uris(
    manifest: &veoveo_mcp_contract::ServerManifest,
    task: &mut DetailedTask,
) -> Result<(), McpError> {
    let TaskPayload::Completed { result } = &mut task.payload else {
        return Ok(());
    };
    let mut tool_result: CallToolResult = serde_json::from_value(Value::Object(result.clone()))
        .map_err(|error| {
            mcp_internal(format!(
                "upstream completed task result was not a tool result: {error}"
            ))
        })?;
    project_call_tool_resource_uris(manifest, &mut tool_result)?;
    *result = serde_json::to_value(tool_result)
        .map_err(|error| mcp_internal(format!("failed to encode projected task result: {error}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| mcp_internal("projected task result was not an object"))?;
    Ok(())
}

pub(super) fn rewrite_detailed_task_id(task: &mut DetailedTask, canonical_task_id: &str) {
    task.task.task_id = canonical_task_id.to_owned();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_context_metadata_is_restored_before_upstream_projection() {
        let mut request = CallToolRequestParams::new("timeseries__forecast");
        let mut context_meta = rmcp::model::RequestMetaObject::new();
        context_meta.set_traceparent("00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01");

        restore_request_meta(&mut request, &context_meta);

        assert_eq!(
            request
                .meta
                .as_ref()
                .and_then(|meta| meta.get_traceparent()),
            context_meta.get_traceparent()
        );
    }
}
