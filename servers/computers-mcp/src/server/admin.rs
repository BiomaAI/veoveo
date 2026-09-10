use super::http_error::{HttpError, actor};
use super::*;
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;
use veoveo_computers::{OperationStage, api::*};
use veoveo_mcp_contract::GatewayInternalIdentity;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<Uuid>,
}
fn receipt(operation: veoveo_computers::Operation) -> (StatusCode, Json<OperationReceipt>) {
    let status = match operation.stage {
        OperationStage::Queued | OperationStage::Dispatched => StatusCode::ACCEPTED,
        _ => StatusCode::OK,
    };
    (
        status,
        Json(OperationReceipt {
            task_id: operation.operation_id,
            computer_id: operation.computer_id,
            action: operation.action,
            status: match operation.stage {
                OperationStage::Queued => OperationStatus::Queued,
                OperationStage::Dispatched => OperationStatus::Running,
                OperationStage::Succeeded => OperationStatus::Succeeded,
                OperationStage::Failed => OperationStatus::Failed,
                OperationStage::Cancelled => OperationStatus::Cancelled,
                OperationStage::RecoveryRequired => OperationStatus::RecoveryRequired,
            },
        }),
    )
}
async fn collection(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Query(page): Query<Page>,
) -> Result<Json<ComputerSnapshot>, HttpError> {
    Ok(Json(app.snapshot(&actor(&identity)?, page.after).await?))
}
async fn computer(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
) -> Result<Json<ComputerView>, HttpError> {
    Ok(Json(app.computer(&actor(&identity)?, id).await?))
}
async fn create(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<CreateInput>,
) -> Result<(StatusCode, Json<OperationReceipt>), HttpError> {
    Ok(receipt(app.create(actor(&identity)?, request).await?))
}
async fn operation(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path((computer_id, operation_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<OperationReceipt>, HttpError> {
    let (_, receipt) = receipt(
        app.operation(&actor(&identity)?, computer_id, operation_id)
            .await?,
    );
    Ok(receipt)
}
async fn start(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
    Json(request): Json<StartInput>,
) -> Result<(StatusCode, Json<OperationReceipt>), HttpError> {
    Ok(receipt(
        app.lifecycle(
            actor(&identity)?,
            LifecycleInput {
                computer_id: id,
                request_id: request.request_id,
            },
            Action::Start,
        )
        .await?,
    ))
}
async fn stop(
    State(app): State<Arc<Application>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
    Json(request): Json<StopInput>,
) -> Result<(StatusCode, Json<OperationReceipt>), HttpError> {
    Ok(receipt(
        app.lifecycle(
            actor(&identity)?,
            LifecycleInput {
                computer_id: id,
                request_id: request.request_id,
            },
            Action::Stop,
        )
        .await?,
    ))
}
pub fn router(app: Arc<Application>) -> Router {
    Router::new()
        .route(
            "/docs/llms.txt",
            get(|| async { crate::protocol::resources::DOCS.llms_txt() }),
        )
        .route(
            "/docs/{id}",
            get(|Path(id): Path<String>| async move {
                crate::protocol::resources::DOCS
                    .doc(&id)
                    .map(|d| {
                        (
                            [(
                                axum::http::header::CONTENT_TYPE,
                                "text/markdown; charset=utf-8",
                            )],
                            d.body,
                        )
                            .into_response()
                    })
                    .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
            }),
        )
        .route("/computers", get(collection).post(create))
        .route("/computers/{id}", get(computer))
        .route("/computers/{id}/operations/{operation_id}", get(operation))
        .route("/computers/{id}/start", post(start))
        .route("/computers/{id}/stop", post(stop))
        .with_state(app)
}
