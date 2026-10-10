use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use veoveo_mcp_contract::GatewayInternalIdentity;

use crate::{
    contract::{
        ActivateReleaseRequest, AdminPage, AuthorityRelease, CalendarId, ClockQualityPolicy,
        CreateAcquisitionRequest, CreateCalendarRequest, CreateSourceRequest, MissionEpoch,
        MissionEpochId, ReplaceClockQualityPolicyRequest, ReplaceSourceRequest, TimeAcquisition,
        TimeAcquisitionId, TimeSource, TimeSourceId, UpsertMissionEpochRequest,
    },
    state::TimeApplication,
    uris,
};

use super::error::ApiError;

type ApiResult<T> = Result<Json<T>, ApiError>;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(super) struct CursorQuery<C> {
    cursor: Option<C>,
}

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
    let request = crate::CreateSourceRequestValue::from(request);
    if request.idempotency_key.trim().is_empty() {
        return Err(ApiError::bad_request(
            "source creation requires an idempotency key",
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
    let request = crate::CreateAcquisitionRequestValue::from(request);
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
    let candidate = state
        .catalog
        .release(&scope, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown authority release"))?;
    let release = state
        .authorities
        .activate_release(
            &state.catalog,
            &scope,
            &candidate.release_id,
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
) -> ApiResult<AdminPage<crate::ActiveAuthoritySelection>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(page(state.catalog.active_authorities(&scope).await?)))
}

pub(super) async fn list_calendars(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Query(query): Query<CursorQuery<crate::contract::CalendarCursor>>,
) -> ApiResult<
    crate::contract::CollectionPage<
        crate::contract::OperationalCalendar,
        crate::contract::CalendarCursor,
    >,
> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let after = query.cursor;
    Ok(Json(
        state.catalog.calendars_page(&scope, after.as_ref()).await?,
    ))
}

pub(super) async fn get_calendar(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path((calendar_id, version)): Path<(CalendarId, crate::contract::TimeVersion)>,
) -> ApiResult<crate::contract::OperationalCalendar> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .calendar(&scope, &calendar_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("unknown calendar version"))?,
    ))
}

pub(super) async fn create_calendar(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<CreateCalendarRequest>,
) -> ApiResult<crate::contract::OperationalCalendar> {
    let request = crate::CreateCalendarRequestValue::from(request);
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
    Query(query): Query<CursorQuery<crate::contract::EpochCursor>>,
) -> ApiResult<crate::contract::CollectionPage<MissionEpoch, crate::contract::EpochCursor>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let after = query.cursor;
    Ok(Json(
        state.catalog.epochs_page(&scope, after.as_ref()).await?,
    ))
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
    let request = crate::UpsertMissionEpochRequestValue::from(request);
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
) -> ApiResult<(ClockQualityPolicy, crate::TimeVersion)> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(state.catalog.clock_policy(&scope).await?.ok_or_else(
        || ApiError::not_found("clock policy is not configured"),
    )?))
}

pub(super) async fn replace_clock_policy(
    State(state): State<Arc<TimeApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<ReplaceClockQualityPolicyRequest>,
) -> ApiResult<(ClockQualityPolicy, crate::TimeVersion)> {
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

fn page<T>(items: Vec<T>) -> AdminPage<T> {
    AdminPage {
        items,
        next_cursor: None,
    }
}
