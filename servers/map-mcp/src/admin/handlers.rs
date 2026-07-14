use std::sync::Arc;

use anyhow::Context;
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use veoveo_mcp_contract::GatewayInternalIdentity;

use crate::{
    contract::{
        AcquisitionId, AcquisitionJob, AcquisitionListQuery, ActiveReleaseListQuery,
        ActiveReleasePointer, AdminPage, CreateAcquisitionRequest, CreateMobilityProfileRequest,
        CreateSourceRequest, DatasetRelease, DatasetReleaseId, DatasetReleaseState, MapSourceId,
        MobilityProfile, MobilityProfileListQuery, MobilityProfilePath, RegisteredSource,
        ReleaseListQuery, ReleaseMutationRequest, ReleaseMutationResponse, ReplaceSourceRequest,
        SourceListQuery, SourceMutationRequest,
    },
    server::auth::ForwardedBearer,
    state::MapApplication,
};

use super::error::ApiError;

type ApiResult<T> = Result<Json<T>, ApiError>;

pub(super) async fn list_sources(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Query(query): Query<SourceListQuery>,
) -> ApiResult<AdminPage<RegisteredSource>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let items = state
        .catalog
        .list_sources(&scope)
        .await?
        .into_iter()
        .filter(|source| query.enabled.is_none_or(|value| source.enabled == value))
        .filter(|source| {
            query
                .adapter_kind
                .is_none_or(|value| source.adapter_kind == value)
        })
        .collect();
    Ok(Json(admin_page(
        items,
        query.cursor.as_deref(),
        query.limit,
    )?))
}

pub(super) async fn get_source(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(source_id): Path<String>,
) -> ApiResult<RegisteredSource> {
    let source_id = MapSourceId::parse(source_id).map_err(ApiError::bad_request)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .source(&scope, &source_id)
            .await?
            .context("unknown map source")?,
    ))
}

pub(super) async fn create_source(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<CreateSourceRequest>,
) -> ApiResult<RegisteredSource> {
    validate_idempotency_key(&request.idempotency_key)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    if let Some(existing) = state
        .catalog
        .source(&scope, &request.source.source_id)
        .await?
    {
        if existing == request.source {
            return Ok(Json(existing));
        }
        return Err(ApiError::conflict(
            "source id conflicts with an existing registered source",
        ));
    }
    Ok(Json(
        state.catalog.create_source(&scope, request.source).await?,
    ))
}

pub(super) async fn replace_source(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(source_id): Path<String>,
    Json(request): Json<ReplaceSourceRequest>,
) -> ApiResult<RegisteredSource> {
    let source_id = MapSourceId::parse(source_id).map_err(ApiError::bad_request)?;
    if request.source.source_id != source_id {
        return Err(ApiError::bad_request(
            "path source id does not match the replacement body",
        ));
    }
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .replace_source(&scope, request.source, request.expected_record_version)
            .await?,
    ))
}

pub(super) async fn disable_source(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(source_id): Path<String>,
    Json(request): Json<SourceMutationRequest>,
) -> ApiResult<RegisteredSource> {
    let source_id = MapSourceId::parse(source_id).map_err(ApiError::bad_request)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let mut source = state
        .catalog
        .source(&scope, &source_id)
        .await?
        .context("unknown map source")?;
    if source.record_version != request.expected_record_version {
        return Err(ApiError::conflict("source record version changed"));
    }
    source.enabled = false;
    source.record_version += 1;
    source.updated_at = chrono::Utc::now();
    Ok(Json(
        state
            .catalog
            .replace_source(&scope, source, request.expected_record_version)
            .await?,
    ))
}

pub(super) async fn list_acquisitions(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Query(query): Query<AcquisitionListQuery>,
) -> ApiResult<AdminPage<AcquisitionJob>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    state.acquisitions.reconcile_interrupted(&scope).await?;
    let items = state
        .catalog
        .list_acquisitions(&scope)
        .await?
        .into_iter()
        .filter(|job| {
            query
                .source_id
                .as_ref()
                .is_none_or(|value| &job.source_id == value)
        })
        .filter(|job| query.status.is_none_or(|value| job.status == value))
        .collect();
    Ok(Json(admin_page(
        items,
        query.cursor.as_deref(),
        query.limit,
    )?))
}

pub(super) async fn get_acquisition(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(acquisition_id): Path<String>,
) -> ApiResult<AcquisitionJob> {
    let acquisition_id = AcquisitionId::parse(acquisition_id).map_err(ApiError::bad_request)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .acquisition(&scope, &acquisition_id)
            .await?
            .context("unknown acquisition job")?,
    ))
}

pub(super) async fn create_acquisition(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Extension(bearer): Extension<ForwardedBearer>,
    Json(request): Json<CreateAcquisitionRequest>,
) -> Result<(StatusCode, Json<AcquisitionJob>), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let caller = state.caller(identity, bearer.0);
    Ok((
        StatusCode::ACCEPTED,
        Json(state.acquisitions.start(scope, caller, request).await?),
    ))
}

