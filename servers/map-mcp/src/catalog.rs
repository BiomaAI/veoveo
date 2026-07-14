use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use veoveo_platform_store::{
    MapAcquisitionDraft, MapAcquisitionState, MapDependencyKind, MapMobilityProfileDraft,
    MapOperationalSnapshotDraft, MapReleaseDraft, MapReleaseState, MapRestrictionDraft,
    MapRouteDependencyDraft, MapRouteDraft, MapRouteMatrixDraft, MapRouteState, MapSourceDraft,
    PlatformIdentity, PlatformStore,
};

use crate::contract::{
    AcquisitionJob, AcquisitionPhase, AcquisitionProgress, AcquisitionStatus, ActiveReleasePointer,
    CreateAcquisitionRequest, DatasetRelease, DatasetReleaseState, MapSourceId, MobilityProfile,
    OperationalSnapshot, RegisteredSource, Restriction, RouteMatrix, RoutePlan, RouteStatus,
};

#[derive(Clone, Debug)]
pub struct MapScope {
    pub identity: PlatformIdentity,
}

impl MapScope {
    pub fn tenant_key(&self) -> String {
        self.identity.tenant_id.to_string()
    }
}

#[derive(Clone, Debug)]
pub struct MapCatalog {
    store: PlatformStore,
}

impl MapCatalog {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &PlatformStore {
        &self.store
    }

    pub async fn create_source(
        &self,
        scope: &MapScope,
        source: RegisteredSource,
    ) -> Result<RegisteredSource> {
        source.validate()?;
        let draft = source_draft(scope, &source)?;
        self.store.create_map_source(draft).await?;
        Ok(source)
    }

    pub async fn replace_source(
        &self,
        scope: &MapScope,
        source: RegisteredSource,
        expected_record_version: u64,
    ) -> Result<RegisteredSource> {
        source.validate()?;
        if source.record_version != expected_record_version + 1 {
            bail!("replacement source record_version must increment expected_record_version");
        }
        let draft = source_draft(scope, &source)?;
        self.store
            .replace_map_source(draft, integer_version(expected_record_version)?)
            .await?;
        Ok(source)
    }

    pub async fn source(
        &self,
        scope: &MapScope,
        source_id: &MapSourceId,
    ) -> Result<Option<RegisteredSource>> {
        self.store
            .map_source(scope.identity.tenant_id, source_id.as_str())
            .await?
            .map(|record| decode(&record.canonical_json, "map source"))
            .transpose()
    }

