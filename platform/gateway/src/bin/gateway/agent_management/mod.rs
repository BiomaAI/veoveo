//! Authoring and publication compose existing policy, store and native discovery.
pub(crate) mod authority;
mod commands;
mod events;
pub(crate) mod execution;
pub(crate) mod import;
mod instances;
pub(crate) mod models;
mod projection;
#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod tests_instances;
#[cfg(test)]
mod tests_templates;
mod validation;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State},
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    routing::{get, post},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tower_http::set_header::SetResponseHeaderLayer;
use veoveo_mcp_contract::{GatewayAction as Action, agent_management as wire};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalogHandle, GatewayState};
use veoveo_platform_store::{PlatformStore, agent_management as domain};

use crate::workspace::operations::OperationState;

#[derive(Clone)]
pub(crate) struct AgentManagementState {
    pub gateway: GatewayState,
    pub catalog: GatewayCatalogHandle,
    pub models: Arc<Vec<models::ModelConnection>>,
    pub operations: OperationState,
    pub stop: CancellationToken,
    pub definition_limit: u32,
    pub instance_limits: domain::instances::ManagedAgentLimits,
}

impl AgentManagementState {
    pub fn store(&self) -> &PlatformStore {
        self.gateway.platform_store()
    }
}

#[derive(Debug)]
pub(crate) enum Fault {
    Status(StatusCode),
    Response(Box<axum::response::Response>),
    Validation(wire::Validation),
}
impl Fault {
    pub(crate) fn code(&self) -> StatusCode {
        match self {
            Self::Status(code) => *code,
            Self::Response(response) => response.status(),
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    fn status(status: StatusCode) -> Self {
        Self::Status(status)
    }
    fn unavailable() -> Self {
        Self::Status(StatusCode::SERVICE_UNAVAILABLE)
    }
}
impl From<domain::AgentManagementError> for Fault {
    fn from(error: domain::AgentManagementError) -> Self {
        Self::status(match error {
            domain::AgentManagementError::Invalid(_) => StatusCode::BAD_REQUEST,
            domain::AgentManagementError::NotFound => StatusCode::NOT_FOUND,
            domain::AgentManagementError::Forbidden => StatusCode::FORBIDDEN,
            domain::AgentManagementError::Conflict => StatusCode::CONFLICT,
            domain::AgentManagementError::Capacity => StatusCode::TOO_MANY_REQUESTS,
            domain::AgentManagementError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        })
    }
}
type Api<T> = Result<Json<T>, Fault>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<String>,
    #[serde(default = "page_size")]
    limit: u32,
}
fn page_size() -> u32 {
    50
}

pub(crate) fn router(state: AgentManagementState) -> Router {
    Router::new()
        .route(
            "/admin/{profile}/agent-definitions",
            get(list).post(commands::create),
        )
        .route(
            "/admin/{profile}/agent-definitions/{id}",
            get(read).patch(commands::metadata),
        )
        .route(
            "/admin/{profile}/agent-definitions/{id}/draft",
            get(draft).put(commands::draft),
        )
        .route(
            "/admin/{profile}/agent-definitions/{id}/revisions",
            get(revisions),
        )
        .route(
            "/admin/{profile}/agent-definitions/{id}/validate",
            post(validation::validate),
        )
        .route(
            "/admin/{profile}/agent-definitions/{id}/publish",
            post(commands::publish),
        )
        .route(
            "/admin/{profile}/agent-definitions/{id}/disable",
            post(commands::disable),
        )
        .route(
            "/admin/{profile}/agent-definitions/{id}/enable",
            post(commands::enable),
        )
        .route(
            "/admin/{profile}/agent-definitions/{id}/archive",
            post(commands::archive),
        )
        .route("/admin/{profile}/agent-authoring", get(authoring))
        .route("/admin/{profile}/agent-models", get(model_choices))
        .route("/admin/{profile}/agent-templates", get(template_choices))
        .route(
            "/admin/{profile}/agent-capabilities",
            get(validation::capabilities),
        )
        .merge(instances::router())
        .merge(events::router(state.clone()))
        .layer(DefaultBodyLimit::max(80 * 1024))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(state)
}

async fn list(
    State(state): State<AgentManagementState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(page): Query<Page>,
) -> Api<wire::DefinitionPage> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsRead).await?;
    let values = state
        .store()
        .agent_definitions(&actor.authority, page.after.as_deref(), page.limit)
        .await?;
    let next = if values.len() == page.limit as usize {
        values
            .last()
            .map(|d| wire::AgentDefinitionId::new(d.key.clone()).map_err(|_| Fault::unavailable()))
            .transpose()?
    } else {
        None
    };
    Ok(Json(wire::DefinitionPage {
        items: projection::definitions(&state, &actor, values).await?,
        next,
    }))
}

