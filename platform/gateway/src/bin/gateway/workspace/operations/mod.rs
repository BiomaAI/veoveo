mod apps;
mod commands;
mod events;
mod inputs;
mod native;
mod personal;
mod progress;
mod projection;
#[cfg(test)]
pub(super) mod test_domain;
#[cfg(test)]
mod tests;

use super::{Api, WorkspaceState, authority, fault};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::CACHE_CONTROL},
    routing::{get, post},
};
use rmcp::model::{GetTaskParams, PaginatedRequestParams};
use secrecy::SecretString;
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tower_http::set_header::SetResponseHeaderLayer;
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayDiscoveryDegradation, GatewayDiscoverySurface, GatewayProfileId, GatewayToolName,
    workspace as wire,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalogHandle, GatewayState};
use veoveo_platform_store::{
    PlatformStore, WorkspaceChatId, WorkspaceOperationId,
    workspace::{WorkspaceAuthority, WorkspaceOperation, WorkspaceOperationPhase as Phase},
};

#[derive(Clone)]
pub(crate) struct OperationState {
    workspace: WorkspaceState,
    gateway: GatewayState,
    catalog: GatewayCatalogHandle,
    native: native::NativeTransport,
    limits: Arc<Semaphore>,
    watches: Arc<crate::stream_limits::Limits>,
    stop: CancellationToken,
    personal: personal::PersonalHub,
}

/// Credentials remain in memory and always represent the initiating human.
#[derive(Clone)]
pub(crate) struct Caller {
    pub profile: GatewayProfileId,
    pub subject: AuthenticatedSubject,
    pub bearer: SecretString,
}

impl Caller {
    pub fn new(
        profile: GatewayProfileId,
        subject: AuthenticatedSubject,
        headers: &HeaderMap,
    ) -> Result<Self, StatusCode> {
        Ok(Self {
            profile,
            subject,
            bearer: native::bearer(headers)?,
        })
    }
}

impl OperationState {
    pub async fn capabilities(
        &self,
        caller: &Caller,
        required: &[GatewayToolName],
    ) -> Result<Vec<rmcp::model::Tool>, StatusCode> {
        self.authority(&caller.subject, &caller.profile).await?;
        self.authoring_capabilities(caller, required).await
    }

    /// The management caller supplies current action/context admission. Native
    /// discovery independently authenticates this caller's bearer and profile.
    pub(crate) async fn authoring_capabilities(
        &self,
        caller: &Caller,
        required: &[GatewayToolName],
    ) -> Result<Vec<rmcp::model::Tool>, StatusCode> {
        let client = self.native.connect(&caller.profile, &caller.bearer).await?;
        let result = catalog_tools(&client, Some(required)).await;
        client.close().await;
        result
    }

    pub async fn submit(
        &self,
        caller: Caller,
        id: WorkspaceOperationId,
        intent: veoveo_platform_store::workspace::WorkspaceOperationIntent,
    ) -> Result<wire::OperationSummary, StatusCode> {
        commands::submit(self.clone(), caller, id, intent, true).await
    }
    pub fn new(
        store: PlatformStore,
        gateway: GatewayState,
        catalog: GatewayCatalogHandle,
        stop: CancellationToken,
        port: u16,
        public_base: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            personal: personal::PersonalHub::new(store.clone(), stop.clone()),
            workspace: WorkspaceState { store },
            gateway,
            catalog,
            native: native::NativeTransport::new(port, public_base)?,
            limits: Arc::new(Semaphore::new(16)),
            watches: crate::stream_limits::Limits::new(64),
            stop,
        })
    }
    async fn authority(
        &self,
        subject: &AuthenticatedSubject,
        profile: &GatewayProfileId,
    ) -> Result<WorkspaceAuthority, StatusCode> {
        authority::admit_live(
            &self.workspace,
            &self.gateway,
            &self.catalog.current(),
            profile,
            subject,
        )
        .await
    }
    async fn operation(
        &self,
        subject: &AuthenticatedSubject,
        profile: &GatewayProfileId,
        id: Uuid,
    ) -> Result<WorkspaceOperation, StatusCode> {
        let authority = self.authority(subject, profile).await?;
        let operation = self
            .workspace
            .store
            .workspace_operation(&authority, WorkspaceOperationId::from_uuid(id))
            .await
            .map_err(fault)?;
        if operation.profile != profile.as_str() {
            return Err(StatusCode::NOT_FOUND);
        }
        Ok(operation)
    }
}

pub(crate) fn router(state: OperationState) -> Router {
    Router::new()
        .route("/workspace-api/{profile}/events", get(personal::events))
        .merge(apps::router())
        .route("/workspace-api/{profile}/capabilities", get(capabilities))
        .route("/workspace-api/{profile}/operations", get(list))
        .route(
            "/workspace-api/{profile}/operations/events",
            get(events::watch),
        )
        .route(
            "/workspace-api/{profile}/chats/{chat}/operations",
            post(commands::start),
        )
        .route("/workspace-api/{profile}/operations/{id}", get(detail))
        .route(
            "/workspace-api/{profile}/operations/{id}/cancel",
            post(commands::cancel),
        )
        .route(
            "/workspace-api/{profile}/operations/{id}/input",
            post(commands::answer),
        )
        .layer(DefaultBodyLimit::max(80 * 1024))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(state)
}

