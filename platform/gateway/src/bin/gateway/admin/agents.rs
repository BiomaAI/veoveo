use std::{collections::BTreeMap, str::FromStr, time::Instant};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use veoveo_agent_runtime::{
    AgentControlReceipt, AgentControlTarget, AgentRuntimeError, ElicitationAnswer,
    ElicitationDecisionDraft, GovernedElicitation, OperatorMessageDraft, json_object,
};
use veoveo_mcp_contract::{
    AgentElicitationDecisionRequest, AgentElicitationView, AgentOperatorMessageRequest,
    AgentWakeReceipt, GatewayAction, GatewayProfile, PolicyTarget, PrincipalKind,
};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{AgentElicitationId, AgentElicitationState};

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminAuthorizationRequest, AdminOperationAuditRecord, AdminOperationFailure,
        AdminOperationStatus, authorize_admin_target_request, internal_error_response,
        record_admin_operation_audit,
    },
    runtime::AdminState,
};

const AGENT_MESSAGE_METHOD: &str = "admin/agents/messages";
const AGENT_MESSAGE_RESULT_METHOD: &str = "admin/agents/messages/result";
const AGENT_ELICITATIONS_METHOD: &str = "admin/agents/elicitations";
const AGENT_ELICITATIONS_RESULT_METHOD: &str = "admin/agents/elicitations/result";
const AGENT_ELICITATION_DECISION_METHOD: &str = "admin/agents/elicitations/decision";
const AGENT_ELICITATION_DECISION_RESULT_METHOD: &str = "admin/agents/elicitations/decision/result";

#[derive(Clone, Copy)]
enum AgentOperation {
    ReadElicitations,
    SendMessage,
    DecideElicitation,
}

impl AgentOperation {
    const fn action(self) -> GatewayAction {
        match self {
            Self::ReadElicitations => GatewayAction::AgentsRead,
            Self::SendMessage => GatewayAction::AgentsMessage,
            Self::DecideElicitation => GatewayAction::AgentsElicitationAnswer,
        }
    }

    const fn method(self) -> &'static str {
        match self {
            Self::ReadElicitations => AGENT_ELICITATIONS_METHOD,
            Self::SendMessage => AGENT_MESSAGE_METHOD,
            Self::DecideElicitation => AGENT_ELICITATION_DECISION_METHOD,
        }
    }

    const fn result_method(self) -> &'static str {
        match self {
            Self::ReadElicitations => AGENT_ELICITATIONS_RESULT_METHOD,
            Self::SendMessage => AGENT_MESSAGE_RESULT_METHOD,
            Self::DecideElicitation => AGENT_ELICITATION_DECISION_RESULT_METHOD,
        }
    }

    const fn failure(self) -> AdminOperationFailure {
        match self {
            Self::ReadElicitations | Self::DecideElicitation => {
                AdminOperationFailure::AgentElicitation
            }
            Self::SendMessage => AdminOperationFailure::AgentMessage,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::ReadElicitations => "list_agent_elicitations",
            Self::SendMessage => "send_agent_message",
            Self::DecideElicitation => "decide_agent_elicitation",
        }
    }
}

struct AuthorizedAgentOperation {
    profile: GatewayProfile,
    subject: AuthenticatedSubject,
    target: AgentControlTarget,
    metadata: BTreeMap<String, String>,
}

