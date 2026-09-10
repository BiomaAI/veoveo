//! Authenticated browser confirmation. The domain owns one-use consumption.
use super::{
    BrowserOrigins,
    http_error::{HttpError, actor},
};
use crate::{Application, ApplicationError};
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use std::sync::Arc;
use uuid::Uuid;
use veoveo_computers::{ComputerError, api::*};
use veoveo_mcp_contract::GatewayInternalIdentity;

#[derive(Clone)]
struct PairingState {
    app: Arc<Application>,
    origins: BrowserOrigins,
}
pub(super) fn router(app: Arc<Application>, origins: BrowserOrigins) -> Router {
    Router::new()
        .route("/computers/{id}/cli-pairings", post(begin))
        .route(
            "/computers/{id}/cli-pairings/{pairing_id}/confirm",
            post(confirm),
        )
        .with_state(PairingState { app, origins })
}
fn admit(state: &PairingState, headers: &HeaderMap, query: Option<&str>) -> Result<(), HttpError> {
    if query.is_some() || !state.origins.permits(headers) {
        return Err(HttpError(ApplicationError::Domain(
            ComputerError::Forbidden,
        )));
    }
    state.app.runtime.current()?;
    Ok(())
}
async fn begin(
    State(state): State<PairingState>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    Json(input): Json<CliPairingInput>,
) -> Result<Response, HttpError> {
    admit(&state, &headers, query.as_deref())?;
    let challenge = state
        .app
        .store
        .begin_cli_pairing(&actor(&identity)?, id, &input)
        .await
        .map_err(ApplicationError::from)?;
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(CliPairingChallenge {
            computer_id: challenge.computer_id,
            pairing_id: challenge.pairing_id,
            expires_at: challenge.expires_at,
        }),
    )
        .into_response())
}
async fn confirm(
    State(state): State<PairingState>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path((id, pairing_id)): Path<(Uuid, Uuid)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    Json(_input): Json<CliPairingConfirmBody>,
) -> Result<Response, HttpError> {
    admit(&state, &headers, query.as_deref())?;
    let grant = state
        .app
        .store
        .confirm_cli_pairing(&actor(&identity)?, id, pairing_id)
        .await
        .map_err(ApplicationError::from)?;
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(CliPairingResult {
            computer_id: grant.computer_id,
            pairing_id,
            grant_id: grant.grant_id,
            token: CliPairingToken::new(grant.credential.expose_secret().to_owned()),
            callback_port: grant.callback_port,
            expires_at: grant.expires_at,
        }),
    )
        .into_response())
}
