//! Transport-neutral map administration operations.
//!
//! Every governed administrative mutation lives here; the MCP tool surface
//! in `mcp.rs` is its only transport. Reads are MCP resources served from
//! the catalog directly.

use anyhow::Context;
use veoveo_mcp_contract::PlaneCaller;

use crate::{
    catalog::MapScope,
    contract::{
        AcquisitionJob, CancelAcquisitionRequest, CreateAcquisitionRequest,
        CreateMobilityProfileRequest, CreateSourceRequest, DatasetReleaseState,
        DisableSourceRequest, MobilityProfile, RegisteredSource, ReleaseMutationRequest,
        ReleaseMutationResponse, ReplaceSourceRequest,
    },
    state::MapApplication,
    uris,
};

#[derive(Debug)]
pub enum AdminOpError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Internal(anyhow::Error),
}

impl AdminOpError {
    fn bad_request(message: impl std::fmt::Display) -> Self {
        Self::BadRequest(message.to_string())
    }

    fn conflict(message: impl std::fmt::Display) -> Self {
        Self::Conflict(message.to_string())
    }
}

/// Domain errors surface as anyhow messages; classify them the same way the
/// retired admin REST layer did so operators keep precise failure kinds.
impl From<anyhow::Error> for AdminOpError {
    fn from(error: anyhow::Error) -> Self {
        let message = error.to_string();
        if message.contains("unknown") || message.contains("disappeared") {
            Self::NotFound(message)
        } else if message.contains("conflict") || message.contains("record_version") {
            Self::Conflict(message)
        } else if message.contains("invalid")
            || message.contains("must")
            || message.contains("disabled")
            || message.contains("terminal")
        {
            Self::BadRequest(message)
        } else {
            Self::Internal(error)
        }
    }
}

type OpResult<T> = Result<T, AdminOpError>;

pub async fn register_source(
    state: &MapApplication,
    scope: &MapScope,
    request: CreateSourceRequest,
) -> OpResult<RegisteredSource> {
    validate_idempotency_key(&request.idempotency_key)?;
    if let Some(existing) = state
        .catalog
        .source(scope, &request.source.source_id)
        .await?
    {
        if existing == request.source {
            return Ok(existing);
        }
        return Err(AdminOpError::conflict(
            "source id conflicts with an existing registered source",
        ));
    }
    Ok(state.catalog.create_source(scope, request.source).await?)
}

pub async fn replace_source(
    state: &MapApplication,
    scope: &MapScope,
    request: ReplaceSourceRequest,
) -> OpResult<RegisteredSource> {
    Ok(state
        .catalog
        .replace_source(scope, request.source, request.expected_record_version)
        .await?)
}

pub async fn disable_source(
    state: &MapApplication,
    scope: &MapScope,
    request: DisableSourceRequest,
) -> OpResult<RegisteredSource> {
    let mut source = state
        .catalog
        .source(scope, &request.source_id)
        .await?
        .context("unknown map source")?;
    if source.record_version != request.expected_record_version {
        return Err(AdminOpError::conflict("source record version changed"));
    }
    source.enabled = false;
    source.record_version += 1;
    source.updated_at = chrono::Utc::now();
    Ok(state
        .catalog
        .replace_source(scope, source, request.expected_record_version)
        .await?)
}

pub async fn start_acquisition(
    state: &MapApplication,
    scope: MapScope,
    caller: PlaneCaller,
    request: CreateAcquisitionRequest,
) -> OpResult<AcquisitionJob> {
    validate_idempotency_key(&request.idempotency_key)?;
    Ok(state.acquisitions.start(scope, caller, request).await?)
}

pub async fn cancel_acquisition(
    state: &MapApplication,
    scope: &MapScope,
    request: CancelAcquisitionRequest,
) -> OpResult<AcquisitionJob> {
    Ok(state
        .acquisitions
        .cancel(scope, &request.acquisition_id)
        .await?)
}

