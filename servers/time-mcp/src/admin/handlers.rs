use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use veoveo_mcp_contract::GatewayInternalIdentity;

use crate::{
    contract::{
        ActivateReleaseRequest, AdminPage, AuthorityRelease, CalendarVersionPath,
        ClockQualityPolicy, CreateAcquisitionRequest, CreateCalendarRequest, CreateSourceRequest,
        MissionEpoch, MissionEpochId, ReplaceClockQualityPolicyRequest, ReplaceSourceRequest,
        TimeAcquisition, TimeAcquisitionId, TimeSource, TimeSourceId, UpsertMissionEpochRequest,
    },
    mcp::SERVER_DOCS,
    state::TimeApplication,
    uris,
};

use super::error::ApiError;

type ApiResult<T> = Result<Json<T>, ApiError>;

pub(super) async fn list_sources(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
) -> ApiResult<AdminPage<TimeSource>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(page(state.catalog.list_sources(&scope).await?)))
}

pub(super) async fn get_source(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(source_id): Path<TimeSourceId>,
) -> ApiResult<TimeSource> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .source(&scope, &source_id)
            .await?
            .ok_or_else(|| ApiError::not_found("unknown time source"))?,
    ))
}

pub(super) async fn create_source(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<CreateSourceRequest>,
) -> ApiResult<TimeSource> {
    if request.idempotency_key.trim().is_empty() || request.source.record_version != 0 {
        return Err(ApiError::bad_request(
            "source creation requires an idempotency key and record_version 0",
        ));
    }
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state.catalog.create_source(&scope, request.source).await?,
    ))
}

pub(super) async fn replace_source(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(source_id): Path<TimeSourceId>,
    Json(request): Json<ReplaceSourceRequest>,
) -> ApiResult<TimeSource> {
    if source_id != request.source.source_id {
        return Err(ApiError::bad_request("path and source identities differ"));
    }
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .replace_source(&scope, request.source, request.expected_record_version)
            .await?,
    ))
}

pub(super) async fn list_acquisitions(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
) -> ApiResult<AdminPage<TimeAcquisition>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(page(state.catalog.list_acquisitions(&scope).await?)))
}

pub(super) async fn get_acquisition(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<TimeAcquisitionId>,
) -> ApiResult<TimeAcquisition> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .acquisition(&scope, &id)
            .await?
            .ok_or_else(|| ApiError::not_found("unknown time acquisition"))?,
    ))
}

pub(super) async fn create_acquisition(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<CreateAcquisitionRequest>,
) -> ApiResult<TimeAcquisition> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let source = state
        .catalog
        .source(&scope, &request.source_id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown time source"))?;
    Ok(Json(
        state
            .acquisitions
            .start(
                scope,
                source,
                request.expected_source_digest_sha256,
                request.idempotency_key,
            )
            .await?,
    ))
}

pub(super) async fn cancel_acquisition(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<TimeAcquisitionId>,
) -> ApiResult<TimeAcquisition> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(state.acquisitions.cancel(&scope, &id).await?))
}

pub(super) async fn list_releases(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
) -> ApiResult<AdminPage<AuthorityRelease>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(page(state.catalog.list_releases(&scope).await?)))
}

pub(super) async fn get_release(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<crate::contract::AuthorityReleaseId>,
) -> ApiResult<AuthorityRelease> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(state.catalog.release(&scope, &id).await?.ok_or_else(
        || ApiError::not_found("unknown authority release"),
    )?))
}

pub(super) async fn activate_release(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<crate::contract::AuthorityReleaseId>,
    Json(request): Json<ActivateReleaseRequest>,
) -> ApiResult<AuthorityRelease> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let _guard = state.activation.lock().await;
    let candidate = state
        .catalog
        .release(&scope, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown authority release"))?;
    state
        .authorities
        .preflight_activation(&state.catalog, &scope, &candidate)
        .await?;
    let release = state
        .catalog
        .activate_release(
            &scope,
            &id,
            request.expected_release_record_version,
            request.expected_active_pointer_version,
        )
        .await?;
    state.authorities.reload(&state.catalog, &scope).await?;
    state
        .subscriptions
        .notify_resource_updated(uris::AUTHORITIES_CURRENT_URI)
        .await;
    Ok(Json(release))
}

pub(super) async fn list_active_authorities(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
) -> ApiResult<AdminPage<AuthorityRelease>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(page(state.catalog.active_releases(&scope).await?)))
}

