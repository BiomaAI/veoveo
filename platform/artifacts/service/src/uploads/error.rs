use super::*;
use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub struct UploadFault(pub contract::UploadErrorCode);

impl UploadFault {
    pub(super) fn unavailable() -> Self {
        Self(contract::UploadErrorCode::Unavailable)
    }
}

impl From<contract::UploadErrorCode> for UploadFault {
    fn from(code: contract::UploadErrorCode) -> Self {
        Self(code)
    }
}

impl From<platform::StoreError> for UploadFault {
    fn from(error: platform::StoreError) -> Self {
        use contract::UploadErrorCode as Code;
        use platform::ArtifactUploadRejection as Rejection;
        let code = match &error {
            platform::StoreError::ArtifactUpload(reason) => match reason {
                Rejection::Conflict => Code::Conflict,
                Rejection::Denied => Code::Denied,
                Rejection::Quota => Code::QuotaExceeded,
                Rejection::Busy => Code::Busy,
                Rejection::Expired => Code::Expired,
                Rejection::Integrity => Code::Integrity,
            },
            platform::StoreError::ArtifactBlobIntegrityConflict => Code::Integrity,
            _ => {
                tracing::error!(error = %error, "upload ledger operation failed");
                Code::Unavailable
            }
        };
        Self(code)
    }
}

impl From<crate::store::BlobStoreError> for UploadFault {
    fn from(error: crate::store::BlobStoreError) -> Self {
        use crate::store::BlobStoreError;
        let code = match &error {
            BlobStoreError::TooLarge { .. } | BlobStoreError::VerificationFailed { .. } => {
                contract::UploadErrorCode::Integrity
            }
            _ => {
                tracing::error!(error = %error, "upload storage operation failed");
                contract::UploadErrorCode::Unavailable
            }
        };
        Self(code)
    }
}

impl IntoResponse for UploadFault {
    fn into_response(self) -> Response {
        use contract::UploadErrorCode::*;
        let message = match self.0 {
            Malformed => "The upload request is invalid.",
            Unauthenticated => "Sign in to continue this upload.",
            Denied => "Current access does not allow this upload.",
            NotFound => "This upload is unavailable.",
            Conflict => "The upload has changed. Refresh its status before continuing.",
            Expired => "This upload expired. Start a new upload.",
            TooLarge => "The file or part exceeds the upload limit.",
            UnsupportedType => "This file type is not allowed here.",
            Integrity => "The file bytes did not pass verification.",
            QuotaExceeded => "There is not enough available artifact storage.",
            Busy => "Upload capacity is busy. Try again shortly.",
            Unavailable => "The upload service is temporarily unavailable.",
        };
        let retry = matches!(self.0, Busy | Unavailable);
        let body = contract::ArtifactUploadError {
            code: self.0,
            message: message.into(),
            request_id: contract::ArtifactUploadRequestId::new(),
            required_bytes: None,
            available_bytes: None,
        };
        let mut response = (
            StatusCode::from_u16(self.0.http_status()).unwrap_or(StatusCode::SERVICE_UNAVAILABLE),
            [(header::CACHE_CONTROL, "no-store")],
            Json(body),
        )
            .into_response();
        if retry {
            response.headers_mut().insert(
                header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("2"),
            );
        }
        response
    }
}
