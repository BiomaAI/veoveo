//! Upload-only HTTP transport. Authentication precedes bounded body consumption.

use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use futures::StreamExt;
use serde::{Deserialize, de::DeserializeOwned};
use std::num::NonZeroU32;
use veoveo_mcp_contract::{self as contract, UploadErrorCode as Code};

use crate::{
    store::BlobStoreError,
    uploads::{UploadFault, UploadService},
};

#[derive(Clone)]
struct UploadState {
    service: UploadService,
    verifier: contract::GatewayInternalTokenVerifier,
}

pub fn router(service: UploadService, verifier: contract::GatewayInternalTokenVerifier) -> Router {
    Router::new()
        .route("/artifact-uploads/policy", get(policy))
        .route("/artifact-uploads", post(create))
        .route("/artifact-uploads/{id}", get(status).delete(cancel))
        .route("/artifact-uploads/{id}/parts/{number}", put(part))
        .route("/artifact-uploads/{id}/complete", post(complete))
        .with_state(UploadState { service, verifier })
        .layer(middleware::map_response(|mut response: Response| async {
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }))
}

impl UploadState {
    fn caller(
        &self,
        headers: &HeaderMap,
    ) -> Result<contract::VerifiedArtifactUploadIdentity, UploadFault> {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split_once(' '))
            .filter(|(scheme, token)| scheme.eq_ignore_ascii_case("Bearer") && !token.is_empty())
            .map(|(_, token)| token)
            .ok_or(Code::Unauthenticated)?;
        self.verifier
            .verify_artifact_upload(bearer)
            .map_err(|_| Code::Unauthenticated.into())
    }
}

async fn json<T: DeserializeOwned>(headers: &HeaderMap, body: Body) -> Result<T, UploadFault> {
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_none_or(|v| v.trim() != "application/json")
    {
        return Err(Code::Malformed.into());
    }
    let bytes = to_bytes(body, 16 * 1024)
        .await
        .map_err(|_| Code::TooLarge)?;
    serde_json::from_slice(&bytes).map_err(|_| Code::Malformed.into())
}

fn id(value: String) -> Result<contract::ArtifactUploadId, UploadFault> {
    contract::ArtifactUploadId::parse(value).map_err(|_| Code::NotFound.into())
}

fn required<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, UploadFault> {
    if headers.get_all(name).iter().count() != 1 {
        return Err(Code::Malformed.into());
    }
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or(Code::Malformed.into())
}

async fn policy(
    State(state): State<UploadState>,
    headers: HeaderMap,
) -> Result<Response, UploadFault> {
    let caller = state.caller(&headers)?;
    Ok(Json(state.service.policy(&caller).await?).into_response())
}

async fn create(
    State(state): State<UploadState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, UploadFault> {
    let caller = state.caller(&headers)?;
    let request_id =
        contract::ArtifactUploadRequestId::parse(required(&headers, "idempotency-key")?)
            .map_err(|_| Code::Malformed)?;
    let descriptor = json(&headers, body).await?;
    let (session, created) = state
        .service
        .create(&caller, request_id, descriptor)
        .await?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(session),
    )
        .into_response())
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    #[serde(default)]
    after: u32,
}

async fn status(
    State(state): State<UploadState>,
    Path(upload): Path<String>,
    headers: HeaderMap,
    query: Result<Query<Cursor>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, UploadFault> {
    let caller = state.caller(&headers)?;
    let Query(cursor) = query.map_err(|_| Code::Malformed)?;
    if cursor.after > 10000 {
        return Err(Code::Malformed.into());
    }
    Ok(Json(
        state
            .service
            .status(&caller, id(upload)?, cursor.after)
            .await?,
    )
    .into_response())
}

async fn part(
    State(state): State<UploadState>,
    Path((upload, number)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, UploadFault> {
    let caller = state.caller(&headers)?;
    let number = number.parse::<NonZeroU32>().map_err(|_| Code::Malformed)?;
    let len = required(&headers, contract::UPLOAD_PART_BYTE_LEN_HEADER)?
        .parse::<u64>()
        .map_err(|_| Code::Malformed)?;
    let sha =
        contract::UploadSha256::parse(required(&headers, contract::UPLOAD_PART_SHA256_HEADER)?)
            .map_err(|_| Code::Malformed)?;
    if headers.contains_key(header::CONTENT_LENGTH) {
        let content_len = required(&headers, "content-length")?
            .parse::<u64>()
            .map_err(|_| Code::Malformed)?;
        if content_len != len {
            return Err(Code::Malformed.into());
        }
    }
    let stream = Box::pin(body.into_data_stream().map(|frame| {
        frame.map_err(|_| BlobStoreError::Backend("upload request interrupted".into()))
    }));
    Ok(Json(
        state
            .service
            .put_part(&caller, id(upload)?, number, len, sha, stream)
            .await?,
    )
    .into_response())
}

async fn complete(
    State(state): State<UploadState>,
    Path(upload): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, UploadFault> {
    let caller = state.caller(&headers)?;
    let manifest = json(&headers, body).await?;
    let session = state
        .service
        .complete(&caller, id(upload)?, manifest)
        .await?;
    let status = if session.receipt.is_some() {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((status, Json(session)).into_response())
}

async fn cancel(
    State(state): State<UploadState>,
    Path(upload): Path<String>,
    headers: HeaderMap,
) -> Result<Response, UploadFault> {
    let caller = state.caller(&headers)?;
    state.service.cancel(&caller, id(upload)?).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
