//! Governed intent admission. Kubernetes writes belong to the lifecycle manager.
mod admission;
mod projection;

use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::get,
};
use uuid::Uuid;
use veoveo_mcp_contract::{GatewayAction as Action, PolicyTarget, agent_management as wire};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{RecordId, agent_management::instances::*};

use super::{
    AgentManagementState, Api, Fault, Page,
    authority::{self, Admission},
};
use crate::audit::{
    AdminOperationAuditRecord, AdminOperationFailure, AdminOperationStatus,
    record_gateway_operation_audit,
};

pub(super) use admission::limits;

pub(super) fn router() -> Router<AgentManagementState> {
    Router::new()
        .route(
            "/admin/{profile}/agent-instances",
            get(list).post(provision),
        )
        .route(
            "/admin/{profile}/agent-instances/{id}",
            get(read).patch(update),
        )
        .route("/admin/{profile}/agent-operations/{id}", get(operation))
}

async fn list(
    State(state): State<AgentManagementState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(page): Query<Page>,
) -> Api<wire::InstancePage> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsRead).await?;
    let values = state
        .store()
        .managed_agents(&actor.authority, page.after.as_deref(), page.limit)
        .await?;
    let items = projection::instances(&state, &actor, values).await?;
    let next = (items.len() == page.limit as usize)
        .then(|| items.last().map(|v| v.id.clone()))
        .flatten();
    Ok(Json(wire::InstancePage { items, next }))
}

async fn read(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::ManagedInstance> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsRead).await?;
    let value = state.store().managed_agent(&actor.authority, &id).await?;
    let result = projection::instances(&state, &actor, vec![value])
        .await?
        .pop()
        .ok_or_else(Fault::unavailable)?;
    Ok(Json(result))
}

async fn operation(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::LifecycleOperation> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsRead).await?;
    let value = state
        .store()
        .managed_agent_operation(
            &actor.authority,
            RecordId::new("managed_agent_operation", surrealdb::types::Uuid::from(id)),
        )
        .await?;
    let instance: Option<ManagedAgentInstance> = state
        .store()
        .client()
        .select(value.instance.clone())
        .await
        .map_err(|_| Fault::unavailable())?;
    Ok(Json(projection::operation(
        &instance.ok_or_else(Fault::unavailable)?.key,
        value,
    )?))
}

async fn finish(
    state: &AgentManagementState,
    actor: &Admission,
    key: &str,
    request_id: Uuid,
    result: Result<ManagedAgentOperation, Fault>,
) -> Result<(StatusCode, Json<wire::LifecycleOperation>), Fault> {
    let result = result.and_then(|value| projection::operation(key, value));
    record_gateway_operation_audit(
        &state.gateway,
        &actor.profile,
        &actor.subject,
        PolicyTarget::Gateway,
        AdminOperationAuditRecord {
            action: actor.action,
            method: "admin/agent-instances/result",
            started_at: actor.started,
            status: match &result {
                Ok(_) => AdminOperationStatus::Succeeded,
                Err(error) if error.code().is_server_error() => AdminOperationStatus::Failed,
                Err(_) => AdminOperationStatus::Rejected,
            },
            failure: result
                .as_ref()
                .err()
                .map(|_| AdminOperationFailure::AgentManagement),
            metadata: BTreeMap::from([
                ("request_id".into(), request_id.to_string()),
                ("instance".into(), key.into()),
            ]),
        },
    )
    .await
    .map_err(|_| Fault::unavailable())?;
    result.map(|value| (StatusCode::ACCEPTED, Json(value)))
}

async fn provision(
    State(state): State<AgentManagementState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::ProvisionInstance>,
) -> Result<(StatusCode, Json<wire::LifecycleOperation>), Fault> {
    let actor = authority::admit(&state, profile, subject, Action::AgentInstancesDeploy).await?;
    let result = admission::provision(&state, &actor, &request).await;
    finish(
        &state,
        &actor,
        request.id.as_str(),
        request.request_id,
        result,
    )
    .await
}

async fn update(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::UpdateInstance>,
) -> Result<(StatusCode, Json<wire::LifecycleOperation>), Fault> {
    let actor = authority::admit(&state, profile, subject, Action::AgentInstancesControl).await?;
    let result = async {
        let instance = state.store().managed_agent(&actor.authority, &id).await?;
        let mut mutation = match request.change {
            wire::InstanceChange::State { desired } => ManagedAgentMutation::State {
                desired: projection::desired(desired),
            },
            wire::InstanceChange::Revision { revision } => ManagedAgentMutation::Revision {
                digest: revision.hex().to_owned(),
                image: instance.resources.image.clone(),
            },
            wire::InstanceChange::Stop => ManagedAgentMutation::Stop,
            wire::InstanceChange::Retry => ManagedAgentMutation::Retry,
        };
        if let Some(receipt) = state
            .store()
            .replay_managed_agent(
                &actor.authority,
                &id,
                request.request_id,
                Some(request.expected_generation),
                &mutation,
            )
            .await?
        {
            return Ok(receipt);
        }
        if matches!(
            mutation,
            ManagedAgentMutation::State {
                desired: ManagedAgentDesired::Running
            } | ManagedAgentMutation::Revision { .. }
                | ManagedAgentMutation::Retry
        ) {
            if !authority::allowed(
                &actor.catalog,
                &actor.profile.id,
                &actor.subject,
                Action::AgentInstancesDeploy,
            ) {
                return Err(Fault::status(StatusCode::FORBIDDEN));
            }
            let projected = projection::instances(&state, &actor, vec![instance.clone()])
                .await?
                .pop()
                .ok_or_else(Fault::unavailable)?;
            let digest = match &mutation {
                ManagedAgentMutation::Revision { digest, .. } => digest.as_str(),
                _ => projected.requested_revision.hex(),
            };
            let template =
                admission::template(&state, &actor, projected.definition.as_str(), digest).await?;
            if let ManagedAgentMutation::Revision { image, .. } = &mut mutation {
                let previous = state
                    .store()
                    .agent_revision(
                        &actor.authority,
                        projected.definition.as_str(),
                        projected.requested_revision.hex(),
                    )
                    .await?;
                *image =
                    admission::revision_image(&previous, &instance.resources.image, &template)?;
            }
        }
        authority::live_session(&state, &actor.profile, &actor.subject).await?;
        Ok(state
            .store()
            .mutate_managed_agent(
                &actor.authority,
                &id,
                request.request_id,
                Some(request.expected_generation),
                mutation,
                state.instance_limits,
            )
            .await?)
    }
    .await;
    finish(&state, &actor, &id, request.request_id, result).await
}
