use super::*;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CancelTaskParams, InputRequiredResult, TaskPayload,
    UpdateTaskParams,
};
use tokio::sync::OwnedSemaphorePermit;
use veoveo_platform_store::workspace::{
    WorkspaceOperationIntent, WorkspaceOperationOutcome as Outcome,
};

pub(super) async fn start(
    State(state): State<OperationState>,
    Path((raw_profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::StartOperation>,
) -> Api<wire::OperationSummary> {
    let profile = profile(raw_profile)?;
    let caller = Caller::new(profile.clone(), subject, &headers)?;
    Ok(Json(
        submit(
            state,
            caller,
            WorkspaceOperationId::from_uuid(request.id.0),
            WorkspaceOperationIntent {
                chat: WorkspaceChatId::from_uuid(chat),
                run: None,
                profile: profile.to_string(),
                tool: request.tool,
                arguments: serde_json::to_string(&request.arguments)
                    .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?,
            },
            false,
        )
        .await?,
    ))
}

pub(super) async fn submit(
    state: OperationState,
    caller: Caller,
    id: WorkspaceOperationId,
    intent: WorkspaceOperationIntent,
    wait_for_dispatch: bool,
) -> Result<wire::OperationSummary, StatusCode> {
    if intent.profile != caller.profile.as_str() {
        return Err(StatusCode::FORBIDDEN);
    }
    let authority = state.authority(&caller.subject, &caller.profile).await?;
    let run_fence = intent.run.map(|(_, fence)| fence);
    let permit = state
        .limits
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let admitted = state
        .workspace
        .store
        .start_workspace_operation(&authority, id, intent)
        .await
        .map_err(fault)?;
    let response = projection::summary(&admitted.operation)?;
    if admitted.dispatch {
        let dispatched = tokio::spawn(dispatch(
            state,
            caller,
            admitted.operation,
            None,
            run_fence,
            permit,
        ));
        if wait_for_dispatch {
            // Keep the model run alive until the native invocation has an
            // observed receipt. The Task itself continues independently.
            let _ = dispatched.await;
        }
    }
    Ok(response)
}

pub(super) async fn cancel(
    State(state): State<OperationState>,
    Path((raw_profile, id)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let profile = profile(raw_profile)?;
    let operation = state.operation(&subject, &profile, id).await?;
    let client = state
        .native
        .connect(&profile, &native::bearer(&headers)?)
        .await?;
    let result = tokio::time::timeout(
        Duration::from_secs(8),
        client
            .peer()
            .cancel_task(CancelTaskParams::new(task_id(&operation)?)),
    )
    .await
    .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
    .map_err(mcp_error);
    client.close().await;
    result?;
    // Acknowledges only the request. Only tasks/get can report Cancelled.
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn answer(
    State(state): State<OperationState>,
    Path((raw_profile, id)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::AnswerOperation>,
) -> Result<StatusCode, StatusCode> {
    let profile = profile(raw_profile)?;
    let authority = state.authority(&subject, &profile).await?;
    let operation = state.operation(&subject, &profile, id).await?;
    if request.revision != operation.revision {
        return Err(StatusCode::CONFLICT);
    }
    let bearer = native::bearer(&headers)?;
    let client = state.native.connect(&profile, &bearer).await?;
    if operation.phase == Phase::Task {
        let task_id = task_id(&operation)?;
        let task = tokio::time::timeout(
            Duration::from_secs(8),
            client.peer().get_task(GetTaskParams::new(task_id.clone())),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error)?;
        let TaskPayload::InputRequired { input_requests } = task.task.payload else {
            return Err(StatusCode::CONFLICT);
        };
        let responses = inputs::answers(&input_requests, request.answers)?;
        let result = tokio::time::timeout(
            Duration::from_secs(8),
            client
                .peer()
                .update_task(UpdateTaskParams::new(task_id, responses)),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error);
        client.close().await;
        result?;
    } else if operation.phase == Phase::InputRequired {
        // Native discovery rechecks tool authority before accepting the response.
        if !tools(&client)
            .await?
            .iter()
            .any(|tool| tool.name == operation.tool)
        {
            return Err(StatusCode::FORBIDDEN);
        }
        client.close().await;
        let mut previous: InputRequiredResult = projection::decode(&operation)?;
        let input_requests = previous.input_requests.take().unwrap_or_default();
        if request.answers.len() != input_requests.len() {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let responses = if input_requests.is_empty() {
            Default::default()
        } else {
            inputs::answers(&input_requests, request.answers)?
        };
        let permit = state
            .limits
            .clone()
            .try_acquire_owned()
            .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
        let resumed = state
            .workspace
            .store
            .resume_workspace_operation(
                &authority,
                WorkspaceOperationId::from_uuid(id),
                request.revision,
            )
            .await
            .map_err(fault)?;
        let mut call = call(&resumed)?;
        call.request_state = previous.request_state;
        call.input_responses = Some(responses);
        tokio::spawn(dispatch(
            state,
            Caller {
                profile,
                subject,
                bearer,
            },
            resumed,
            Some(call),
            None,
            permit,
        ));
    } else {
        return Err(StatusCode::CONFLICT);
    }
    Ok(StatusCode::NO_CONTENT)
}

fn call(operation: &WorkspaceOperation) -> Result<CallToolRequestParams, StatusCode> {
    Ok(
        CallToolRequestParams::new(operation.tool.clone()).with_arguments(
            serde_json::from_str(&operation.arguments)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        ),
    )
}

async fn dispatch(
    state: OperationState,
    caller: Caller,
    operation: WorkspaceOperation,
    continuation: Option<CallToolRequestParams>,
    run_fence: Option<Uuid>,
    _permit: OwnedSemaphorePermit,
) {
    let Ok(id) = super::super::projection::uuid(&operation.id) else {
        return;
    };
    let work = async {
        let Ok(client) = state.native.connect(&caller.profile, &caller.bearer).await else {
            return Outcome::Failed;
        };
        let Ok(authority) = state.authority(&caller.subject, &caller.profile).await else {
            return Outcome::Failed;
        };
        if state
            .workspace
            .store
            .check_workspace_operation_dispatch(
                &authority,
                WorkspaceOperationId::from_uuid(id),
                operation.fence,
                run_fence,
            )
            .await
            .is_err()
        {
            return Outcome::Failed;
        }
        let params = match continuation.map(Ok).unwrap_or_else(|| call(&operation)) {
            Ok(value) => value,
            Err(_) => return Outcome::Failed,
        };
        // Explicit once: the SDK must not drive hidden MRTR rounds or retry mutations.
        let response = client.peer().call_tool_once(params).await;
        client.close().await;
        match response {
            Ok(CallToolResponse::Complete(result)) => serde_json::to_string(&result)
                .ok()
                .filter(|value| value.len() <= 1_048_576)
                .map(Outcome::Completed)
                .unwrap_or(Outcome::Unconfirmed),
            Ok(CallToolResponse::InputRequired(result)) => serde_json::to_string(&result)
                .ok()
                .filter(|value| value.len() <= 1_048_576)
                .map(Outcome::InputRequired)
                .unwrap_or(Outcome::Unconfirmed),
            Ok(CallToolResponse::Task(result)) => Outcome::Task(result.task.task_id),
            Err(rmcp::ServiceError::McpError(_)) => Outcome::Failed,
            _ => Outcome::Unconfirmed,
        }
    };
    let outcome = tokio::select! {
        _ = state.stop.cancelled() => Outcome::Unconfirmed,
        result = tokio::time::timeout(Duration::from_secs(85), work) => result.unwrap_or(Outcome::Unconfirmed),
    };
    if state
        .workspace
        .store
        .settle_workspace_operation(
            WorkspaceOperationId::from_uuid(id),
            operation.fence,
            outcome,
        )
        .await
        .is_err()
    {
        tracing::error!(operation_id = %id, "Workspace operation receipt could not settle");
    }
}
