use super::http_error::{HttpError, actor};
use crate::Application;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use std::sync::Arc;
use uuid::Uuid;
use veoveo_computers::{ComputerError, api::*};
use veoveo_mcp_contract::GatewayInternalIdentity;

async fn state(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(computer): Path<Uuid>,
) -> Result<Json<MaintenanceState>, HttpError> {
    Ok(Json(
        app.maintenance_state(&actor(&identity)?, computer).await?,
    ))
}
async fn operation(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path((computer, task)): Path<(Uuid, Uuid)>,
) -> Result<Json<MaintenanceView>, HttpError> {
    Ok(Json(
        app.maintenance_operation(&actor(&identity)?, computer, task)
            .await?,
    ))
}
async fn update(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(computer): Path<Uuid>,
    Json(input): Json<UpdateTemplateInput>,
) -> Result<(StatusCode, Json<MaintenanceView>), HttpError> {
    if input.computer_id != computer {
        return Err(crate::ApplicationError::Domain(ComputerError::InvalidInput).into());
    }
    let actor = actor(&identity)?;
    let operation = app.update_template(&actor, input).await?;
    let authority = app
        .store
        .control_authority(&actor)
        .await
        .map_err(crate::ApplicationError::from)?;
    let view = app.project_maintenance(&authority, &operation).await?;
    Ok((status(&view), Json(view)))
}
async fn resume(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path((computer, task)): Path<(Uuid, Uuid)>,
    Json(input): Json<ResumeUpdateInput>,
) -> Result<(StatusCode, Json<MaintenanceView>), HttpError> {
    if input.computer_id != computer || input.task_id != task {
        return Err(crate::ApplicationError::Domain(ComputerError::InvalidInput).into());
    }
    let actor = actor(&identity)?;
    let operation = app.resume_update(&actor, input).await?;
    let authority = app
        .store
        .control_authority(&actor)
        .await
        .map_err(crate::ApplicationError::from)?;
    let view = app.project_maintenance(&authority, &operation).await?;
    Ok((status(&view), Json(view)))
}
fn status(view: &MaintenanceView) -> StatusCode {
    if matches!(
        view.phase,
        MaintenancePhase::Succeeded
            | MaintenancePhase::Cancelled
            | MaintenancePhase::RecoveryRequired
    ) {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    }
}
pub(super) fn router(app: Arc<Application>) -> Router {
    Router::new()
        .route("/computers/{id}/maintenance", get(state))
        .route("/computers/{id}/maintenance/{operation_id}", get(operation))
        .route(
            "/computers/{id}/maintenance/{operation_id}/resume",
            post(resume),
        )
        .route("/computers/{id}/update-template", post(update))
        .with_state(app)
}
