mod config;
mod keys;
mod messages;
mod projection;
#[cfg(test)]
pub(super) mod tests;
#[cfg(test)]
mod tool_tests;
mod tools;
mod worker;

use super::{
    Api, WorkspaceState, authority, fault,
    operations::{Caller, OperationState},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::CACHE_CONTROL},
    routing::{delete, get, post},
};
use chrono::{TimeDelta, Utc};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tower_http::set_header::SetResponseHeaderLayer;
use uuid::Uuid;
use veoveo_mcp_contract::{GatewayProfileId, workspace as wire};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalogHandle, GatewayState};
use veoveo_platform_store::{
    PlatformStore, WorkspaceAgentId, WorkspaceChatId, WorkspaceMessageId, WorkspaceRunId,
    workspace::WorkspaceRunState,
};

#[derive(Clone)]
struct RunState {
    workspace: WorkspaceState,
    gateway: GatewayState,
    catalog: GatewayCatalogHandle,
    definitions: Arc<Vec<config::Definition>>,
    limits: Arc<Semaphore>,
    stop: CancellationToken,
    http: reqwest::Client,
    keys: keys::ModelKeys,
    operations: OperationState,
}

pub(crate) fn router(
    store: PlatformStore,
    gateway: GatewayState,
    catalog: GatewayCatalogHandle,
    stop: CancellationToken,
    operations: OperationState,
) -> anyhow::Result<Router> {
    let definitions = config::from_env(&catalog.current())?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(125))
        .build()?;
    Ok(routes(RunState {
        workspace: WorkspaceState { store },
        gateway,
        catalog,
        definitions: Arc::new(definitions),
        limits: Arc::new(Semaphore::new(16)),
        stop,
        http,
        keys: keys::ModelKeys::default(),
        operations,
    }))
}

fn routes(state: RunState) -> Router {
    Router::new()
        .route("/workspace-api/{profile}/agents", get(catalog))
        .route("/workspace-api/{profile}/chats/{chat}/agents", post(add))
        .route(
            "/workspace-api/{profile}/chats/{chat}/agents/{agent}",
            delete(remove),
        )
        .route(
            "/workspace-api/{profile}/chats/{chat}/activity",
            get(activity),
        )
        .route("/workspace-api/{profile}/chats/{chat}/runs", post(start))
        .route(
            "/workspace-api/{profile}/chats/{chat}/runs/{run}/cancel",
            post(cancel),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .route(
            "/workspace-api/{profile}/chats/{chat}/messages",
            post(messages::send).layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(state)
}

async fn catalog(
    State(state): State<RunState>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<Vec<wire::AgentDefinition>> {
    authority::admit(&state.workspace, &subject).await?;
    Ok(Json(
        state
            .definitions
            .iter()
            .filter(|definition| definition.permits(&subject))
            .map(config::Definition::public)
            .collect(),
    ))
}
async fn add(
    State(state): State<RunState>,
    Path((_profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::AddAgent>,
) -> Api<wire::ChatAgent> {
    let authority = authority::admit(&state.workspace, &subject).await?;
    let definition = state
        .definitions
        .iter()
        .find(|d| d.id == request.definition && d.permits(&subject))
        .ok_or(StatusCode::NOT_FOUND)?;
    let added = state
        .workspace
        .store
        .add_workspace_agent(
            &authority,
            WorkspaceChatId::from_uuid(chat),
            definition.admission(),
        )
        .await
        .map_err(fault)?;
    Ok(Json(projection::agent(added)?))
}
async fn remove(
    State(state): State<RunState>,
    Path((_profile, chat, agent)): Path<(String, Uuid, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::ChatAgent> {
    let authority = authority::admit(&state.workspace, &subject).await?;
    let removed = state
        .workspace
        .store
        .remove_workspace_agent(
            &authority,
            WorkspaceChatId::from_uuid(chat),
            WorkspaceAgentId::from_uuid(agent),
        )
        .await
        .map_err(fault)?;
    Ok(Json(projection::agent(removed)?))
}
async fn activity(
    State(state): State<RunState>,
    Path((_profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::AgentActivity> {
    let authority = authority::admit(&state.workspace, &subject).await?;
    let chat = WorkspaceChatId::from_uuid(chat);
    let runs = state
        .workspace
        .store
        .workspace_runs(&authority, chat)
        .await
        .map_err(fault)?;
    let agents = state
        .workspace
        .store
        .workspace_agents(&authority, chat)
        .await
        .map_err(fault)?;
    Ok(Json(wire::AgentActivity {
        agents: agents
            .into_iter()
            .map(projection::agent)
            .collect::<Result<_, _>>()?,
        runs: runs
            .into_iter()
            .map(projection::run)
            .collect::<Result<_, _>>()?,
    }))
}
async fn start(
    State(state): State<RunState>,
    Path((profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::StartRun>,
) -> Api<wire::Run> {
    let profile = GatewayProfileId::new(profile).map_err(|_| StatusCode::NOT_FOUND)?;
    let caller = Caller::new(profile.clone(), subject.clone(), &headers)?;
    let authority = authority::admit_live(
        &state.workspace,
        &state.gateway,
        &state.catalog.current(),
        &profile,
        &subject,
    )
    .await?;
    let chat = WorkspaceChatId::from_uuid(chat);
    let agent = WorkspaceAgentId::from_uuid(request.agent.0);
    let agents = state
        .workspace
        .store
        .workspace_agents(&authority, chat)
        .await
        .map_err(fault)?;
    let admitted = agents
        .iter()
        .find(|value| value.id == agent.record_id() && value.active)
        .ok_or(StatusCode::NOT_FOUND)?;
    let definition = state
        .definitions
        .iter()
        .find(|d| d.id == admitted.definition && d.permits(&subject))
        .ok_or(StatusCode::NOT_FOUND)?
        .clone();
    if definition.digest() != admitted.definition_digest {
        return Err(StatusCode::CONFLICT);
    }
    let permit = state
        .limits
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let deadline = subject
        .access_token
        .expires_at
        .min(Utc::now() + TimeDelta::seconds(120));
    let run = state
        .workspace
        .store
        .start_workspace_run(
            &authority,
            chat,
            agent,
            WorkspaceMessageId::from_uuid(request.trigger.0),
            &definition.digest(),
            deadline,
        )
        .await
        .map_err(fault)?;
    let response = projection::run(run.clone())?;
    if run.state == WorkspaceRunState::Queued {
        tokio::spawn(worker::execute(state, caller, definition, run, permit));
    }
    Ok(Json(response))
}
async fn cancel(
    State(state): State<RunState>,
    Path((_profile, chat, run)): Path<(String, Uuid, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::Run> {
    let authority = authority::admit(&state.workspace, &subject).await?;
    let result = state
        .workspace
        .store
        .cancel_workspace_run(
            &authority,
            WorkspaceChatId::from_uuid(chat),
            WorkspaceRunId::from_uuid(run),
        )
        .await
        .map_err(fault)?;
    Ok(Json(projection::run(result)?))
}