    pub async fn list_sources(&self, scope: &MapScope) -> Result<Vec<RegisteredSource>> {
        self.store
            .list_map_sources(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(|record| decode(&record.canonical_json, "map source"))
            .collect()
    }

    pub async fn create_release(
        &self,
        scope: &MapScope,
        release: DatasetRelease,
    ) -> Result<DatasetRelease> {
        release.validate()?;
        self.store
            .create_map_release(MapReleaseDraft {
                identity: scope.identity.clone(),
                release_key: release.release_id.to_string(),
                dataset_key: release.dataset_id.to_string(),
                source_key: release.source_id.to_string(),
                state: release_state_to_store(release.state),
                version_label: release.version_label.clone(),
                source_digest_sha256: release.source_digest_sha256.clone(),
                valid_from: release.valid_from,
                valid_until: release.valid_until,
                canonical_json: encode(&release)?,
            })
            .await?;
        Ok(release)
    }

    pub async fn release(
        &self,
        scope: &MapScope,
        release_id: &crate::contract::DatasetReleaseId,
    ) -> Result<Option<DatasetRelease>> {
        self.store
            .map_release(scope.identity.tenant_id, release_id.as_str())
            .await?
            .map(|record| decode(&record.canonical_json, "dataset release"))
            .transpose()
    }

    pub async fn list_releases(&self, scope: &MapScope) -> Result<Vec<DatasetRelease>> {
        self.store
            .list_map_releases(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(|record| decode(&record.canonical_json, "dataset release"))
            .collect()
    }

    pub async fn transition_release(
        &self,
        scope: &MapScope,
        mut release: DatasetRelease,
        state: DatasetReleaseState,
        expected_record_version: u64,
    ) -> Result<DatasetRelease> {
        if release.record_version != expected_record_version {
            bail!("release representation does not match expected record version");
        }
        release.state = state;
        release.record_version += 1;
        release.updated_at = Utc::now();
        let canonical_json = encode(&release)?;
        self.store
            .transition_map_release(
                scope.identity.tenant_id,
                release.release_id.as_str(),
                integer_version(expected_record_version)?,
                release_state_to_store(state),
                canonical_json,
            )
            .await?;
        Ok(release)
    }

    pub async fn activate_release(
        &self,
        scope: &MapScope,
        mut release: DatasetRelease,
        expected_pointer_version: Option<u64>,
    ) -> Result<DatasetRelease> {
        let expected = expected_pointer_version.map(integer_version).transpose()?;
        let expected_release_version = release.record_version;
        release.state = DatasetReleaseState::Active;
        release.record_version += 1;
        release.updated_at = Utc::now();
        self.store
            .activate_map_release(
                &scope.identity,
                release.dataset_id.as_str(),
                release.release_id.as_str(),
                expected,
                integer_version(expected_release_version)?,
                encode(&release)?,
            )
            .await?;
        Ok(release)
    }

    pub async fn active_release_id(
        &self,
        scope: &MapScope,
        dataset_id: &crate::contract::MapDatasetId,
    ) -> Result<Option<crate::contract::DatasetReleaseId>> {
        self.store
            .active_map_release(scope.identity.tenant_id, dataset_id.as_str())
            .await?
            .map(|record| record.release_key.parse())
            .transpose()
            .map_err(Into::into)
    }

    pub async fn list_active_releases(
        &self,
        scope: &MapScope,
    ) -> Result<Vec<ActiveReleasePointer>> {
        self.store
            .list_active_map_releases(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(|record| {
                Ok(ActiveReleasePointer {
                    dataset_id: record.dataset_key.parse()?,
                    release_id: record.release_key.parse()?,
                    previous_release_id: record
                        .previous_release_key
                        .map(|value| value.parse())
                        .transpose()?,
                    record_version: u64::try_from(record.record_version)?,
                    activated_at: record.activated_at,
                })
            })
            .collect()
    }

    pub async fn create_mobility_profile(
        &self,
        scope: &MapScope,
        profile: MobilityProfile,
    ) -> Result<MobilityProfile> {
        profile.validate()?;
        let metadata = profile.metadata();
        self.store
            .create_map_mobility_profile(MapMobilityProfileDraft {
                identity: scope.identity.clone(),
                profile_key: metadata.profile_id.to_string(),
                family: wire(&profile.family())?,
                name: metadata.name.clone(),
                profile_version: integer_version(metadata.version)?,
                valid_from: metadata.valid_from,
                valid_until: metadata.valid_until,
                canonical_json: encode(&profile)?,
            })
            .await?;
        Ok(profile)
    }

    pub async fn mobility_profile(
        &self,
        scope: &MapScope,
        profile_id: &crate::contract::MobilityProfileId,
        version: u64,
    ) -> Result<Option<MobilityProfile>> {
        self.store
            .map_mobility_profile(
                scope.identity.tenant_id,
                profile_id.as_str(),
                integer_version(version)?,
            )
            .await?
            .map(|record| decode(&record.canonical_json, "mobility profile"))
            .transpose()
    }

    pub async fn list_mobility_profiles(&self, scope: &MapScope) -> Result<Vec<MobilityProfile>> {
        self.store
            .list_map_mobility_profiles(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(|record| decode(&record.canonical_json, "mobility profile"))
            .collect()
    }

    pub async fn create_restriction(
        &self,
        scope: &MapScope,
        restriction: Restriction,
    ) -> Result<Restriction> {
        validate_restriction(&restriction)?;
        self.store
            .create_map_restriction(MapRestrictionDraft {
                identity: scope.identity.clone(),
                restriction_key: restriction.restriction_id.to_string(),
                kind: wire(&restriction.kind)?,
                effect_kind: wire(&restriction.effect.kind)?,
                affected_mobility_families: restriction
                    .affected_mobility_families
                    .iter()
                    .map(wire)
                    .collect::<Result<Vec<_>>>()?,
                valid_from: restriction.valid_from,
                valid_until: restriction.valid_until,
                cancelled_by: restriction.cancelled_by.as_ref().map(ToString::to_string),
                canonical_json: encode(&restriction)?,
            })
            .await?;
        Ok(restriction)
    }

    pub async fn restriction(
        &self,
        scope: &MapScope,
        restriction_id: &crate::contract::RestrictionId,
    ) -> Result<Option<Restriction>> {
        self.store
            .map_restriction(scope.identity.tenant_id, restriction_id.as_str())
            .await?
            .map(|record| decode(&record.canonical_json, "restriction"))
            .transpose()
    }

    pub async fn list_restrictions(&self, scope: &MapScope) -> Result<Vec<Restriction>> {
        self.store
            .list_map_restrictions(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(|record| decode(&record.canonical_json, "restriction"))
            .collect()
    }

    pub async fn withdraw_restriction(
        &self,
        scope: &MapScope,
        mut restriction: Restriction,
        expected_record_version: u64,
        effective_at: chrono::DateTime<Utc>,
        cancelled_by: crate::contract::RestrictionId,
    ) -> Result<Restriction> {
        if restriction.record_version != expected_record_version {
            bail!("restriction representation does not match expected record version");
        }
        if effective_at < restriction.valid_from
            || restriction
                .valid_until
                .is_some_and(|until| effective_at > until)
        {
            bail!("restriction withdrawal time is outside its validity interval");
        }
        restriction.valid_until = Some(effective_at);
        restriction.cancelled_by = Some(cancelled_by);
        restriction.record_version += 1;
        validate_restriction(&restriction)?;
        self.store
            .replace_map_restriction(
                scope.identity.tenant_id,
                restriction.restriction_id.as_str(),
                integer_version(expected_record_version)?,
                restriction.valid_until,
                restriction.cancelled_by.as_ref().map(ToString::to_string),
                encode(&restriction)?,
            )
            .await?;
        Ok(restriction)
    }

    pub async fn persist_snapshot(
        &self,
        scope: &MapScope,
        snapshot: &OperationalSnapshot,
    ) -> Result<()> {
        snapshot.coverage.validate()?;
        self.store
            .create_map_operational_snapshot(MapOperationalSnapshotDraft {
                tenant_id: scope.identity.tenant_id,
                snapshot_key: snapshot.snapshot_id.to_string(),
                departure_time: snapshot.departure_time,
                canonical_json: encode(snapshot)?,
            })
            .await?;
        Ok(())
    }

    pub async fn persist_route(
        &self,
        scope: &MapScope,
        route: &RoutePlan,
        cache_digest_sha256: String,
    ) -> Result<()> {
        self.store
            .create_map_route(MapRouteDraft {
                identity: scope.identity.clone(),
                route_key: route.route_id.to_string(),
                status: route_state_to_store(route.status),
                mobility_profile_key: route.mobility_profile_id.to_string(),
                mobility_profile_version: integer_version(route.mobility_profile_version)?,
                operational_snapshot_key: route.provenance.operational_snapshot_id.to_string(),
                departure_time: route.departure_time,
                arrival_time: route.arrival_time,
                cache_digest_sha256,
                canonical_json: encode(route)?,
            })
            .await?;
        for release_id in &route.provenance.base_release_ids {
            self.persist_route_dependency(
                scope,
                &route.route_id,
                MapDependencyKind::Release,
                release_id.as_str(),
            )
            .await?;
        }
        for restriction_id in &route.restriction_ids {
            self.persist_route_dependency(
                scope,
                &route.route_id,
                MapDependencyKind::Restriction,
                restriction_id.as_str(),
            )
            .await?;
        }
        for facility_id in &route.facility_ids {
            self.persist_route_dependency(
                scope,
                &route.route_id,
                MapDependencyKind::Facility,
                facility_id.as_str(),
            )
            .await?;
        }
        Ok(())
    }

    async fn persist_route_dependency(
        &self,
        scope: &MapScope,
        route_id: &crate::contract::RouteId,
        dependency_kind: MapDependencyKind,
        dependency_key: &str,
    ) -> Result<()> {
        self.store
            .create_map_route_dependency(MapRouteDependencyDraft {
                tenant_id: scope.identity.tenant_id,
                route_key: route_id.to_string(),
                dependency_kind,
                dependency_key: dependency_key.to_owned(),
            })
            .await?;
        Ok(())
    }

    pub async fn invalidate_routes_for_release(
        &self,
        scope: &MapScope,
        release_id: &crate::contract::DatasetReleaseId,
    ) -> Result<u64> {
        self.invalidate_routes(scope, |route| {
            route.provenance.base_release_ids.contains(release_id)
        })
        .await
    }

    pub async fn invalidate_routes_for_restriction(
        &self,
        scope: &MapScope,
        restriction_id: &crate::contract::RestrictionId,
    ) -> Result<u64> {
        self.invalidate_routes(scope, |route| {
            route.restriction_ids.contains(restriction_id)
        })
        .await
    }

    async fn invalidate_routes(
        &self,
        scope: &MapScope,
        predicate: impl Fn(&RoutePlan) -> bool,
    ) -> Result<u64> {
        let mut count = 0_u64;
        for record in self.store.list_map_routes(scope.identity.tenant_id).await? {
            let mut route: RoutePlan = decode(&record.canonical_json, "route")?;
            if route.status == RouteStatus::Invalidated || !predicate(&route) {
                continue;
            }
            route.status = RouteStatus::Invalidated;
            self.store
                .set_map_route_state(
                    scope.identity.tenant_id,
                    route.route_id.as_str(),
                    MapRouteState::Invalidated,
                    encode(&route)?,
                )
                .await?;
            count += 1;
        }
        Ok(count)
    }

    pub async fn persist_matrix(
        &self,
        scope: &MapScope,
        matrix: &RouteMatrix,
        mobility_profile_key: &str,
        mobility_profile_version: u64,
    ) -> Result<()> {
        self.store
            .create_map_route_matrix(MapRouteMatrixDraft {
                identity: scope.identity.clone(),
                matrix_key: matrix.matrix_id.to_string(),
                mobility_profile_key: mobility_profile_key.to_owned(),
                mobility_profile_version: integer_version(mobility_profile_version)?,
                operational_snapshot_key: matrix.provenance.operational_snapshot_id.to_string(),
                artifact_uri: None,
                canonical_json: Some(encode(matrix)?),
            })
            .await?;
        Ok(())
    }

    pub async fn matrix(
        &self,
        scope: &MapScope,
        matrix_id: &crate::contract::RouteMatrixId,
    ) -> Result<Option<RouteMatrix>> {
        let record = self
            .store
            .map_route_matrix(scope.identity.tenant_id, matrix_id.as_str())
            .await?;
        let Some(record) = record else {
            return Ok(None);
        };
        if record.owner != scope.identity.principal_id.record_id() {
            return Ok(None);
        }
        record
            .canonical_json
            .map(|value| decode(&value, "route matrix"))
            .transpose()
    }

    pub async fn list_matrices(&self, scope: &MapScope) -> Result<Vec<RouteMatrix>> {
        self.store
            .list_map_route_matrices(scope.identity.tenant_id)
            .await?
            .into_iter()
            .filter(|record| record.owner == scope.identity.principal_id.record_id())
            .filter_map(|record| record.canonical_json)
            .map(|value| decode(&value, "route matrix"))
            .collect()
    }

    pub async fn route(
        &self,
        scope: &MapScope,
        route_id: &crate::contract::RouteId,
    ) -> Result<Option<RoutePlan>> {
        let record = self
            .store
            .map_route(scope.identity.tenant_id, route_id.as_str())
            .await?;
        let Some(record) = record else {
            return Ok(None);
        };
        if record.owner != scope.identity.principal_id.record_id() {
            return Ok(None);
        }
        Ok(Some(decode(&record.canonical_json, "route")?))
    }

    pub async fn list_routes(&self, scope: &MapScope) -> Result<Vec<RoutePlan>> {
        self.store
            .list_map_routes(scope.identity.tenant_id)
            .await?
            .into_iter()
            .filter(|record| record.owner == scope.identity.principal_id.record_id())
            .map(|record| decode(&record.canonical_json, "route"))
            .collect()
    }

    pub async fn create_acquisition(
        &self,
        scope: &MapScope,
        request: CreateAcquisitionRequest,
        acquisition_id: crate::contract::AcquisitionId,
    ) -> Result<AcquisitionJob> {
        request.requested_coverage.validate()?;
        if let Some(digest) = &request.expected_source_digest_sha256 {
            if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!("expected_source_digest_sha256 must be a 64-character hexadecimal digest");
            }
        }
        if let Some(record) = self
            .store
            .map_acquisition_for_idempotency(
                scope.identity.tenant_id,
                scope.identity.principal_id.record_id(),
                &request.idempotency_key,
            )
            .await?
        {
            let existing: AcquisitionJob = decode(&record.canonical_json, "acquisition job")?;
            if existing.source_id == request.source_id
                && existing.requested_coverage == request.requested_coverage
                && existing.expected_source_digest_sha256 == request.expected_source_digest_sha256
            {
                return Ok(existing);
            }
            bail!("acquisition idempotency key conflicts with a different request");
        }
        let now = Utc::now();
        let job = AcquisitionJob {
            acquisition_id,
            source_id: request.source_id,
            requested_coverage: request.requested_coverage,
            expected_source_digest_sha256: request.expected_source_digest_sha256,
            status: AcquisitionStatus::Queued,
            progress: AcquisitionProgress {
                phase: AcquisitionPhase::Queued,
                completed_units: 0,
                total_units: None,
                message: "queued".to_owned(),
            },
            raw_artifact_uri: None,
            staged_release_id: None,
            diagnostics_uri: None,
            created_by: scope.identity.principal_id.to_string(),
            created_at: now,
            updated_at: now,
            record_version: 1,
        };
        self.store
            .create_map_acquisition(MapAcquisitionDraft {
                identity: scope.identity.clone(),
                acquisition_key: job.acquisition_id.to_string(),
                source_key: job.source_id.to_string(),
                idempotency_key: request.idempotency_key,
                status: MapAcquisitionState::Queued,
                phase: "queued".to_owned(),
                staged_release_key: None,
                canonical_json: encode(&job)?,
            })
            .await?;
        Ok(job)
    }

    pub async fn acquisition(
        &self,
        scope: &MapScope,
        acquisition_id: &crate::contract::AcquisitionId,
    ) -> Result<Option<AcquisitionJob>> {
        let record = self
            .store
            .map_acquisition(scope.identity.tenant_id, acquisition_id.as_str())
            .await?;
        let Some(record) = record else {
            return Ok(None);
        };
        if record.owner != scope.identity.principal_id.record_id() {
            return Ok(None);
        }
        Ok(Some(decode(&record.canonical_json, "acquisition job")?))
    }

    pub async fn list_acquisitions(&self, scope: &MapScope) -> Result<Vec<AcquisitionJob>> {
        self.store
            .list_map_acquisitions(scope.identity.tenant_id)
            .await?
            .into_iter()
            .filter(|record| record.owner == scope.identity.principal_id.record_id())
            .map(|record| decode(&record.canonical_json, "acquisition job"))
            .collect()
    }

    pub async fn update_acquisition(
        &self,
        scope: &MapScope,
        mut job: AcquisitionJob,
    ) -> Result<AcquisitionJob> {
        let expected = job.record_version;
        job.record_version += 1;
        job.updated_at = Utc::now();
        self.store
            .update_map_acquisition(
                scope.identity.tenant_id,
                job.acquisition_id.as_str(),
                integer_version(expected)?,
                acquisition_state_to_store(job.status),
                &wire(&job.progress.phase)?,
                job.staged_release_id.as_ref().map(ToString::to_string),
                encode(&job)?,
            )
            .await?;
        Ok(job)
    }
}

fn source_draft(scope: &MapScope, source: &RegisteredSource) -> Result<MapSourceDraft> {
    Ok(MapSourceDraft {
        identity: scope.identity.clone(),
        source_key: source.source_id.to_string(),
        dataset_key: source.dataset_id.to_string(),
        name: source.name.clone(),
        adapter_kind: wire(&source.adapter_kind)?,
        authority_class: wire(&source.authority)?,
        map_families: source
            .map_families
            .iter()
            .map(wire)
            .collect::<Result<Vec<_>>>()?,
        enabled: source.enabled,
        canonical_json: encode(source)?,
    })
}

fn validate_restriction(restriction: &Restriction) -> Result<()> {
    restriction.geometry.validate()?;
    if restriction.affected_mobility_families.is_empty()
        || restriction.record_version == 0
        || restriction
            .valid_until
            .is_some_and(|until| until <= restriction.valid_from)
    {
        bail!("restriction invariants are invalid");
    }
    Ok(())
}

fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("encoding canonical map record")
}

fn decode<T: serde::de::DeserializeOwned>(value: &str, kind: &str) -> Result<T> {
    serde_json::from_str(value).with_context(|| format!("decoding canonical {kind} record"))
}

fn wire<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("controlled enum did not serialize to a string"))
}

fn integer_version(version: u64) -> Result<i64> {
    i64::try_from(version).context("map record version exceeds storage range")
}

fn release_state_to_store(state: DatasetReleaseState) -> MapReleaseState {
    match state {
        DatasetReleaseState::Staged => MapReleaseState::Staged,
        DatasetReleaseState::Active => MapReleaseState::Active,
        DatasetReleaseState::Retired => MapReleaseState::Retired,
        DatasetReleaseState::Quarantined => MapReleaseState::Quarantined,
    }
}

fn route_state_to_store(state: RouteStatus) -> MapRouteState {
    match state {
        RouteStatus::PlanningAdvisory => MapRouteState::PlanningAdvisory,
        RouteStatus::Validated => MapRouteState::Validated,
        RouteStatus::Stale => MapRouteState::Stale,
        RouteStatus::Invalidated => MapRouteState::Invalidated,
        RouteStatus::Unavailable => MapRouteState::Unavailable,
    }
}

fn acquisition_state_to_store(state: AcquisitionStatus) -> MapAcquisitionState {
    match state {
        AcquisitionStatus::Queued => MapAcquisitionState::Queued,
        AcquisitionStatus::Running => MapAcquisitionState::Running,
        AcquisitionStatus::Succeeded => MapAcquisitionState::Succeeded,
        AcquisitionStatus::Failed => MapAcquisitionState::Failed,
        AcquisitionStatus::CancelRequested => MapAcquisitionState::CancelRequested,
        AcquisitionStatus::Cancelled => MapAcquisitionState::Cancelled,
    }
}