fn profile(value: String) -> Result<GatewayProfileId, StatusCode> {
    GatewayProfileId::new(value).map_err(|_| StatusCode::NOT_FOUND)
}

async fn tools(client: &native::NativeClient) -> Result<Vec<rmcp::model::Tool>, StatusCode> {
    catalog_tools(client, None).await
}

async fn catalog_tools(
    client: &native::NativeClient,
    required: Option<&[GatewayToolName]>,
) -> Result<Vec<rmcp::model::Tool>, StatusCode> {
    let mut result = vec![];
    let mut cursor = None;
    for _ in 0..16 {
        let page = tokio::time::timeout(
            Duration::from_secs(8),
            client.peer().list_tools(
                cursor.map(|cursor| PaginatedRequestParams::default().with_cursor(Some(cursor))),
            ),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error)?;
        let degradation = GatewayDiscoveryDegradation::from_meta(page.meta.as_ref())
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        if required.is_some_and(|required| {
            degradation.failures.iter().any(|failure| {
                failure.surface == GatewayDiscoverySurface::Tools
                    && required.iter().any(|tool| {
                        tool.as_str()
                            .split_once("__")
                            .is_none_or(|(server, _)| server == failure.server.as_str())
                    })
            })
        }) {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        result.extend(page.tools);
        if result.len() > 512 {
            return Err(StatusCode::BAD_GATEWAY);
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(result);
        }
    }
    Err(StatusCode::BAD_GATEWAY)
}

async fn capabilities(
    State(state): State<OperationState>,
    Path(raw_profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
) -> Api<Vec<wire::Capability>> {
    let profile = profile(raw_profile)?;
    state.authority(&subject, &profile).await?;
    let client = state
        .native
        .connect(&profile, &native::bearer(&headers)?)
        .await?;
    let result = tools(&client).await;
    client.close().await;
    Ok(Json(
        result?
            .into_iter()
            .map(|tool| wire::Capability {
                title: tool.title.unwrap_or_else(|| tool.name.to_string()),
                name: tool.name.to_string(),
                description: tool.description.map(|value| value.into_owned()),
                input_schema: serde_json::Value::Object((*tool.input_schema).clone()),
            })
            .collect(),
    ))
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivityQuery {
    chat: Option<Uuid>,
    before: Option<Uuid>,
}
async fn list(
    State(state): State<OperationState>,
    Path(raw_profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(query): Query<ActivityQuery>,
) -> Api<wire::OperationPage> {
    let profile = profile(raw_profile)?;
    let authority = state.authority(&subject, &profile).await?;
    let values = state
        .workspace
        .store
        .workspace_operations(
            &authority,
            query.chat.map(WorkspaceChatId::from_uuid),
            query.before.map(WorkspaceOperationId::from_uuid),
        )
        .await
        .map_err(fault)?;
    let next = if values.len() == 100 {
        values
            .last()
            .map(projection::summary)
            .transpose()?
            .map(|value| value.id)
    } else {
        None
    };
    Ok(Json(wire::OperationPage {
        items: values
            .iter()
            .filter(|value| value.profile == profile.as_str())
            .map(projection::summary)
            .collect::<Result<_, _>>()?,
        next,
    }))
}

async fn detail(
    State(state): State<OperationState>,
    Path((raw_profile, id)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
) -> Api<wire::OperationView> {
    let profile = profile(raw_profile)?;
    let operation = state.operation(&subject, &profile, id).await?;
    let client = state
        .native
        .connect(&profile, &native::bearer(&headers)?)
        .await?;
    let result = view(&operation, &client).await;
    client.close().await;
    Ok(Json(result?))
}

async fn view(
    operation: &WorkspaceOperation,
    client: &native::NativeClient,
) -> Result<wire::OperationView, StatusCode> {
    if operation.phase == Phase::Task {
        let task = tokio::time::timeout(
            Duration::from_secs(8),
            client
                .peer()
                .get_task(GetTaskParams::new(task_id(operation)?)),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error)?;
        projection::task(operation, task.task)
    } else {
        // Stored synchronous results and continuation prompts still require the
        // person's current access to the owning tool, not merely a receipt ID.
        if matches!(operation.phase, Phase::Completed | Phase::InputRequired)
            && !tools(client)
                .await?
                .iter()
                .any(|tool| tool.name == operation.tool)
        {
            return Err(StatusCode::FORBIDDEN);
        }
        projection::stored(operation)
    }
}

fn task_id(operation: &WorkspaceOperation) -> Result<String, StatusCode> {
    operation.task_id.clone().ok_or(StatusCode::CONFLICT)
}
fn mcp_error(error: rmcp::ServiceError) -> StatusCode {
    match error {
        rmcp::ServiceError::McpError(error)
            if error.code == rmcp::model::ErrorCode::INVALID_PARAMS =>
        {
            StatusCode::NOT_FOUND
        }
        rmcp::ServiceError::McpError(error)
            if error.code == rmcp::model::ErrorCode::INVALID_REQUEST =>
        {
            StatusCode::FORBIDDEN
        }
        _ => StatusCode::BAD_GATEWAY,
    }
}
