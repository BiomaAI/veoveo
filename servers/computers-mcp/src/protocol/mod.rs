mod auth;
mod guard;
pub(crate) mod resources;
mod subscriptions;
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
        Self { app }
    }
}
fn read_error(error: ApplicationError) -> ErrorData {
    match error {
        ApplicationError::Domain(ComputerError::NotFound | ComputerError::InvalidInput) => {
            ErrorData::invalid_params("unknown Computer resource", None)
        }
        ApplicationError::Domain(ComputerError::Forbidden) => auth::forbidden(),
        _ => auth::unavailable(),
    }
}
fn page<T>(
    values: Vec<T>,
    params: Option<&PaginatedRequestParams>,
) -> Result<veoveo_mcp_contract::Page<T>, ErrorData> {
    veoveo_mcp_contract::paginate(values, params, 100)
        .map_err(|_| ErrorData::invalid_params("invalid catalog cursor", None))
}
impl ServerHandler for ComputersMcp {
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }
    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_prompts()
            .enable_completions()
            .build();
        capabilities
            .extensions
            .get_or_insert_default()
            .insert(TASKS_EXTENSION_ID.into(), JsonObject::new());
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = Implementation::new("computers", env!("CARGO_PKG_VERSION"));
        info.instructions = Some("Read computer://computers for your collection, availability and permitted actions. Lifecycle tools require the Tasks extension and a stable requestId. Disconnecting access leaves work running; Stop ends processes and preserves the home. Recovery Required keeps uncertain operations protected.".into());
        info
    }
    async fn list_tools(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        auth::actor(&context)?;
        let p = page(tasks::tools(), params.as_ref())?;
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
        let actor = self.task_owner(&context, &request.task_id, false).await?;
        veoveo_task_runtime::get_durable_task(&self.app.tasks, actor.owner(), request).await
    }
    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let actor = self.task_owner(&context, &request.task_id, false).await?;
        veoveo_task_runtime::update_durable_task(&self.app.tasks, actor.owner(), request).await
    }
    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let actor = self.task_owner(&context, &request.task_id, true).await?;
        veoveo_task_runtime::cancel_durable_task(&self.app.tasks, actor.owner(), request.task_id)
            .await
    }
    async fn list_resources(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        auth::actor(&context)?;
        let p = page(resources::roots(), params.as_ref())?;
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
        let p = page(resources::templates(), params.as_ref())?;
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
                resources::COMPUTER_TEMPLATE | resources::ACCESS_TEMPLATE
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
            let (ids, more) = self
                .app
                .store
                .complete_ids(actor.owner(), &request.argument.value)
                .await
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
            (Reference::Resource(r), "doc_id") if r.uri == resources::DOC_TEMPLATE => {
                resources::DOCS
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
