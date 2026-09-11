use super::{
    auth::ForwardedBearer,
    http_error::{HttpError, actor},
};
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
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};

async fn transfer(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Extension(bearer): Extension<ForwardedBearer>,
    Path(computer): Path<Uuid>,
    Json(input): Json<TransferFileInput>,
) -> Result<(StatusCode, Json<FileTransferView>), HttpError> {
    if input.computer_id != computer {
        return Err(crate::ApplicationError::Domain(ComputerError::InvalidInput).into());
    }
    let caller = PlaneCaller {
        memberships: identity.actor.group_memberships(),
        identity,
        bearer_token: bearer.0,
    };
    let operation = app.transfer_file(&caller, input).await?;
    let view = app
        .file_transfer(
            &actor(&caller.identity)?,
            computer,
            operation.transfer_id(),
            false,
        )
        .await?;
    let status = if view.completed_at.is_some() {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((status, Json(view)))
}

async fn state(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path((computer, transfer)): Path<(Uuid, Uuid)>,
) -> Result<Json<FileTransferView>, HttpError> {
    Ok(Json(
        app.file_transfer(&actor(&identity)?, computer, transfer, false)
            .await?,
    ))
}

async fn cancel(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path((computer, transfer)): Path<(Uuid, Uuid)>,
    Json(_body): Json<CancelFileTransferBody>,
) -> Result<Json<FileTransferView>, HttpError> {
    Ok(Json(
        app.file_transfer(&actor(&identity)?, computer, transfer, true)
            .await?,
    ))
}

pub(super) fn router(app: Arc<Application>) -> Router {
    Router::new()
        .route("/computers/{id}/files", post(transfer))
        .route("/computers/{id}/files/{transfer_id}", get(state))
        .route("/computers/{id}/files/{transfer_id}/cancel", post(cancel))
        .with_state(app)
}
