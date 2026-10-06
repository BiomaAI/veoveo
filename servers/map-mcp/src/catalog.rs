use crate::persistence::MapRepository;
use anyhow::{Context, Result, anyhow, bail};
pub mod mobility;
pub mod owned;
pub mod releases;
pub mod restrictions;
pub mod routing_authority;
pub mod sources;
use crate::persistence::{
    MapAcquisitionDraft, MapAcquisitionState, MapAcquisitionUpdate, MapDependencyIdentity,
    MapMobilityProfileDraft, MapOperationalSnapshotDraft, MapReleaseDraft, MapReleaseState,
    MapRestrictionDraft, MapRouteDependencyDraft, MapRouteDraft, MapRouteMatrixDraft,
    MapRouteState, MapSourceDraft,
};
use chrono::Utc;
use veoveo_platform_store::{PlatformIdentity, PlatformStore};

use crate::contract::{
    AcquisitionJob, AcquisitionPhase, AcquisitionProgress, AcquisitionStatus, ActiveReleasePointer,
    CreateAcquisitionRequest, DatasetRelease, DatasetReleaseState, MobilityProfile,
    OperationalSnapshot, RegisteredSource, Restriction, RouteMatrix, RoutePlan, RouteStatus,
};

#[derive(Clone, Debug)]
pub struct MapAccessContext {
    pub identity: PlatformIdentity,
}

impl MapAccessContext {
    pub fn tenant_key(&self) -> String {
        self.identity.tenant_id.to_string()
    }
}

#[derive(Clone, Debug)]
pub struct MapCatalog {
    repository: MapRepository,
}

impl MapCatalog {
    pub fn new(store: PlatformStore) -> Self {
        Self {
            repository: MapRepository::new(store),
        }
    }

    pub fn store(&self) -> &PlatformStore {
        self.repository.platform()
    }

    pub async fn create_source(
        &self,
        scope: &MapAccessContext,
        source: RegisteredSource,
    ) -> Result<RegisteredSource> {
        source.validate()?;
        let draft = source_draft(scope, &source)?;
        self.repository.create_map_source(draft).await?;
        Ok(source)
    }

    pub async fn replace_source(
        &self,
        scope: &MapAccessContext,
        source: RegisteredSource,
        expected_record_version: u64,
    ) -> Result<RegisteredSource> {
        source.validate()?;
        if source.record_version != expected_record_version + 1 {
            bail!("replacement source record_version must increment expected_record_version");
        }
        let draft = source_draft(scope, &source)?;
        self.repository
            .replace_map_source(draft, integer_version(expected_record_version)?)
            .await?;
        Ok(source)
    }