pub(super) async fn list_calendars(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
) -> ApiResult<AdminPage<crate::contract::OperationalCalendar>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(page(state.catalog.list_calendars(&scope).await?)))
}

pub(super) async fn get_calendar(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(path): Path<CalendarVersionPath>,
) -> ApiResult<crate::contract::OperationalCalendar> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .calendar(&scope, &path.calendar_id, path.version)
            .await?
            .ok_or_else(|| ApiError::not_found("unknown calendar version"))?,
    ))
}

pub(super) async fn create_calendar(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<CreateCalendarRequest>,
) -> ApiResult<crate::contract::OperationalCalendar> {
    if request.idempotency_key.trim().is_empty() {
        return Err(ApiError::bad_request(
            "calendar idempotency key must not be empty",
        ));
    }
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let calendar = state
        .catalog
        .create_calendar(&scope, request.calendar)
        .await?;
    state
        .subscriptions
        .notify_resource_updated(uris::CALENDARS_URI)
        .await;
    Ok(Json(calendar))
}

pub(super) async fn list_epochs(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
) -> ApiResult<AdminPage<MissionEpoch>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(page(state.catalog.list_epochs(&scope).await?)))
}

pub(super) async fn get_epoch(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<MissionEpochId>,
) -> ApiResult<MissionEpoch> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(state.catalog.epoch(&scope, &id).await?.ok_or_else(
        || ApiError::not_found("unknown mission epoch"),
    )?))
}

pub(super) async fn create_epoch(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<UpsertMissionEpochRequest>,
) -> ApiResult<MissionEpoch> {
    if request.idempotency_key.trim().is_empty() {
        return Err(ApiError::bad_request(
            "epoch idempotency key must not be empty",
        ));
    }
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let epoch = state.catalog.create_epoch(&scope, request.epoch).await?;
    state.authorities.reload(&state.catalog, &scope).await?;
    state
        .subscriptions
        .notify_resource_updated(uris::EPOCHS_URI)
        .await;
    Ok(Json(epoch))
}

pub(super) async fn get_clock_policy(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
) -> ApiResult<(ClockQualityPolicy, u64)> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(state.catalog.clock_policy(&scope).await?.ok_or_else(
        || ApiError::not_found("clock policy is not configured"),
    )?))
}

pub(super) async fn replace_clock_policy(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<ReplaceClockQualityPolicyRequest>,
) -> ApiResult<(ClockQualityPolicy, u64)> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let result = state
        .catalog
        .replace_clock_policy(&scope, request.policy, request.expected_record_version)
        .await?;
    state
        .subscriptions
        .notify_resource_updated(uris::CLOCK_QUALITY_URI)
        .await;
    Ok(Json(result))
}

/// `GET {mount}/admin/docs/llms.txt` (contract C20).
pub(super) async fn docs_index() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        SERVER_DOCS.llms_txt(),
    )
        .into_response()
}

/// `GET {mount}/admin/docs/{doc_id}` (contract C20).
pub(super) async fn doc_body(Path(doc_id): Path<String>) -> Response {
    match SERVER_DOCS.doc(&doc_id) {
        Some(doc) => (
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            doc.body,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "unknown Time document").into_response(),
    }
}

fn page<T>(items: Vec<T>) -> AdminPage<T> {
    AdminPage {
        items,
        next_cursor: None,
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn respond(uri: &str) -> (axum::http::StatusCode, String, String) {
        let response = super::super::docs_router()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .map(|value| value.to_str().unwrap().to_owned())
            .unwrap_or_default();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            content_type,
            String::from_utf8(bytes.to_vec()).unwrap(),
        )
    }

    #[tokio::test]
    async fn llms_txt_indexes_the_embedded_documents() {
        let (status, content_type, body) = respond("/docs/llms.txt").await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(content_type, "text/plain; charset=utf-8");
        assert!(body.starts_with("# time\n"));
        assert!(body.contains("(agents)"));
        assert!(body.contains("(design)"));
    }

    #[tokio::test]
    async fn document_bodies_serve_the_embedded_markdown() {
        let (status, content_type, body) = respond("/docs/agents").await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(content_type, "text/markdown; charset=utf-8");
        assert!(body.contains("## Contract Compliance"));

        let (status, _, _) = respond("/docs/unknown").await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }
}
