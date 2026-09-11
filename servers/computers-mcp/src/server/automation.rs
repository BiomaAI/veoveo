//! Console projection over the same grant commands used by MCP.
use super::http_error::{HttpError, actor};
use crate::Application;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use std::sync::Arc;
use uuid::Uuid;
use veoveo_computers::{ComputerError, api::*};
use veoveo_mcp_contract::GatewayInternalIdentity;

async fn inventory(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
) -> Result<Json<AutomationGrantCollection>, HttpError> {
    Ok(Json(app.automation_grants(&actor(&identity)?, id).await?))
}
async fn issue(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
    Json(input): Json<IssueAutomationGrantInput>,
) -> Result<Json<AutomationGrantResult>, HttpError> {
    if input.computer_id != id {
        return Err(crate::ApplicationError::Domain(ComputerError::InvalidInput).into());
    }
    Ok(Json(app.grant_automation(&actor(&identity)?, input).await?))
}
async fn read(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path((computer, grant)): Path<(Uuid, Uuid)>,
) -> Result<Json<AutomationGrantResult>, HttpError> {
    Ok(Json(
        app.automation_grant(&actor(&identity)?, computer, grant)
            .await?,
    ))
}
async fn revoke(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path((computer_id, grant_id)): Path<(Uuid, Uuid)>,
    Json(_input): Json<RevokeAutomationGrantBody>,
) -> Result<Json<AutomationGrantResult>, HttpError> {
    Ok(Json(
        app.revoke_automation(
            &actor(&identity)?,
            RevokeAutomationGrantInput {
                computer_id,
                grant_id,
            },
        )
        .await?,
    ))
}
pub(super) fn router(app: Arc<Application>) -> Router {
    Router::new()
        .route("/computers/{id}/automation", get(inventory).post(issue))
        .route("/computers/{id}/automation/{grant_id}", get(read))
        .route("/computers/{id}/automation/{grant_id}/revoke", post(revoke))
        .with_state(app)
}