pub async fn register_mobility_profile(
    state: &MapApplication,
    scope: &MapScope,
    request: CreateMobilityProfileRequest,
) -> OpResult<MobilityProfile> {
    validate_idempotency_key(&request.idempotency_key)?;
    request
        .profile
        .validate()
        .map_err(AdminOpError::bad_request)?;
    let metadata = request.profile.metadata();
    if let Some(existing) = state
        .catalog
        .mobility_profile(scope, &metadata.profile_id, metadata.version)
        .await?
    {
        if existing == request.profile {
            return Ok(existing);
        }
        return Err(AdminOpError::conflict(
            "mobility profile id and version conflict with an existing profile",
        ));
    }
    Ok(state
        .catalog
        .create_mobility_profile(scope, request.profile)
        .await?)
}

pub async fn activate_release(
    state: &MapApplication,
    scope: &MapScope,
    request: ReleaseMutationRequest,
    rollback: bool,
) -> OpResult<ReleaseMutationResponse> {
    let _activation = state.activation.lock().await;
    let release = state
        .catalog
        .release(scope, &request.release_id)
        .await?
        .context("unknown dataset release")?;
    if release.record_version != request.expected_record_version {
        return Err(AdminOpError::conflict("release record version changed"));
    }
    if release.state == DatasetReleaseState::Quarantined {
        return Err(AdminOpError::bad_request(
            "quarantined release cannot be activated",
        ));
    }
    let active_pointer = state
        .catalog
        .list_active_releases(scope)
        .await?
        .into_iter()
        .find(|pointer| pointer.dataset_id == release.dataset_id);
    let already_active = release.state == DatasetReleaseState::Active
        && active_pointer
            .as_ref()
            .is_some_and(|pointer| pointer.release_id == release.release_id);
    if !rollback && release.state != DatasetReleaseState::Staged && !already_active {
        return Err(AdminOpError::bad_request(
            "activation requires a staged release or the current active release",
        ));
    }
    if already_active
        && active_pointer.as_ref().is_some_and(|pointer| {
            pointer.record_version != request.expected_active_pointer_version
        })
    {
        return Err(AdminOpError::conflict(
            "active release pointer version changed",
        ));
    }
    let previous_release_id = active_pointer
        .as_ref()
        .map(|pointer| pointer.release_id.clone());
    let source = state
        .catalog
        .source(scope, &release.source_id)
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
                scope,
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
            .invalidate_routes_for_release(scope, &previous_release_id)
            .await?;
        if let Some(previous) = state.catalog.release(scope, &previous_release_id).await?
            && previous.state == DatasetReleaseState::Active
        {
            let expected = previous.record_version;
            state
                .catalog
                .transition_release(scope, previous, DatasetReleaseState::Retired, expected)
                .await?;
        }
        invalidated
    } else {
        0
    };
    state
        .subscriptions
        .notify_resource_updated(uris::DATASETS_URI)
        .await;
    state
        .subscriptions
        .notify_resource_updated(uris::ROUTES_URI)
        .await;
    Ok(ReleaseMutationResponse {
        release,
        invalidated_route_count,
    })
}

pub async fn quarantine_release(
    state: &MapApplication,
    scope: &MapScope,
    request: ReleaseMutationRequest,
) -> OpResult<ReleaseMutationResponse> {
    let _activation = state.activation.lock().await;
    let release = state
        .catalog
        .release(scope, &request.release_id)
        .await?
        .context("unknown dataset release")?;
    if state
        .catalog
        .active_release_id(scope, &release.dataset_id)
        .await?
        .as_ref()
        == Some(&release.release_id)
    {
        return Err(AdminOpError::conflict(
            "an active release must be replaced before it can be quarantined",
        ));
    }
    let release = state
        .catalog
        .transition_release(
            scope,
            release,
            DatasetReleaseState::Quarantined,
            request.expected_record_version,
        )
        .await?;
    let invalidated_route_count = state
        .catalog
        .invalidate_routes_for_release(scope, &release.release_id)
        .await?;
    state
        .subscriptions
        .notify_resource_updated(uris::DATASETS_URI)
        .await;
    state
        .subscriptions
        .notify_resource_updated(uris::ROUTES_URI)
        .await;
    Ok(ReleaseMutationResponse {
        release,
        invalidated_route_count,
    })
}

fn validate_idempotency_key(value: &str) -> Result<(), AdminOpError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(AdminOpError::bad_request("invalid idempotency key"));
    }
    Ok(())
}
