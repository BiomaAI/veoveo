mod auth;
mod automation;
mod guard;
mod resources;
pub(crate) mod setup;
mod subscriptions;
pub fn validate_contract() {
    std::sync::LazyLock::force(&setup::SERVER_SETUP);
}
mod task_authority;
mod tasks;
use crate::{Application, ApplicationError};
use rmcp::{
    ErrorData, RoleServer,
    handler::server::router::tool::ToolRouter,
    model::*,
    service::{RequestContext, SubscriptionContext},
};
use std::sync::Arc;
use veoveo_computers::ComputerError;
use veoveo_computers_contract::ComputerResource;
use veoveo_mcp_contract::{
    hosting::{DomainRead, DomainServer, Listing, TaskSupport, unknown_prompt},
    server_contract::McpServerSetup,
};

#[derive(Clone)]
pub struct ComputersMcp {
    pub(crate) app: Arc<Application>,
    tool_router: Arc<ToolRouter<Self>>,
}
impl ComputersMcp {
    pub fn new(app: Arc<Application>) -> Self {
        validate_contract();
        Self {
            app,
            tool_router: Arc::new(tasks::router()),
        }
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
impl DomainServer for ComputersMcp {
    type Contract = setup::ComputersContract;

    fn setup() -> &'static McpServerSetup<Self::Contract> {
        &setup::SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    async fn list_tools(
        &self,
        tools: Vec<Tool>,
        context: &RequestContext<RoleServer>,
    ) -> Result<Listing<Tool>, ErrorData> {
        auth::actor(context)?;
        Ok(Listing::all(tools))
    }

    async fn list_resources(
        &self,
        declared: Vec<Resource>,
        _cursor: Option<&str>,
        context: &RequestContext<RoleServer>,
    ) -> Result<Listing<Resource>, ErrorData> {
        auth::actor(context)?;
        Ok(Listing::all(declared))
    }

    async fn list_resource_templates(
        &self,
        declared: Vec<ResourceTemplate>,
        context: &RequestContext<RoleServer>,
    ) -> Result<Listing<ResourceTemplate>, ErrorData> {
        auth::actor(context)?;
        Ok(Listing::all(declared))
    }

    async fn read(
        &self,
        address: ComputerResource,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, ErrorData> {
        self.resource(address, request, context)
            .await
            .map(DomainRead::private)
    }

    fn prompts(&self) -> Vec<Prompt> {
        vec![Prompt::new(
            "develop",
            Some("Plan work in a private retained Computer"),
            None,
        )]
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        auth::actor(&context)?;
        if request.name != "develop" {
            return Err(unknown_prompt(&request.name));
        }
        if request.arguments.is_some_and(|a| !a.is_empty()) {
            return Err(ErrorData::invalid_params(
                "the develop prompt takes no arguments",
                None,
            ));
        }
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            "Read computer://computers and select a Computer using its current permitted actions. Keep lifecycle request IDs stable across retries and follow the returned Task until its authoritative outcome. Preserve work in the retained home. Explain whether access is disconnected or the Computer is stopped; retain uncertain operations for recovery.",
        )]))
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, ErrorData> {
        let actor = auth::actor(&context)?;
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        if request.argument.name != "computer_id"
            || !matches!(
                reference.uri.as_str(),
                veoveo_computers_contract::COMPUTER_TEMPLATE
                    | veoveo_computers_contract::ACCESS_TEMPLATE
                    | veoveo_computers_contract::AUTOMATION_TEMPLATE
                    | veoveo_computers_contract::GRANT_TEMPLATE
            )
        {
            return Ok(CompleteResult::default());
        }
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
        Ok(CompleteResult::new(
            CompletionInfo::with_pagination(ids, None, more).map_err(|_| auth::unavailable())?,
        ))
    }
}

/// Computers' Tasks. Each Computers tool creates its own domain operation and
/// Task, so the host dispatches every call to the tool. Task requests and
/// subscriptions resolve the owning operation, command, file transfer or
/// maintenance record before the task store sees them.
#[derive(Clone)]
pub struct ComputerTasks(ComputersMcp);

impl ComputerTasks {
    pub fn new(server: ComputersMcp) -> Self {
        Self(server)
    }
}

impl TaskSupport for ComputerTasks {
    async fn start_task(
        &self,
        _request: &mut CallToolRequestParams,
        _context: &RequestContext<RoleServer>,
    ) -> Result<Option<CreateTaskResult>, ErrorData> {
        Ok(None)
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, ErrorData> {
        let access = self
            .0
            .task_access(&context, &request.task_id, false)
            .await?;
        access
            .run(veoveo_task_runtime::get_durable_task(
                &self.0.app.tasks.for_owner(&access.owner),
                request,
            ))
            .await
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let access = self
            .0
            .task_access(&context, &request.task_id, false)
            .await?;
        access
            .run(veoveo_task_runtime::update_durable_task(
                &self.0.app.tasks.for_owner(&access.owner),
                request,
            ))
            .await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let access = self.0.task_access(&context, &request.task_id, true).await?;
        access
            .run(veoveo_task_runtime::cancel_durable_task(
                &self.0.app.tasks.for_owner(&access.owner),
                request.task_id,
            ))
            .await
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        self.0.listen_updates(context).await
    }
}
