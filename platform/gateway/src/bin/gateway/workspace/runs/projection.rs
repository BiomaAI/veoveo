use super::super::projection::uuid;
use axum::http::StatusCode;
use veoveo_mcp_contract::workspace as wire;
use veoveo_platform_store::workspace as stored;

pub(super) fn agent(value: stored::WorkspaceAgent) -> Result<wire::ChatAgent, StatusCode> {
    Ok(wire::ChatAgent {
        id: wire::AgentId(uuid(&value.id)?),
        definition: value.definition,
        name: value.display_name,
        provider: value.provider,
        model: value.model,
        active: value.active,
        revision: veoveo_mcp_contract::Sha256Digest::from_hex(&value.definition_digest)
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?,
    })
}
pub(super) fn run(value: stored::WorkspaceRun) -> Result<wire::Run, StatusCode> {
    use stored::{WorkspaceRunFailure as F, WorkspaceRunPhase as P, WorkspaceRunState as S};
    Ok(wire::Run {
        id: wire::RunId(uuid(&value.id)?),
        agent: wire::AgentId(uuid(&value.agent)?),
        initiator: wire::PersonId(uuid(&value.initiator)?),
        trigger: wire::MessageId(uuid(&value.trigger)?),
        state: match value.state {
            S::Queued => wire::RunState::Queued,
            S::Running => wire::RunState::Running,
            S::Completed => wire::RunState::Completed,
            S::Cancelled => wire::RunState::Cancelled,
            S::Interrupted => wire::RunState::Interrupted,
            S::Failed => wire::RunState::Failed,
        },
        text: value.text,
        feedback: wire::RunFeedback {
            phase: match value.feedback.phase {
                P::Preparing => wire::RunPhase::Preparing,
                P::Responding => wire::RunPhase::Responding,
                P::CallingTools => wire::RunPhase::CallingTools,
            },
            operations: value.feedback.operations,
        },
        failure: value.failure.map(|value| match value {
            F::Capacity => wire::RunFailure::Capacity,
            F::ModelUnavailable => wire::RunFailure::ModelUnavailable,
            F::PermissionChanged => wire::RunFailure::PermissionChanged,
            F::OutputLimit => wire::RunFailure::OutputLimit,
            F::Deadline => wire::RunFailure::Deadline,
            F::WorkerLost => wire::RunFailure::WorkerLost,
        }),
        sequence: value.sequence,
        updated_sequence: value.updated_sequence,
        created_at: value.created_at,
    })
}
