use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use veoveo_computers::{ComputerActor, ComputerError, api::*};
use veoveo_mcp_contract::GatewayInternalIdentity;
pub(super) fn actor(identity: &GatewayInternalIdentity) -> Result<ComputerActor, HttpError> {
    ComputerActor::from_verified(identity)
        .map_err(|e| HttpError(crate::ApplicationError::Domain(e)))
}
pub(super) struct HttpError(pub(super) crate::ApplicationError);
impl From<crate::ApplicationError> for HttpError {
    fn from(e: crate::ApplicationError) -> Self {
        Self(e)
    }
}
impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            crate::ApplicationError::Domain(ComputerError::NotFound) => {
                (StatusCode::NOT_FOUND, ErrorCode::NotFound)
            }
            crate::ApplicationError::Domain(ComputerError::Forbidden) => {
                (StatusCode::FORBIDDEN, ErrorCode::Forbidden)
            }
            crate::ApplicationError::Domain(
                ComputerError::InvalidInput | ComputerError::RequestConflict,
            ) => (StatusCode::BAD_REQUEST, ErrorCode::InvalidInput),
            crate::ApplicationError::Domain(ComputerError::CapacityFull) => {
                (StatusCode::CONFLICT, ErrorCode::CapacityFull)
            }
            crate::ApplicationError::Domain(ComputerError::AccessLimit) => {
                (StatusCode::CONFLICT, ErrorCode::AccessLimit)
            }
            crate::ApplicationError::Domain(ComputerError::OperationBusy) => {
                (StatusCode::CONFLICT, ErrorCode::Busy)
            }
            crate::ApplicationError::Domain(
                ComputerError::InvalidState | ComputerError::StateConflict,
            ) => (StatusCode::CONFLICT, ErrorCode::InvalidState),
            _ => (StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable),
        };
        (
            status,
            Json(ApiError {
                code,
                message: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
