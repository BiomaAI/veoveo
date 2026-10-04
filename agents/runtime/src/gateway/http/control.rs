use crate::contract::AgentAction;
use crate::{
    AgentControlReceipt, AgentControlTarget, AgentRuntimeError, GovernedInputRequest,
    InputRequestAnswer, InputRequestDecisionDraft, OperatorMessageDraft, json_object,
};
use std::{str::FromStr, time::Instant};
use veoveo_mcp_contract::audit::AdministrativeOperation;
use veoveo_mcp_contract::{
    AgentInputRequestDecision, AgentInputRequestView, AgentOperatorMessageRequest,
    AgentWakeReceipt, GatewayProfile, PolicyTarget,
};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_mcp_gateway::http::auth_support::internal_error_response;
use veoveo_platform_store::{AgentInputRequestId, AgentInputRequestState};

use super::AgentManagementState;
use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use veoveo_mcp_gateway::http::action_audit::{
    AdminAuthorizationRequest, AdminOperationAuditRecord, AdminOperationFailure,
    AdminOperationStatus, authorize_gateway_action, record_gateway_operation_audit,
};

#[derive(Clone, Copy)]
enum AgentOperation {
    ReadConversation,
    ReadInputRequests,
    SendMessage,
    DecideInputRequest,
}

impl AgentOperation {
    const fn action(self) -> AgentAction {
        match self {
            Self::ReadConversation | Self::ReadInputRequests => AgentAction::AgentsRead,
            Self::SendMessage => AgentAction::AgentsMessage,
            Self::DecideInputRequest => AgentAction::AgentsInputRequestAnswer,
        }
    }

    const fn audit_operation(self) -> AdministrativeOperation {
        match self {
            Self::ReadConversation => AdministrativeOperation::AgentConversation,
            Self::ReadInputRequests => AdministrativeOperation::AgentInputRequests,
            Self::SendMessage => AdministrativeOperation::AgentMessage,
            Self::DecideInputRequest => AdministrativeOperation::AgentInputDecision,
        }
    }

    const fn failure(self) -> AdminOperationFailure {
        match self {
            Self::ReadConversation => AdminOperationFailure::AgentConversation,
            Self::ReadInputRequests | Self::DecideInputRequest => {
                AdminOperationFailure::AgentInputRequest
            }
            Self::SendMessage => AdminOperationFailure::AgentMessage,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::ReadConversation => "read_agent_conversation",
            Self::ReadInputRequests => "list_agent_input_requests",
            Self::SendMessage => "send_agent_message",
            Self::DecideInputRequest => "decide_agent_input_request",
        }
    }
}

struct AuthorizedAgentOperation {
    profile: GatewayProfile,
    subject: AuthenticatedSubject,
    target: AgentControlTarget,
}