    pub async fn create_release(
        &self,
        scope: &MapAccessContext,
        release: DatasetRelease,
    ) -> Result<DatasetRelease> {
        release.validate()?;
        self.repository
            .create_map_release(MapReleaseDraft {
                identity: scope.identity.clone(),
                release_key: release.release_id.clone(),
                dataset_key: release.dataset_id.clone(),
                source_key: release.source_id.clone(),
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
        scope: &MapAccessContext,
        release_id: &crate::contract::DatasetReleaseId,
    ) -> Result<Option<DatasetRelease>> {
        self.repository
            .map_release(scope.identity.tenant_id, release_id)
            .await?
            .map(releases::checked_release)
            .transpose()
    }

    pub async fn list_releases(&self, scope: &MapAccessContext) -> Result<Vec<DatasetRelease>> {
        self.repository
            .list_map_releases(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(releases::checked_release)
            .collect()
    }

    pub async fn transition_release(
        &self,
        scope: &MapAccessContext,
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
        self.repository
            .transition_map_release(
                scope.identity.tenant_id,
                &release.release_id,
                integer_version(expected_record_version)?,
                release_state_to_store(state),
                canonical_json,
            )
            .await?;
        Ok(release)
    }

    pub async fn activate_release(
        &self,
        scope: &MapAccessContext,
        mut release: DatasetRelease,
        expected_pointer_version: Option<u64>,
    ) -> Result<DatasetRelease> {
        let expected = expected_pointer_version.map(integer_version).transpose()?;
        let expected_release_version = release.record_version;
        release.state = DatasetReleaseState::Active;
        release.record_version += 1;
        release.updated_at = Utc::now();
        self.repository
            .activate_map_release(
                &scope.identity,
                &release.dataset_id,
                &release.release_id,
                expected,
                integer_version(expected_release_version)?,
                encode(&release)?,
            )
            .await?;
        Ok(release)
    }

    pub async fn active_release_id(
        &self,
        scope: &MapAccessContext,
        dataset_id: &crate::contract::MapDatasetId,
    ) -> Result<Option<crate::contract::DatasetReleaseId>> {
        Ok(self
            .active_release_pointer(scope, dataset_id)
            .await?
            .map(|pointer| pointer.release_id))
    }

    pub async fn active_release_pointer(
        &self,
        scope: &MapAccessContext,
        dataset_id: &crate::contract::MapDatasetId,
    ) -> Result<Option<ActiveReleasePointer>> {
        let Some(row) = self
            .repository
            .active_map_release(scope.identity.tenant_id, dataset_id)
            .await?
        else {
            return Ok(None);
        };
        let pointer = checked_pointer(row, scope.identity.tenant_id)?;
        anyhow::ensure!(
            &pointer.dataset_id == dataset_id,
            "stored active release dataset mismatch"
        );
        let release = self
            .release(scope, &pointer.release_id)
            .await?
            .context("active pointer release is missing")?;
        anyhow::ensure!(
            release.dataset_id == pointer.dataset_id,
            "active pointer release parent mismatch"
        );
        Ok(Some(pointer))
    }

    pub async fn list_active_releases(
        &self,
        scope: &MapAccessContext,
    ) -> Result<Vec<ActiveReleasePointer>> {
        let pointers = self
            .repository
            .list_active_map_releases(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(|row| checked_pointer(row, scope.identity.tenant_id))
            .collect::<Result<Vec<_>>>()?;
        let ids = pointers
            .iter()
            .map(|pointer| pointer.release_id.clone())
            .collect();
        let releases = self.release_set(scope, &ids).await?;
        for pointer in &pointers {
            anyhow::ensure!(
                releases
                    .iter()
                    .any(|release| release.release_id == pointer.release_id
                        && release.dataset_id == pointer.dataset_id),
                "active pointer release parent mismatch"
            );
        }
        Ok(pointers)
    }

    pub async fn create_mobility_profile(
        &self,
        scope: &MapAccessContext,
        profile: MobilityProfile,
    ) -> Result<MobilityProfile> {
        profile.validate()?;
        let metadata = profile.metadata();
        self.repository
            .create_map_mobility_profile(MapMobilityProfileDraft {
                identity: scope.identity.clone(),
                profile_key: metadata.profile_id.clone(),
                family: wire(&profile.family())?,
                name: metadata.name.clone(),
                profile_version: integer_version(metadata.version.get())?,
                valid_from: metadata.valid_from,
                valid_until: metadata.valid_until,
                canonical_json: encode(&profile)?,
            })
            .await?;
        Ok(profile)
    }

    pub async fn create_restriction(
        &self,
        scope: &MapAccessContext,
        restriction: Restriction,
    ) -> Result<Restriction> {
        validate_restriction(&restriction)?;
        self.repository
            .create_map_restriction(MapRestrictionDraft {
                identity: scope.identity.clone(),
                restriction_key: restriction.restriction_id.clone(),
                kind: wire(&restriction.kind)?,
                effect_kind: wire(&restriction.effect.kind)?,
                affected_mobility_families: restriction
                    .affected_mobility_families
                    .iter()
                    .map(wire)
                    .collect::<Result<Vec<_>>>()?,
                valid_from: restriction.valid_from,
                valid_until: restriction.valid_until,
                cancelled_by: restriction.cancelled_by.as_ref().cloned(),
                canonical_json: encode(&restriction)?,
            })
            .await?;
        Ok(restriction)
    }

    pub async fn withdraw_restriction(
        &self,
        scope: &MapAccessContext,
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
        self.repository
            .replace_map_restriction(
                scope.identity.tenant_id,
                &restriction.restriction_id,
                integer_version(expected_record_version)?,
                restriction.valid_until,
                restriction.cancelled_by.as_ref().cloned(),
                encode(&restriction)?,
            )
            .await?;
        Ok(restriction)
    }

    pub async fn persist_snapshot(
        &self,
        scope: &MapAccessContext,
        snapshot: &OperationalSnapshot,
    ) -> Result<()> {
        snapshot.coverage.validate()?;
        self.repository
            .create_map_operational_snapshot(MapOperationalSnapshotDraft {
                tenant_id: scope.identity.tenant_id,
                snapshot_key: snapshot.snapshot_id.clone(),
                departure_time: snapshot.departure_time,
                canonical_json: encode(snapshot)?,
            })
            .await?;
        Ok(())
    }

    pub async fn persist_route(
        &self,
        scope: &MapAccessContext,
        route: &RoutePlan,
        cache_digest_sha256: String,
    ) -> Result<()> {
        self.repository
            .create_map_route(MapRouteDraft {
                identity: scope.identity.clone(),
                route_key: route.route_id.clone(),
                status: route_state_to_store(route.status),
                mobility_profile_key: route.mobility_profile_id.clone(),
                mobility_profile_version: integer_version(route.mobility_profile_version.get())?,
                operational_snapshot_key: route.provenance.operational_snapshot_id.clone(),
                departure_time: route.departure_time,
                arrival_time: route.arrival_time,
                cache_digest_sha256,
                canonical_json: encode(route)?,
                base_release_ids: route.provenance.base_release_ids.iter().cloned().collect(),
                restriction_ids: route.restriction_ids.iter().cloned().collect(),
                facility_ids: route.facility_ids.iter().cloned().collect(),
            })
            .await?;
        for release_id in &route.provenance.base_release_ids {
            self.persist_route_dependency(
                scope,
                &route.route_id,
                MapDependencyIdentity::Release(release_id.clone()),
            )
            .await?;
        }
        for restriction_id in &route.restriction_ids {
            self.persist_route_dependency(
                scope,
                &route.route_id,
                MapDependencyIdentity::Restriction(restriction_id.clone()),
            )
            .await?;
        }
        for facility_id in &route.facility_ids {
            self.persist_route_dependency(
                scope,
                &route.route_id,
                MapDependencyIdentity::Facility(facility_id.clone()),
            )
            .await?;
        }
        Ok(())
    }

    async fn persist_route_dependency(
        &self,
        scope: &MapAccessContext,
        route_id: &crate::contract::RouteId,
        dependency: MapDependencyIdentity,
    ) -> Result<()> {
        self.repository
            .create_map_route_dependency(MapRouteDependencyDraft {
                tenant_id: scope.identity.tenant_id,
                route_key: route_id.clone(),
                dependency,
            })
            .await?;
        Ok(())
    }

    pub async fn persist_matrix(
        &self,
        scope: &MapAccessContext,
        matrix: &RouteMatrix,
        mobility_profile_id: &crate::contract::MobilityProfileId,
        mobility_profile_version: crate::contract::MobilityProfileVersion,
    ) -> Result<()> {
        self.repository
            .create_map_route_matrix(MapRouteMatrixDraft {
                identity: scope.identity.clone(),
                matrix_key: matrix.matrix_id.clone(),
                mobility_profile_key: mobility_profile_id.clone(),
                mobility_profile_version: integer_version(mobility_profile_version.get())?,
                operational_snapshot_key: matrix.provenance.operational_snapshot_id.clone(),
                artifact_uri: None,
                canonical_json: Some(encode(matrix)?),
            })
            .await?;
        Ok(())
    }

    pub async fn create_acquisition(
        &self,
        scope: &MapAccessContext,
        request: CreateAcquisitionRequest,
        acquisition_id: crate::contract::AcquisitionId,
    ) -> Result<AcquisitionJob> {
        request.requested_coverage.validate()?;
        if let Some(digest) = &request.expected_source_digest_sha256
            && (digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            bail!("expected_source_digest_sha256 must be a 64-character hexadecimal digest");
        }
        if let Some(record) = self
            .repository
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
        let job = AcquisitionJob::new(crate::contract::AcquisitionJobValue {
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
        })?;
        self.repository
            .create_map_acquisition(MapAcquisitionDraft {
                identity: scope.identity.clone(),
                acquisition_key: job.acquisition_id.clone(),
                source_key: job.source_id.clone(),
                idempotency_key: request.idempotency_key,
                status: MapAcquisitionState::Queued,
                phase: "queued".to_owned(),
                staged_release_key: None,
                canonical_json: encode(&job)?,
            })
            .await?;
        Ok(job)
    }

    pub async fn update_acquisition(
        &self,
        scope: &MapAccessContext,
        mut job: AcquisitionJob,
    ) -> Result<AcquisitionJob> {
        let expected = job.record_version;
        job.record_version += 1;
        job.updated_at = Utc::now();
        self.repository
            .update_map_acquisition(MapAcquisitionUpdate {
                identity: scope.identity.clone(),
                acquisition_key: job.acquisition_id.clone(),
                expected_record_version: integer_version(expected)?,
                status: acquisition_state_to_store(job.status),
                phase: wire(&job.progress.phase)?,
                staged_release_key: job.staged_release_id.as_ref().cloned(),
                canonical_json: encode(&job)?,
            })
            .await?;
        Ok(job)
    }
}

fn source_draft(scope: &MapAccessContext, source: &RegisteredSource) -> Result<MapSourceDraft> {
    Ok(MapSourceDraft {
        identity: scope.identity.clone(),
        source_key: source.source_id.clone(),
        dataset_key: source.dataset_id.clone(),
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
    veoveo_types::Check::check(&**restriction)?;
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

fn checked_pointer(
    row: crate::persistence::MapActiveReleaseRecord,
    tenant: veoveo_platform_store::TenantId,
) -> Result<ActiveReleasePointer> {
    let dataset_id: crate::contract::MapDatasetId = row.dataset_key.parse()?;
    anyhow::ensure!(
        row.tenant == tenant.record_id()
            && row.id
                == surrealdb::types::RecordId::new(
                    "map_active_release",
                    format!("{tenant}:{dataset_id}")
                )
            && row.record_version > 0,
        "stored active release pointer metadata mismatch"
    );
    Ok(ActiveReleasePointer {
        dataset_id,
        release_id: row.release_key.parse()?,
        previous_release_id: row
            .previous_release_key
            .map(|key| key.parse())
            .transpose()?,
        record_version: u64::try_from(row.record_version)?,
        activated_at: row.activated_at,
    })
}