pub(super) async fn cancel_acquisition(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(acquisition_id): Path<String>,
) -> ApiResult<AcquisitionJob> {
    let acquisition_id = AcquisitionId::parse(acquisition_id).map_err(ApiError::bad_request)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state.acquisitions.cancel(&scope, &acquisition_id).await?,
    ))
}

pub(super) async fn list_releases(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Query(query): Query<ReleaseListQuery>,
) -> ApiResult<AdminPage<DatasetRelease>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let items = state
        .catalog
        .list_releases(&scope)
        .await?
        .into_iter()
        .filter(|release| {
            query
                .dataset_id
                .as_ref()
                .is_none_or(|value| &release.dataset_id == value)
        })
        .filter(|release| {
            query
                .source_id
                .as_ref()
                .is_none_or(|value| &release.source_id == value)
        })
        .filter(|release| query.state.is_none_or(|value| release.state == value))
        .collect();
    Ok(Json(admin_page(
        items,
        query.cursor.as_deref(),
        query.limit,
    )?))
}

pub(super) async fn get_release(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(release_id): Path<String>,
) -> ApiResult<DatasetRelease> {
    let release_id = DatasetReleaseId::parse(release_id).map_err(ApiError::bad_request)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .release(&scope, &release_id)
            .await?
            .context("unknown dataset release")?,
    ))
}

pub(super) async fn list_active_releases(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Query(query): Query<ActiveReleaseListQuery>,
) -> ApiResult<AdminPage<ActiveReleasePointer>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(admin_page(
        state.catalog.list_active_releases(&scope).await?,
        query.cursor.as_deref(),
        query.limit,
    )?))
}

pub(super) async fn list_mobility_profiles(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Query(query): Query<MobilityProfileListQuery>,
) -> ApiResult<AdminPage<MobilityProfile>> {
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let items = state
        .catalog
        .list_mobility_profiles(&scope)
        .await?
        .into_iter()
        .filter(|profile| query.family.is_none_or(|family| profile.family() == family))
        .collect();
    Ok(Json(admin_page(
        items,
        query.cursor.as_deref(),
        query.limit,
    )?))
}

pub(super) async fn get_mobility_profile(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(path): Path<MobilityProfilePath>,
) -> ApiResult<MobilityProfile> {
    if path.version == 0 {
        return Err(ApiError::bad_request("profile version must be positive"));
    }
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    Ok(Json(
        state
            .catalog
            .mobility_profile(&scope, &path.profile_id, path.version)
            .await?
            .context("unknown mobility profile version")?,
    ))
}

pub(super) async fn create_mobility_profile(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Json(request): Json<CreateMobilityProfileRequest>,
) -> ApiResult<MobilityProfile> {
    validate_idempotency_key(&request.idempotency_key)?;
    request.profile.validate().map_err(ApiError::bad_request)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let metadata = request.profile.metadata();
    if let Some(existing) = state
        .catalog
        .mobility_profile(&scope, &metadata.profile_id, metadata.version)
        .await?
    {
        if existing == request.profile {
            return Ok(Json(existing));
        }
        return Err(ApiError::conflict(
            "mobility profile id and version conflict with an existing profile",
        ));
    }
    Ok(Json(
        state
            .catalog
            .create_mobility_profile(&scope, request.profile)
            .await?,
    ))
}

pub(super) async fn activate_release(
    state: State<Arc<MapApplication>>,
    identity: Extension<GatewayInternalIdentity>,
    path: Path<String>,
    request: Json<ReleaseMutationRequest>,
) -> ApiResult<ReleaseMutationResponse> {
    activate_target(state, identity, path, request, false).await
}

pub(super) async fn rollback_release(
    state: State<Arc<MapApplication>>,
    identity: Extension<GatewayInternalIdentity>,
    path: Path<String>,
    request: Json<ReleaseMutationRequest>,
) -> ApiResult<ReleaseMutationResponse> {
    activate_target(state, identity, path, request, true).await
}

