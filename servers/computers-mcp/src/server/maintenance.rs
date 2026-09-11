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
    let operation = app.update_template(&actor(&identity)?, input).await?;
    let view = crate::application::maintenance::view(&operation);
    let status = if matches!(
        view.phase,
        MaintenancePhase::Succeeded
            | MaintenancePhase::Cancelled
            | MaintenancePhase::RecoveryRequired
    ) {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((status, Json(view)))
}
pub(super) fn router(app: Arc<Application>) -> Router {
    Router::new()
        .route("/computers/{id}/maintenance", get(state))
        .route("/computers/{id}/maintenance/{operation_id}", get(operation))
        .route("/computers/{id}/update-template", post(update))
        .with_state(app)
}
