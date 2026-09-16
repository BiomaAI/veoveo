//! Native MCP Apps use the same durable human operation journal as chat tools.
use super::*;
use rmcp::model::{
    CancelTaskParams, CreateTaskResult, GetTaskResult, InputRequiredResult, Resource,
    TaskAckResult, TaskPayload, UpdateTaskParams,
};
use veoveo_mcp_apps_extension::{app_allows_tool, is_app_resource, resolve_app_tool};
use veoveo_platform_store::workspace::WorkspaceOperationIntent;

pub(super) fn router() -> Router<OperationState> {
    Router::new()
        .route(
            "/workspace-api/{profile}/chats/{chat}/app-operations",
            post(start),
        )
        .route("/workspace-api/{profile}/app-operations/{id}", post(detail))
        .route("/workspace-api/{profile}/app-tasks/get", post(get_task))
        .route(
            "/workspace-api/{profile}/app-tasks/update",
            post(update_task),
        )
        .route(
            "/workspace-api/{profile}/app-tasks/cancel",
            post(cancel_task),
        )
}

async fn app_resource(client: &native::NativeClient, uri: &str) -> Result<Resource, StatusCode> {
    if uri.len() > 2048 || !uri.starts_with("ui://") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut cursor = None;
    let mut seen = 0;
    for _ in 0..16 {
        let page = tokio::time::timeout(
            Duration::from_secs(8),
            client.peer().list_resources(
                cursor.map(|cursor| PaginatedRequestParams::default().with_cursor(Some(cursor))),
            ),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error)?;
        seen += page.resources.len();
        if seen > 1024 {
            return Err(StatusCode::BAD_GATEWAY);
        }
        if let Some(resource) = page
            .resources
            .into_iter()
            .find(|resource| resource.uri == uri && is_app_resource(resource))
        {
            return Ok(resource);
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Err(StatusCode::NOT_FOUND)
}

pub(super) async fn authorize(
    client: &native::NativeClient,
    uri: &str,
    tool: &str,
) -> Result<(), StatusCode> {
    let resource = app_resource(client, uri).await?;
    if app_allows_tool(&resource, &tools(client).await?, tool) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

async fn start(
    State(state): State<OperationState>,
    Path((raw_profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::StartAppOperation>,
) -> Api<wire::OperationSummary> {
    let profile = profile(raw_profile)?;
    let caller = Caller::new(profile.clone(), subject.clone(), &headers)?;
    state.authority(&subject, &profile).await?;
    let client = state.native.connect(&profile, &caller.bearer).await?;
    let selected = async {
        let resource = app_resource(&client, &request.app_uri).await?;
        let catalog = tools(&client).await?;
        let tool =
            resolve_app_tool(&resource, &catalog, &request.tool).ok_or(StatusCode::FORBIDDEN)?;
        Ok::<_, StatusCode>(tool.name.to_string())
    }
    .await;
    client.close().await;
    let tool = selected?;
    if let Some(reference) = request.request_state {
        let (id, revision) = continuation(&reference)?;
        let operation = state.operation(&subject, &profile, id).await?;
        let arguments =
            serde_json::from_str::<std::collections::BTreeMap<String, serde_json::Value>>(
                &operation.arguments,
            )
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if operation.app_uri.as_deref() != Some(&request.app_uri)
            || operation.tool != tool
            || operation.chat != WorkspaceChatId::from_uuid(chat).record_id()
            || arguments != request.arguments
            || operation.revision != revision
            || operation.phase != Phase::InputRequired
        {
            return Err(StatusCode::CONFLICT);
        }
        let previous: InputRequiredResult = projection::decode(&operation)?;
        let responses = request.input_responses.unwrap_or_default();
        let answers =
            inputs::native_answers(&previous.input_requests.unwrap_or_default(), responses)?;
        commands::answer(
            State(state.clone()),
            Path((profile.to_string(), id)),
            Extension(subject.clone()),
            headers,
            Json(wire::AnswerOperation { revision, answers }),
        )
        .await?;
        return Ok(Json(projection::summary(
            &state.operation(&subject, &profile, id).await?,
        )?));
    }
    if request.input_responses.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(Json(
        commands::submit(
            state,
            caller,
            WorkspaceOperationId::from_uuid(request.id.0),
            WorkspaceOperationIntent {
                chat: WorkspaceChatId::from_uuid(chat),
                run: None,
                profile: profile.to_string(),
                app_uri: Some(request.app_uri),
                tool,
                arguments: serde_json::to_string(&request.arguments)
                    .map_err(|_| StatusCode::BAD_REQUEST)?,
            },
            false,
        )
        .await?,
    ))
}

fn continuation(reference: &str) -> Result<(Uuid, i64), StatusCode> {
    let (id, revision) = reference
        .strip_prefix("workspace-operation:")
        .and_then(|value| value.split_once(':'))
        .ok_or(StatusCode::BAD_REQUEST)?;
    Ok((
        Uuid::parse_str(id).map_err(|_| StatusCode::BAD_REQUEST)?,
        revision.parse().map_err(|_| StatusCode::BAD_REQUEST)?,
    ))
}

async fn detail(
    State(state): State<OperationState>,
    Path((raw_profile, id)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(origin): Json<wire::AppOrigin>,
) -> Api<wire::AppOperationView> {
    let profile = profile(raw_profile)?;
    let operation = state.operation(&subject, &profile, id).await?;
    if operation.app_uri.as_deref() != Some(&origin.app_uri) {
        return Err(StatusCode::NOT_FOUND);
    }
    let client = state
        .native
        .connect(&profile, &native::bearer(&headers)?)
        .await?;
    let response = async {
        authorize(&client, &origin.app_uri, &operation.tool).await?;
        let native = match operation.phase {
            Phase::Task => {
                let task = tokio::time::timeout(
                    Duration::from_secs(8),
                    client
                        .peer()
                        .get_task(GetTaskParams::new(task_id(&operation)?)),
                )
                .await
                .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
                .map_err(mcp_error)?;
                Some(wire::AppToolResult::Task(CreateTaskResult::new(
                    task.task.task,
                )))
            }
            Phase::Completed => Some(wire::AppToolResult::Complete(projection::decode(
                &operation,
            )?)),
            Phase::InputRequired => {
                let mut input: InputRequiredResult = projection::decode(&operation)?;
                // Host-owned continuation reference. Upstream opaque state stays in the journal.
                input.request_state =
                    Some(format!("workspace-operation:{id}:{}", operation.revision));
                Some(wire::AppToolResult::InputRequired(input))
            }
            _ => None,
        };
        Ok(Json(wire::AppOperationView {
            operation: projection::summary(&operation)?,
            native,
        }))
    }
    .await;
    client.close().await;
    response
}

async fn task_authority(
    state: &OperationState,
    caller: &Caller,
    app: &str,
    task: &str,
) -> Result<WorkspaceOperation, StatusCode> {
    let authority = state.authority(&caller.subject, &caller.profile).await?;
    state
        .workspace
        .store
        .workspace_app_task(&authority, caller.profile.as_str(), app, task)
        .await
        .map_err(fault)
}

async fn get_task(
    State(state): State<OperationState>,
    Path(raw_profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::AppTaskRequest>,
) -> Api<GetTaskResult> {
    let caller = Caller::new(profile(raw_profile)?, subject, &headers)?;
    let operation = task_authority(&state, &caller, &request.app_uri, &request.task_id).await?;
    let client = state
        .native
        .connect(&caller.profile, &caller.bearer)
        .await?;
    let result = async {
        authorize(&client, &request.app_uri, &operation.tool).await?;
        tokio::time::timeout(
            Duration::from_secs(8),
            client.peer().get_task(GetTaskParams::new(request.task_id)),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error)
        .map(Json)
    }
    .await;
    client.close().await;
    result
}

async fn cancel_task(
    State(state): State<OperationState>,
    Path(raw_profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::AppTaskRequest>,
) -> Api<TaskAckResult> {
    let caller = Caller::new(profile(raw_profile)?, subject, &headers)?;
    let operation = task_authority(&state, &caller, &request.app_uri, &request.task_id).await?;
    let client = state
        .native
        .connect(&caller.profile, &caller.bearer)
        .await?;
    let result = async {
        authorize(&client, &request.app_uri, &operation.tool).await?;
        tokio::time::timeout(
            Duration::from_secs(8),
            client
                .peer()
                .cancel_task(CancelTaskParams::new(request.task_id)),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error)
        .map(|()| Json(TaskAckResult::new()))
    }
    .await;
    client.close().await;
    result
}

async fn update_task(
    State(state): State<OperationState>,
    Path(raw_profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::UpdateAppTask>,
) -> Api<TaskAckResult> {
    let caller = Caller::new(profile(raw_profile)?, subject, &headers)?;
    let operation = task_authority(&state, &caller, &request.app_uri, &request.task_id).await?;
    let client = state
        .native
        .connect(&caller.profile, &caller.bearer)
        .await?;
    let result = async {
        authorize(&client, &request.app_uri, &operation.tool).await?;
        let current = tokio::time::timeout(
            Duration::from_secs(8),
            client
                .peer()
                .get_task(GetTaskParams::new(request.task_id.clone())),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error)?;
        let TaskPayload::InputRequired { input_requests } = current.task.payload else {
            return Err(StatusCode::CONFLICT);
        };
        let answers = inputs::native_answers(&input_requests, request.input_responses)?;
        let responses = inputs::answers(&input_requests, answers)?;
        tokio::time::timeout(
            Duration::from_secs(8),
            client
                .peer()
                .update_task(UpdateTaskParams::new(request.task_id, responses)),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error)
        .map(|()| Json(TaskAckResult::new()))
    }
    .await;
    client.close().await;
    result
}