async fn activate_target(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(release_id): Path<String>,
    Json(request): Json<ReleaseMutationRequest>,
    rollback: bool,
) -> ApiResult<ReleaseMutationResponse> {
    let _activation = state.activation.lock().await;
    let release_id = DatasetReleaseId::parse(release_id).map_err(ApiError::bad_request)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let release = state
        .catalog
        .release(&scope, &release_id)
        .await?
        .context("unknown dataset release")?;
    if release.record_version != request.expected_record_version {
        return Err(ApiError::conflict("release record version changed"));
    }
    if release.state == DatasetReleaseState::Quarantined {
        return Err(ApiError::bad_request(
            "quarantined release cannot be activated",
        ));
    }
    let active_pointer = state
        .catalog
        .list_active_releases(&scope)
        .await?
        .into_iter()
        .find(|pointer| pointer.dataset_id == release.dataset_id);
    let already_active = release.state == DatasetReleaseState::Active
        && active_pointer
            .as_ref()
            .is_some_and(|pointer| pointer.release_id == release.release_id);
    if !rollback && release.state != DatasetReleaseState::Staged && !already_active {
        return Err(ApiError::bad_request(
            "activation requires a staged release or the current active release",
        ));
    }
    if already_active
        && active_pointer.as_ref().is_some_and(|pointer| {
            pointer.record_version != request.expected_active_pointer_version
        })
    {
        return Err(ApiError::conflict("active release pointer version changed"));
    }
    let previous_release_id = active_pointer
        .as_ref()
        .map(|pointer| pointer.release_id.clone());
    let source = state
        .catalog
        .source(&scope, &release.source_id)
        .await?
        .context("unknown map source for release")?;
    let tenant_key = scope.tenant_key();
    state
        .products
        .prepare(&tenant_key, &release, &source)
        .await?;
    let release = if already_active {
        release
    } else {
        state
            .catalog
            .activate_release(
                &scope,
                release,
                Some(request.expected_active_pointer_version),
            )
            .await?
    };
    state.products.activate(&tenant_key, &release).await?;
    if release.routing_build_version.is_some() {
        state.valhalla_process.restart().await?;
    }
    let invalidated_route_count = if let Some(previous_release_id) = previous_release_id
        && previous_release_id != release.release_id
    {
        let invalidated = state
            .catalog
            .invalidate_routes_for_release(&scope, &previous_release_id)
            .await?;
        if let Some(previous) = state.catalog.release(&scope, &previous_release_id).await?
            && previous.state == DatasetReleaseState::Active
        {
            let expected = previous.record_version;
            state
                .catalog
                .transition_release(&scope, previous, DatasetReleaseState::Retired, expected)
                .await?;
        }
        invalidated
    } else {
        0
    };
    state
        .subscriptions
        .notify_resource_updated(crate::uris::DATASETS_URI)
        .await;
    state
        .subscriptions
        .notify_resource_updated(crate::uris::ROUTES_URI)
        .await;
    Ok(Json(ReleaseMutationResponse {
        release,
        invalidated_route_count,
    }))
}

pub(super) async fn quarantine_release(
    State(state): State<Arc<MapApplication>>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(release_id): Path<String>,
    Json(request): Json<ReleaseMutationRequest>,
) -> ApiResult<ReleaseMutationResponse> {
    let _activation = state.activation.lock().await;
    let release_id = DatasetReleaseId::parse(release_id).map_err(ApiError::bad_request)?;
    let scope = state.scope(&identity).await.map_err(ApiError::internal)?;
    let release = state
        .catalog
        .release(&scope, &release_id)
        .await?
        .context("unknown dataset release")?;
    if state
        .catalog
        .active_release_id(&scope, &release.dataset_id)
        .await?
        .as_ref()
        == Some(&release.release_id)
    {
        return Err(ApiError::conflict(
            "an active release must be replaced before it can be quarantined",
        ));
    }
    let release = state
        .catalog
        .transition_release(
            &scope,
            release,
            DatasetReleaseState::Quarantined,
            request.expected_record_version,
        )
        .await?;
    let invalidated_route_count = state
        .catalog
        .invalidate_routes_for_release(&scope, &release.release_id)
        .await?;
    state
        .subscriptions
        .notify_resource_updated(crate::uris::DATASETS_URI)
        .await;
    state
        .subscriptions
        .notify_resource_updated(crate::uris::ROUTES_URI)
        .await;
    Ok(Json(ReleaseMutationResponse {
        release,
        invalidated_route_count,
    }))
}

fn validate_idempotency_key(value: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(ApiError::bad_request("invalid idempotency key"));
    }
    Ok(())
}

fn admin_page<T>(
    items: Vec<T>,
    cursor: Option<&str>,
    limit: Option<usize>,
) -> Result<AdminPage<T>, ApiError> {
    const DEFAULT_LIMIT: usize = 50;
    const MAX_LIMIT: usize = 200;
    const CURSOR_PREFIX: &str = "map-admin-v1:";
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::bad_request("list limit must be within 1..=200"));
    }
    let offset = cursor
        .map(|cursor| {
            cursor
                .strip_prefix(CURSOR_PREFIX)
                .ok_or_else(|| ApiError::bad_request("invalid list cursor"))?
                .parse::<usize>()
                .map_err(|_| ApiError::bad_request("invalid list cursor"))
        })
        .transpose()?
        .unwrap_or_default();
    let total = items.len();
    let next_offset = offset.saturating_add(limit);
    Ok(AdminPage {
        items: items.into_iter().skip(offset).take(limit).collect(),
        next_cursor: (next_offset < total).then(|| format!("{CURSOR_PREFIX}{next_offset}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_pages_use_bounded_opaque_cursors() {
        let first = admin_page((0_u16..125).collect(), None, Some(50)).unwrap();
        assert_eq!(first.items.len(), 50);
        assert_eq!(first.next_cursor.as_deref(), Some("map-admin-v1:50"));

        let last = admin_page(
            (0_u16..125).collect(),
            first.next_cursor.as_deref(),
            Some(100),
        )
        .unwrap();
        assert_eq!(last.items, (50_u16..125).collect::<Vec<_>>());
        assert!(last.next_cursor.is_none());
    }

    #[test]
    fn admin_pages_reject_forged_and_unbounded_cursors() {
        assert!(admin_page(vec![1], Some("50"), Some(1)).is_err());
        assert!(admin_page(vec![1], None, Some(201)).is_err());
    }
}
