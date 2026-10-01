mod auth;
mod automation;
mod guard;
pub(crate) mod resources;
pub(crate) mod setup;
mod subscriptions;
pub fn validate_contract() {
    std::sync::LazyLock::force(&setup::SERVER_SETUP);
}
mod task_authority;
mod tasks;
use crate::{Application, ApplicationError};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::*,
    service::{RequestContext, SubscriptionContext},
};
use std::sync::Arc;
use veoveo_computers::ComputerError;

#[derive(Clone)]
pub struct ComputersMcp {
    pub(crate) app: Arc<Application>,
}
impl ComputersMcp {
    pub fn new(app: Arc<Application>) -> Self {
        validate_contract();
        Self { app }
    }
}
fn read_error(error: ApplicationError) -> ErrorData {
    match error {
        ApplicationError::Domain(ComputerError::NotFound | ComputerError::InvalidInput) => {
            ErrorData::invalid_params(
                "Computer resource was not found. Read computer://computers to see your Computers.",
                None,
            )
        }
        ApplicationError::Domain(ComputerError::Forbidden) => auth::forbidden(),
        _ => auth::unavailable(),
    }
}
fn page<T>(
    values: Vec<T>,
    params: Option<&PaginatedRequestParams>,
) -> Result<veoveo_mcp_contract::Page<T>, ErrorData> {
    veoveo_mcp_contract::paginate(values, params, 100).map_err(|_| {
        ErrorData::invalid_params(
            "The cursor is invalid. Pass the nextCursor from the previous page.",
            None,
        )
    })
}
impl ServerHandler for ComputersMcp {
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }
    fn get_info(&self) -> ServerConfig {
        setup::SERVER_SETUP.server_config().clone()
    }
    async fn list_tools(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        auth::actor(&context)?;
        let p = page(
            tasks::tools()
                .into_iter()
                .chain(automation::tools())
                .collect(),
            params.as_ref(),
        )?;
        Ok(ListToolsResult {
            tools: p.items,
            next_cursor: p.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.call(request, context).await
    }
    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, ErrorData> {
        let access = self.task_access(&context, &request.task_id, false).await?;
        access
            .run(veoveo_task_runtime::get_durable_task(
                &self.app.tasks.for_owner(&access.owner),
                request,
            ))
            .await
    }
    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let access = self.task_access(&context, &request.task_id, false).await?;
        access
            .run(veoveo_task_runtime::update_durable_task(
                &self.app.tasks.for_owner(&access.owner),
                request,
            ))
            .await
    }
    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let access = self.task_access(&context, &request.task_id, true).await?;
        access
            .run(veoveo_task_runtime::cancel_durable_task(
                &self.app.tasks.for_owner(&access.owner),
                request.task_id,
            ))
            .await
    }
    async fn list_resources(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        auth::actor(&context)?;
        let p = page(
            setup::SERVER_SETUP
                .resources()
                .iter()
                .map(|resource| resource.descriptor().clone())
                .collect(),
            params.as_ref(),
        )?;
        Ok(ListResourcesResult {
            resources: p.items,
            next_cursor: p.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }
    async fn list_resource_templates(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        auth::actor(&context)?;
        let p = page(
            setup::SERVER_SETUP
                .resource_templates()
                .iter()
                .map(|template| template.descriptor().clone())
                .collect(),
            params.as_ref(),
        )?;
        Ok(ListResourceTemplatesResult {
            resource_templates: p.items,
            next_cursor: p.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if let Some(result) = setup::SERVER_SETUP.read_documents(&request, &context)? {
            return Ok(result);
        }
        self.resource(request, context).await
    }
    async fn list_prompts(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        auth::actor(&context)?;
        let p = page(
            vec![Prompt::new(
                "develop",
                Some("Plan work in a private retained Computer"),
                None,
            )],
            params.as_ref(),
        )?;
        Ok(ListPromptsResult {
            prompts: p.items,
            next_cursor: p.next_cursor,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }
    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        auth::actor(&context)?;
        if request.name != "develop" || request.arguments.is_some_and(|a| !a.is_empty()) {
            return Err(ErrorData::invalid_params(
                "unknown prompt or arguments",
                None,
            ));
        }
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(Role::User, "Read computer://computers and select a Computer using its current permitted actions. Keep lifecycle request IDs stable across retries and follow the returned Task until its authoritative outcome. Preserve work in the retained home. Explain whether access is disconnected or the Computer is stopped; retain uncertain operations for recovery.")]).into())
    }
    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, ErrorData> {
        let actor = auth::actor(&context)?;
        if let (Reference::Resource(reference), "computer_id") =
            (&request.r#ref, request.argument.name.as_str())
            && matches!(
                reference.uri.as_str(),
                veoveo_computers_contract::COMPUTER_TEMPLATE
                    | veoveo_computers_contract::ACCESS_TEMPLATE
                    | veoveo_computers_contract::AUTOMATION_TEMPLATE
                    | veoveo_computers_contract::GRANT_TEMPLATE
            )
        {
            let authority = self
                .app
                .store
                .control_authority(&actor)
                .await
                .map_err(|_| auth::forbidden())?;
            authority
                .require_read(None)
                .map_err(|_| auth::forbidden())?;
            let (ids, more) = if reference.uri == veoveo_computers_contract::COMPUTER_TEMPLATE {
                self.app
                    .store
                    .complete_accessible_ids(&actor, &authority, &request.argument.value)
                    .await
            } else {
                self.app
                    .store
                    .complete_ids(actor.owner(), &request.argument.value)
                    .await
            }
            .map_err(|e| read_error(e.into()))?;
            authority
                .require_read(None)
                .map_err(|_| auth::forbidden())?;
            return Ok(CompleteResult::new(
                CompletionInfo::with_pagination(ids, None, more)
                    .map_err(|_| auth::unavailable())?,
            ));
        }
        let values = match (&request.r#ref, request.argument.name.as_str()) {
            (Reference::Resource(r), "doc_id")
                if r.uri == veoveo_computers_contract::DOC_TEMPLATE =>
            {
                setup::SERVER_DOCS
                    .iter()
                    .map(|d| d.id.to_owned())
                    .filter(|id| id.starts_with(&request.argument.value))
                    .collect()
            }
            _ => vec![],
        };
        Ok(CompleteResult::new(
            CompletionInfo::with_pagination(values, None, false)
                .map_err(|_| auth::unavailable())?,
        ))
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        self.listen_updates(context).await
    }
}