pub(super) async fn read_agent_conversation(
    State(state): State<AgentManagementState>,
    AxumPath((profile, agent_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let operation = AgentOperation::ReadConversation;
    let context =
        match authorize_agent_operation(&state, profile, agent_id, subject, operation, started_at)
            .await
        {
            Ok(context) => context,
            Err(response) => return *response,
        };
    match state.agent_control.conversation(&context.target).await {
        Ok(conversation) => {
            if let Err(error) = record_agent_result(
                &state,
                &context,
                operation,
                started_at,
                AdminOperationStatus::Succeeded,
                None,
            )
            .await
            {
                return internal_error_response(error);
            }
            Json(conversation).into_response()
        }
        Err(error) => handle_runtime_error(&state, &context, operation, started_at, error).await,
    }
}

pub(super) async fn send_agent_message(
    State(state): State<AgentManagementState>,
    AxumPath((profile, agent_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<AgentOperatorMessageRequest>,
) -> Response {
    let started_at = Instant::now();
    let operation = AgentOperation::SendMessage;
    let context =
        match authorize_agent_operation(&state, profile, agent_id, subject, operation, started_at)
            .await
        {
            Ok(context) => context,
            Err(response) => return *response,
        };
    let result = state
        .agent_control
        .send_operator_message(
            &context.target,
            OperatorMessageDraft {
                request_id: request.request_id,
                message: request.message,
                actor_id: context.subject.principal.id.to_string(),
            },
        )
        .await;
    finish_receipt(&state, context, operation, started_at, result).await
}

pub(super) async fn list_agent_input_requests(
    State(state): State<AgentManagementState>,
    AxumPath((profile, agent_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let operation = AgentOperation::ReadInputRequests;
    let context =
        match authorize_agent_operation(&state, profile, agent_id, subject, operation, started_at)
            .await
        {
            Ok(context) => context,
            Err(response) => return *response,
        };
    match state
        .agent_control
        .pending_input_requests(&context.target)
        .await
    {
        Ok(input_requests) => {
            let views = match input_requests
                .into_iter()
                .map(input_request_view)
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(views) => views,
                Err(error) => {
                    return audited_failure(&state, &context, operation, started_at, error).await;
                }
            };
            if let Err(error) = record_agent_result(
                &state,
                &context,
                operation,
                started_at,
                AdminOperationStatus::Succeeded,
                None,
            )
            .await
            {
                return internal_error_response(error);
            }
            Json(views).into_response()
        }
        Err(error) => handle_runtime_error(&state, &context, operation, started_at, error).await,
    }
}

pub(super) async fn decide_agent_input_request(
    State(state): State<AgentManagementState>,
    AxumPath((profile, agent_id, input_request_id)): AxumPath<(String, String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<AgentInputRequestDecision>,
) -> Response {
    let started_at = Instant::now();
    let operation = AgentOperation::DecideInputRequest;
    let Ok(input_request_id) = AgentInputRequestId::from_str(&input_request_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if input_request_id.as_uuid().get_version_num() != 7 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let context =
        match authorize_agent_operation(&state, profile, agent_id, subject, operation, started_at)
            .await
        {
            Ok(context) => context,
            Err(response) => return *response,
        };
    let request_id = request.request_id();
    let (state_value, answer) = match request {
        AgentInputRequestDecision::Accept { content, .. } => {
            let answer = match json_object(content, "content") {
                Ok(answer) => answer,
                Err(error) => {
                    return handle_runtime_error(&state, &context, operation, started_at, error)
                        .await;
                }
            };
            (AgentInputRequestState::Answered, Some(answer))
        }
        AgentInputRequestDecision::Decline { .. } => (AgentInputRequestState::Declined, None),
        AgentInputRequestDecision::Cancel { .. } => (AgentInputRequestState::Cancelled, None),
    };
    let result = state
        .agent_control
        .decide_input_request(
            &context.target,
            InputRequestDecisionDraft {
                request_id,
                input_request_id,
                answer: InputRequestAnswer {
                    state: state_value,
                    answer,
                    answered_by: context.subject.principal.id.to_string(),
                },
            },
        )
        .await;
    finish_receipt(&state, context, operation, started_at, result).await
}

async fn authorize_agent_operation(
    state: &AgentManagementState,
    profile: String,
    agent_id: String,
    subject: AuthenticatedSubject,
    operation: AgentOperation,
    started_at: Instant,
) -> Result<AuthorizedAgentOperation, Box<Response>> {
    let Some(profile_id) = veoveo_mcp_contract::GatewayProfileId::new(profile).ok() else {
        return Err(StatusCode::NOT_FOUND.into_response().into());
    };
    if agent_id.trim().is_empty() || agent_id.len() > 256 || agent_id.chars().any(char::is_control)
    {
        return Err(StatusCode::NOT_FOUND.into_response().into());
    }
    let audit_target = agent_audit_target(&subject, &agent_id)
        .map_err(|error| Box::new(internal_error_response(error)))?;
    let (_catalog, profile, subject) = authorize_gateway_action(
        &state.gateway,
        state.catalog.current(),
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            audit_target: Some(audit_target),
            action: super::authority::policy_action(&state.catalog.current(), operation.action())
                .map_err(|error| Box::new(internal_error_response(error)))?,
            target: PolicyTarget::Gateway,
            operation: operation.audit_operation(),
            started_at,
        },
    )
    .await?;
    let target = AgentControlTarget {
        tenant_key: subject.authority.tenant.to_string(),
        work_context_key: subject.authority.work_context.to_string(),
        agent_key: agent_id,
    };
    let context = AuthorizedAgentOperation {
        profile,
        subject,
        target,
    };
    Ok(context)
}

fn input_request_view(
    value: GovernedInputRequest,
) -> Result<AgentInputRequestView, serde_json::Error> {
    Ok(AgentInputRequestView {
        input_request_id: value.input_request_id.as_uuid(),
        message: value.message,
        requested_schema: value
            .requested_schema
            .map(serde_json::to_value)
            .transpose()?,
        requested_at: value.requested_at,
    })
}

async fn finish_receipt(
    state: &AgentManagementState,
    context: AuthorizedAgentOperation,
    operation: AgentOperation,
    started_at: Instant,
    result: Result<AgentControlReceipt, AgentRuntimeError>,
) -> Response {
    match result {
        Ok(receipt) => {
            if let Err(error) = record_agent_result(
                state,
                &context,
                operation,
                started_at,
                AdminOperationStatus::Succeeded,
                None,
            )
            .await
            {
                return internal_error_response(error);
            }
            Json(AgentWakeReceipt {
                request_id: receipt.request_id,
                wake_id: receipt.wake_id.as_uuid(),
                agent_id: receipt.agent_key,
                work_context: receipt.work_context_key,
                accepted_at: receipt.accepted_at,
            })
            .into_response()
        }
        Err(error) => handle_runtime_error(state, &context, operation, started_at, error).await,
    }
}

async fn handle_runtime_error(
    state: &AgentManagementState,
    context: &AuthorizedAgentOperation,
    operation: AgentOperation,
    started_at: Instant,
    error: AgentRuntimeError,
) -> Response {
    let status = match error {
        AgentRuntimeError::InvalidField { .. } => StatusCode::BAD_REQUEST,
        AgentRuntimeError::NotFound { .. } => StatusCode::NOT_FOUND,
        AgentRuntimeError::Conflict { .. } | AgentRuntimeError::AgentConflict(_) => {
            StatusCode::CONFLICT
        }
        _ => return audited_failure(state, context, operation, started_at, error).await,
    };
    if let Err(audit_error) = record_agent_result(
        state,
        context,
        operation,
        started_at,
        AdminOperationStatus::Rejected,
        Some(operation.failure()),
    )
    .await
    {
        return internal_error_response(audit_error);
    }
    status.into_response()
}

async fn audited_failure(
    state: &AgentManagementState,
    context: &AuthorizedAgentOperation,
    operation: AgentOperation,
    started_at: Instant,
    error: impl std::fmt::Display,
) -> Response {
    tracing::error!(
        operation = operation.name(),
        "agent control operation failed: {error}"
    );
    if let Err(audit_error) = record_agent_result(
        state,
        context,
        operation,
        started_at,
        AdminOperationStatus::Failed,
        Some(operation.failure()),
    )
    .await
    {
        return internal_error_response(audit_error);
    }
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

async fn record_agent_result(
    state: &AgentManagementState,
    context: &AuthorizedAgentOperation,
    operation: AgentOperation,
    started_at: Instant,
    status: AdminOperationStatus,
    failure: Option<AdminOperationFailure>,
) -> anyhow::Result<()> {
    record_gateway_operation_audit(
        &state.gateway,
        &context.profile,
        &context.subject,
        &state.catalog.current(),
        PolicyTarget::Gateway,
        AdminOperationAuditRecord {
            audit_target: Some(agent_audit_target(
                &context.subject,
                &context.target.agent_key,
            )?),
            action: super::authority::policy_action(&state.catalog.current(), operation.action())?,
            operation: operation.audit_operation(),
            started_at,
            status,
            failure,
        },
    )
    .await
}

fn agent_audit_target(
    subject: &AuthenticatedSubject,
    agent: &str,
) -> anyhow::Result<veoveo_mcp_contract::audit::AuditTarget> {
    use veoveo_types::{ResourceUriBuilder, UriSegment};
    let uri = ResourceUriBuilder::new("veoveo://agents")?
        .segment(UriSegment::new(subject.authority.tenant.to_string())?)
        .segment(UriSegment::new(subject.authority.work_context.to_string())?)
        .segment(UriSegment::new(agent)?)
        .build()?;
    Ok(veoveo_mcp_contract::audit::AuditTarget::PlatformResource { uri })
}