pub(crate) async fn send_agent_message(
    State(state): State<AdminState>,
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
            Err(response) => return response,
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

pub(crate) async fn list_agent_elicitations(
    State(state): State<AdminState>,
    AxumPath((profile, agent_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let operation = AgentOperation::ReadElicitations;
    let context =
        match authorize_agent_operation(&state, profile, agent_id, subject, operation, started_at)
            .await
        {
            Ok(context) => context,
            Err(response) => return response,
        };
    match state
        .agent_control
        .parked_elicitations(&context.target)
        .await
    {
        Ok(elicitations) => {
            let views = match elicitations
                .into_iter()
                .map(elicitation_view)
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

pub(crate) async fn decide_agent_elicitation(
    State(state): State<AdminState>,
    AxumPath((profile, agent_id, elicitation_id)): AxumPath<(String, String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<AgentElicitationDecisionRequest>,
) -> Response {
    let started_at = Instant::now();
    let operation = AgentOperation::DecideElicitation;
    let Ok(elicitation_id) = AgentElicitationId::from_str(&elicitation_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if elicitation_id.as_uuid().get_version_num() != 7 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let context =
        match authorize_agent_operation(&state, profile, agent_id, subject, operation, started_at)
            .await
        {
            Ok(context) => context,
            Err(response) => return response,
        };
    let request_id = request.request_id();
    let (state_value, answer) = match request {
        AgentElicitationDecisionRequest::Accept { content, .. } => {
            let answer = match json_object(content, "content") {
                Ok(answer) => answer,
                Err(error) => {
                    return handle_runtime_error(&state, &context, operation, started_at, error)
                        .await;
                }
            };
            (AgentElicitationState::Answered, Some(answer))
        }
        AgentElicitationDecisionRequest::Decline { .. } => (AgentElicitationState::Declined, None),
        AgentElicitationDecisionRequest::Cancel { .. } => (AgentElicitationState::Cancelled, None),
    };
    let result = state
        .agent_control
        .decide_elicitation(
            &context.target,
            ElicitationDecisionDraft {
                request_id,
                elicitation_id,
                answer: ElicitationAnswer {
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
    state: &AdminState,
    profile: String,
    agent_id: String,
    subject: AuthenticatedSubject,
    operation: AgentOperation,
    started_at: Instant,
) -> Result<AuthorizedAgentOperation, Response> {
    let Some(profile_id) = admin_profile_id(profile) else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };
    if agent_id.trim().is_empty() || agent_id.len() > 256 || agent_id.chars().any(char::is_control)
    {
        return Err(StatusCode::NOT_FOUND.into_response());
    }
    let metadata = BTreeMap::from([
        ("operation".to_owned(), operation.name().to_owned()),
        ("agent_id".to_owned(), agent_id.clone()),
    ]);
    let (_catalog, profile, subject) = authorize_admin_target_request(
        state,
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            action: operation.action(),
            target: PolicyTarget::Gateway,
            method: operation.method(),
            metadata: metadata.clone(),
            started_at,
        },
    )
    .await
    .map_err(|response| *response)?;
    let target = AgentControlTarget {
        tenant_key: subject.authority.tenant.to_string(),
        work_context_key: subject.authority.work_context.to_string(),
        profile: profile_id.to_string(),
        agent_key: agent_id,
    };
    let context = AuthorizedAgentOperation {
        profile,
        subject,
        target,
        metadata,
    };
    if context.subject.principal.kind != PrincipalKind::User
        || context.subject.actor.kind != PrincipalKind::User
    {
        if let Err(error) = record_agent_result(
            state,
            &context,
            operation,
            started_at,
            AdminOperationStatus::Rejected,
            Some(AdminOperationFailure::AgentCaller),
        )
        .await
        {
            return Err(internal_error_response(error));
        }
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    Ok(context)
}

fn elicitation_view(value: GovernedElicitation) -> Result<AgentElicitationView, serde_json::Error> {
    Ok(AgentElicitationView {
        elicitation_id: value.elicitation_id.as_uuid(),
        message: value.message,
        requested_schema: value
            .requested_schema
            .map(serde_json::to_value)
            .transpose()?,
        requested_at: value.requested_at,
    })
}

async fn finish_receipt(
    state: &AdminState,
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
    state: &AdminState,
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
    state: &AdminState,
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
    state: &AdminState,
    context: &AuthorizedAgentOperation,
    operation: AgentOperation,
    started_at: Instant,
    status: AdminOperationStatus,
    failure: Option<AdminOperationFailure>,
) -> anyhow::Result<()> {
    record_admin_operation_audit(
        state,
        &context.profile,
        &context.subject,
        AdminOperationAuditRecord {
            action: operation.action(),
            method: operation.result_method(),
            started_at,
            status,
            failure,
            metadata: context.metadata.clone(),
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_control_methods_are_valid_audit_methods() {
        for method in [
            AGENT_MESSAGE_METHOD,
            AGENT_MESSAGE_RESULT_METHOD,
            AGENT_ELICITATIONS_METHOD,
            AGENT_ELICITATIONS_RESULT_METHOD,
            AGENT_ELICITATION_DECISION_METHOD,
            AGENT_ELICITATION_DECISION_RESULT_METHOD,
        ] {
            assert!(veoveo_mcp_contract::McpMethodName::new(method).is_ok());
        }
    }
}
