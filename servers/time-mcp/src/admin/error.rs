use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::contract::AdminError;

#[derive(Debug)]
pub(super) struct ApiError {
    status: StatusCode,
    body: AdminError,
}

impl ApiError {
    pub fn bad_request(message: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            message.to_string(),
            false,
        )
    }
    pub fn not_found(message: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            message.to_string(),
            false,
        )
    }
    pub fn conflict(message: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "version_conflict",
            message.to_string(),
            false,
        )
    }
    pub fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!("Time administrative operation failed: {error}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Time administrative operation failed",
            true,
        )
    }
    fn new(status: StatusCode, code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            status,
            body: AdminError {
                code: code.to_owned(),
                message: message.into(),
                retryable,
                trace_id: uuid::Uuid::now_v7().to_string(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        let message = error.to_string();
        if message.contains("unknown") || message.contains("disappeared") {
            Self::not_found(message)
        } else if message.contains("conflict") || message.contains("record_version") {
            Self::conflict(message)
        } else if message.contains("invalid")
            || message.contains("must")
            || message.contains("disabled")
            || message.contains("digest")
        {
            Self::bad_request(message)
        } else {
            Self::internal(error)
        }
    }
}