async fn read(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::Definition> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsRead).await?;
    let value = state
        .store()
        .agent_definition(&actor.authority, &id)
        .await?;
    let value = projection::definitions(&state, &actor, vec![value])
        .await?
        .pop()
        .ok_or_else(Fault::unavailable)?;
    Ok(Json(value))
}

async fn draft(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::Draft> {
    let actor = authority::admit(
        &state,
        profile,
        subject,
        Action::AgentDefinitionsReadContent,
    )
    .await?;
    let value = state
        .store()
        .agent_definition(&actor.authority, &id)
        .await?;
    Ok(Json(wire::Draft {
        definition: wire::AgentDefinitionId::new(id)
            .map_err(|_| Fault::status(StatusCode::NOT_FOUND))?,
        revision: value.revision,
        content: projection::public_content(value.draft)?,
    }))
}

async fn revisions(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(page): Query<Page>,
) -> Api<wire::RevisionPage> {
    let actor = authority::admit(
        &state,
        profile,
        subject,
        Action::AgentDefinitionsReadContent,
    )
    .await?;
    let after = page
        .after
        .map(|v| {
            veoveo_mcp_contract::Sha256Digest::parse(v)
                .map_err(|_| Fault::status(StatusCode::BAD_REQUEST))
        })
        .transpose()?;
    let values = state
        .store()
        .agent_revision_history(
            &actor.authority,
            &id,
            after.as_ref().map(|v| v.hex()),
            page.limit,
        )
        .await?;
    let definition =
        wire::AgentDefinitionId::new(id).map_err(|_| Fault::status(StatusCode::NOT_FOUND))?;
    let items = values
        .into_iter()
        .map(|v| {
            Ok(wire::PublishedRevision {
                definition: definition.clone(),
                digest: projection::digest(&v.digest)?,
                content: projection::public_content(v.content)?,
                created_by: projection::uuid(&v.created_by)?,
                created_at: v.created_at,
            })
        })
        .collect::<Result<Vec<_>, Fault>>()?;
    let next = (items.len() == page.limit as usize)
        .then(|| items.last().map(|v| v.digest.clone()))
        .flatten();
    Ok(Json(wire::RevisionPage { items, next }))
}

async fn authoring(
    State(state): State<AgentManagementState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::Authoring> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsRead).await?;
    let allowed = |action| {
        (action == Action::AgentDefinitionsReadContent
            || actor.authority.membership
                != veoveo_platform_store::WorkContextMembershipLevel::Viewer)
            && authority::allowed(&actor.catalog, &actor.profile.id, &actor.subject, action)
    };
    Ok(Json(wire::Authoring {
        work_context: actor.subject.authority.work_context.clone(),
        definition_limit: state.definition_limit,
        instance_limit: state.instance_limits.instances,
        storage_limit_gib: state.instance_limits.storage_gib,
        models: state
            .models
            .iter()
            .filter(|m| m.permits(&actor.subject, &actor.subject.authority.work_context))
            .map(models::ModelConnection::public)
            .collect(),
        permissions: wire::AuthoringPermissions {
            read_content: allowed(Action::AgentDefinitionsReadContent),
            create: allowed(Action::AgentDefinitionsCreate),
            edit: allowed(Action::AgentDefinitionsEdit),
            publish: allowed(Action::AgentDefinitionsPublish),
            control: allowed(Action::AgentDefinitionsControl),
            archive: allowed(Action::AgentDefinitionsArchive),
            transfer: allowed(Action::AgentDefinitionsTransfer),
            deploy: allowed(Action::AgentInstancesDeploy),
            instance_control: allowed(Action::AgentInstancesControl),
            manage_context: actor.authority.manage_context,
        },
    }))
}
async fn model_choices(
    State(state): State<AgentManagementState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<Vec<wire::ModelChoice>> {
    let result = authoring(State(state), Path(profile), Extension(subject)).await?;
    Ok(Json(result.0.models))
}

async fn template_choices(
    State(state): State<AgentManagementState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<Vec<wire::TemplateChoice>> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsRead).await?;
    Ok(Json(state.gateway.managed_templates().choices(
        &actor.subject.principal,
        &actor.subject.authority.work_context,
    )))
}

pub(crate) fn instance_limits() -> anyhow::Result<domain::instances::ManagedAgentLimits> {
    instances::limits()
}
