use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use crate::{
    MapAcquisitionRecord, MapAcquisitionState, MapActiveReleaseRecord, MapDatasetReleaseRecord,
    MapMobilityProfileRecord, MapOperationalSnapshotRecord, MapReleaseState, MapRestrictionRecord,
    MapRouteDependencyRecord, MapRouteMatrixRecord, MapRouteRecord, MapRouteState, MapSourceRecord,
    PlatformIdentity, PlatformStore, StoreError, TenantId,
};

const MAX_CATALOG_JSON_BYTES: usize = 2 * 1024 * 1024;
const MAX_ROUTE_JSON_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct MapSourceDraft {
    pub identity: PlatformIdentity,
    pub source_key: String,
    pub dataset_key: String,
    pub name: String,
    pub adapter_kind: String,
    pub authority_class: String,
    pub map_families: Vec<String>,
    pub enabled: bool,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapReleaseDraft {
    pub identity: PlatformIdentity,
    pub release_key: String,
    pub dataset_key: String,
    pub source_key: String,
    pub state: MapReleaseState,
    pub version_label: String,
    pub source_digest_sha256: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapMobilityProfileDraft {
    pub identity: PlatformIdentity,
    pub profile_key: String,
    pub family: String,
    pub name: String,
    pub profile_version: i64,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapRestrictionDraft {
    pub identity: PlatformIdentity,
    pub restriction_key: String,
    pub kind: String,
    pub effect_kind: String,
    pub affected_mobility_families: Vec<String>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub cancelled_by: Option<String>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapOperationalSnapshotDraft {
    pub tenant_id: TenantId,
    pub snapshot_key: String,
    pub departure_time: DateTime<Utc>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapRouteDraft {
    pub identity: PlatformIdentity,
    pub route_key: String,
    pub status: MapRouteState,
    pub mobility_profile_key: String,
    pub mobility_profile_version: i64,
    pub operational_snapshot_key: String,
    pub departure_time: DateTime<Utc>,
    pub arrival_time: Option<DateTime<Utc>>,
    pub cache_digest_sha256: String,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapRouteDependencyDraft {
    pub tenant_id: TenantId,
    pub route_key: String,
    pub dependency_kind: crate::MapDependencyKind,
    pub dependency_key: String,
}

#[derive(Clone, Debug)]
pub struct MapRouteMatrixDraft {
    pub identity: PlatformIdentity,
    pub matrix_key: String,
    pub mobility_profile_key: String,
    pub mobility_profile_version: i64,
    pub operational_snapshot_key: String,
    pub artifact_uri: Option<String>,
    pub canonical_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MapAcquisitionDraft {
    pub identity: PlatformIdentity,
    pub acquisition_key: String,
    pub source_key: String,
    pub idempotency_key: String,
    pub status: MapAcquisitionState,
    pub phase: String,
    pub staged_release_key: Option<String>,
    pub canonical_json: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct MapSourceContent {
    tenant: RecordId,
    owner: RecordId,
    source_key: String,
    dataset_key: String,
    name: String,
    adapter_kind: String,
    authority_class: String,
    map_families: Vec<String>,
    enabled: bool,
    canonical_json: String,
    record_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct MapReleaseContent {
    tenant: RecordId,
    release_key: String,
    dataset_key: String,
    source_key: String,
    state: MapReleaseState,
    version_label: String,
    source_digest_sha256: String,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    canonical_json: String,
    record_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct MapMobilityProfileContent {
    tenant: RecordId,
    owner: RecordId,
    profile_key: String,
    family: String,
    name: String,
    profile_version: i64,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    canonical_json: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct MapRestrictionContent {
    tenant: RecordId,
    owner: RecordId,
    restriction_key: String,
    kind: String,
    effect_kind: String,
    affected_mobility_families: Vec<String>,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    cancelled_by: Option<String>,
    canonical_json: String,
    record_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct MapOperationalSnapshotContent {
    tenant: RecordId,
    snapshot_key: String,
    departure_time: DateTime<Utc>,
    canonical_json: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct MapRouteContent {
    tenant: RecordId,
    owner: RecordId,
    route_key: String,
    status: MapRouteState,
    mobility_profile_key: String,
    mobility_profile_version: i64,
    operational_snapshot_key: String,
    departure_time: DateTime<Utc>,
    arrival_time: Option<DateTime<Utc>>,
    cache_digest_sha256: String,
    canonical_json: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct MapRouteDependencyContent {
    tenant: RecordId,
    route_key: String,
    dependency_kind: crate::MapDependencyKind,
    dependency_key: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct MapRouteMatrixContent {
    tenant: RecordId,
    owner: RecordId,
    matrix_key: String,
    mobility_profile_key: String,
    mobility_profile_version: i64,
    operational_snapshot_key: String,
    artifact_uri: Option<String>,
    canonical_json: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct MapAcquisitionContent {
    tenant: RecordId,
    owner: RecordId,
    acquisition_key: String,
    source_key: String,
    idempotency_key: String,
    status: MapAcquisitionState,
    phase: String,
    staged_release_key: Option<String>,
    canonical_json: String,
    record_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PlatformStore {
    pub async fn create_map_source(
        &self,
        mut draft: MapSourceDraft,
    ) -> Result<MapSourceRecord, StoreError> {
        validate_source_draft(&mut draft)?;
        let now = Utc::now();
        let content = MapSourceContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            source_key: draft.source_key.clone(),
            dataset_key: draft.dataset_key,
            name: draft.name,
            adapter_kind: draft.adapter_kind,
            authority_class: draft.authority_class,
            map_families: draft.map_families,
            enabled: draft.enabled,
            canonical_json: draft.canonical_json,
            record_version: 1,
            created_at: now,
            updated_at: now,
        };
        create_only(self, map_record("map_source", &draft.source_key), content).await?;
        self.map_source(draft.identity.tenant_id, &draft.source_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map source creation readback",
            })
    }

    pub async fn replace_map_source(
        &self,
        mut draft: MapSourceDraft,
        expected_record_version: i64,
    ) -> Result<MapSourceRecord, StoreError> {
        validate_source_draft(&mut draft)?;
        if expected_record_version < 1 {
            return Err(invalid_map("expected_record_version", "must be positive"));
        }
        let record = map_record("map_source", &draft.source_key);
        let mut response = self
            .client()
            .query("UPDATE $record MERGE { dataset_key: $dataset_key, name: $name, adapter_kind: $adapter_kind, authority_class: $authority_class, map_families: $map_families, enabled: $enabled, canonical_json: $canonical_json, record_version: $next_version, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
            .bind(("record", record))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("dataset_key", draft.dataset_key))
            .bind(("name", draft.name))
            .bind(("adapter_kind", draft.adapter_kind))
            .bind(("authority_class", draft.authority_class))
            .bind(("map_families", draft.map_families))
            .bind(("enabled", draft.enabled))
            .bind(("canonical_json", draft.canonical_json))
            .bind(("expected", expected_record_version))
            .bind(("next_version", expected_record_version + 1))
            .await?
            .check()?;
        response
            .take::<Option<MapSourceRecord>>(0)?
            .ok_or_else(|| StoreError::MapRecordConflict {
                entity: "source",
                key: draft.source_key,
            })
    }

    pub async fn map_source(
        &self,
        tenant_id: TenantId,
        source_key: &str,
    ) -> Result<Option<MapSourceRecord>, StoreError> {
        validate_public_key("source_key", source_key, "source-")?;
        select_one(self, map_record("map_source", source_key), tenant_id).await
    }

    pub async fn list_map_sources(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<MapSourceRecord>, StoreError> {
        select_tenant_list(
            self,
            "SELECT * FROM map_source WHERE tenant = $tenant ORDER BY name ASC;",
            tenant_id,
        )
        .await
    }

    pub async fn create_map_release(
        &self,
        draft: MapReleaseDraft,
    ) -> Result<MapDatasetReleaseRecord, StoreError> {
        validate_release_draft(&draft)?;
        let now = Utc::now();
        let content = MapReleaseContent {
            tenant: draft.identity.tenant_id.record_id(),
            release_key: draft.release_key.clone(),
            dataset_key: draft.dataset_key,
            source_key: draft.source_key,
            state: draft.state,
            version_label: draft.version_label,
            source_digest_sha256: draft.source_digest_sha256,
            valid_from: draft.valid_from,
            valid_until: draft.valid_until,
            canonical_json: draft.canonical_json,
            record_version: 1,
            created_at: now,
            updated_at: now,
        };
        create_only(
            self,
            map_record("map_dataset_release", &draft.release_key),
            content,
        )
        .await?;
        self.map_release(draft.identity.tenant_id, &draft.release_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map release creation readback",
            })
    }

    pub async fn map_release(
        &self,
        tenant_id: TenantId,
        release_key: &str,
    ) -> Result<Option<MapDatasetReleaseRecord>, StoreError> {
        validate_public_key("release_key", release_key, "release-")?;
        select_one(
            self,
            map_record("map_dataset_release", release_key),
            tenant_id,
        )
        .await
    }

    pub async fn list_map_releases(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<MapDatasetReleaseRecord>, StoreError> {
        select_tenant_list(
            self,
            "SELECT * FROM map_dataset_release WHERE tenant = $tenant ORDER BY created_at DESC;",
            tenant_id,
        )
        .await
    }

    pub async fn transition_map_release(
        &self,
        tenant_id: TenantId,
        release_key: &str,
        expected_record_version: i64,
        state: MapReleaseState,
        canonical_json: String,
    ) -> Result<MapDatasetReleaseRecord, StoreError> {
        validate_public_key("release_key", release_key, "release-")?;
        validate_json("canonical_json", &canonical_json, MAX_CATALOG_JSON_BYTES)?;
        let mut response = self.client().query("UPDATE $record MERGE { state: $state, canonical_json: $canonical_json, record_version: $next_version, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
            .bind(("record", map_record("map_dataset_release", release_key)))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("state", state))
            .bind(("canonical_json", canonical_json))
            .bind(("expected", expected_record_version))
            .bind(("next_version", expected_record_version + 1))
            .await?.check()?;
        response
            .take::<Option<MapDatasetReleaseRecord>>(0)?
            .ok_or_else(|| StoreError::MapRecordConflict {
                entity: "release",
                key: release_key.to_owned(),
            })
    }

    pub async fn activate_map_release(
        &self,
        identity: &PlatformIdentity,
        dataset_key: &str,
        release_key: &str,
        expected_pointer_version: Option<i64>,
        expected_release_version: i64,
        canonical_json: String,
    ) -> Result<MapDatasetReleaseRecord, StoreError> {
        validate_public_key("dataset_key", dataset_key, "dataset-")?;
        validate_public_key("release_key", release_key, "release-")?;
        validate_positive("expected_release_version", expected_release_version)?;
        validate_json("canonical_json", &canonical_json, MAX_CATALOG_JSON_BYTES)?;
        let active_id = format!("{}:{}", identity.tenant_id, dataset_key);
        let active_record = map_record("map_active_release", &active_id);
        let release_record = map_record("map_dataset_release", release_key);
        let release = self
            .map_release(identity.tenant_id, release_key)
            .await?
            .ok_or_else(|| StoreError::MapRecordConflict {
                entity: "release",
                key: release_key.to_owned(),
            })?;
        if release.dataset_key != dataset_key || release.record_version != expected_release_version
        {
            return Err(StoreError::MapRecordConflict {
                entity: "release",
                key: release_key.to_owned(),
            });
        }
        let existing = self
            .active_map_release(identity.tenant_id, dataset_key)
            .await?;
        let expected = expected_pointer_version.unwrap_or(0);
        if existing.as_ref().map_or(0, |record| record.record_version) != expected {
            return Err(StoreError::MapRecordConflict {
                entity: "active release",
                key: dataset_key.to_owned(),
            });
        }
        let previous = existing.map(|record| record.release_key);
        let next = expected + 1;
        let pointer_statement = if expected == 0 {
            "CREATE ONLY $active CONTENT { tenant: $tenant, dataset_key: $dataset_key, release_key: $release_key, previous_release_key: $previous, activated_by: $owner, activated_at: time::now(), record_version: $next } RETURN NONE;"
        } else {
            "LET $pointer_updated = (UPDATE ONLY $active MERGE { release_key: $release_key, previous_release_key: $previous, activated_by: $owner, activated_at: time::now(), record_version: $next } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER); IF $pointer_updated = NONE { THROW 'map_active_release_conflict'; };"
        };
        let query = format!(
            "BEGIN TRANSACTION; LET $release_updated = (UPDATE ONLY $release MERGE {{ state: 'active', canonical_json: $canonical_json, record_version: $next_release_version, updated_at: time::now() }} WHERE tenant = $tenant AND dataset_key = $dataset_key AND record_version = $expected_release_version RETURN AFTER); IF $release_updated = NONE {{ THROW 'map_dataset_release_conflict'; }}; {pointer_statement} COMMIT TRANSACTION;"
        );
        let _response = self
            .client()
            .query(query)
            .bind(("active", active_record))
            .bind(("release", release_record))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("dataset_key", dataset_key.to_owned()))
            .bind(("release_key", release_key.to_owned()))
            .bind(("previous", previous))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("expected", expected))
            .bind(("next", next))
            .bind(("expected_release_version", expected_release_version))
            .bind(("next_release_version", expected_release_version + 1))
            .bind(("canonical_json", canonical_json))
            .await?
            .check()
            .map_err(|error| {
                if error.to_string().contains("map_active_release_conflict") {
                    StoreError::MapRecordConflict {
                        entity: "active release",
                        key: dataset_key.to_owned(),
                    }
                } else if error.to_string().contains("map_dataset_release_conflict") {
                    StoreError::MapRecordConflict {
                        entity: "release",
                        key: release_key.to_owned(),
                    }
                } else if error.to_string().contains("failed transaction") {
                    StoreError::MapRecordConflict {
                        entity: "active release",
                        key: dataset_key.to_owned(),
                    }
                } else {
                    StoreError::Database(error)
                }
            })?;
        self.map_release(identity.tenant_id, release_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map release activation readback",
            })
    }

    pub async fn active_map_release(
        &self,
        tenant_id: TenantId,
        dataset_key: &str,
    ) -> Result<Option<MapActiveReleaseRecord>, StoreError> {
        validate_public_key("dataset_key", dataset_key, "dataset-")?;
        let id = format!("{}:{}", tenant_id, dataset_key);
        select_one(self, map_record("map_active_release", &id), tenant_id).await
    }

    pub async fn list_active_map_releases(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<MapActiveReleaseRecord>, StoreError> {
        select_tenant_list(
            self,
            "SELECT * FROM map_active_release WHERE tenant = $tenant ORDER BY dataset_key ASC;",
            tenant_id,
        )
        .await
    }

    pub async fn create_map_mobility_profile(
        &self,
        draft: MapMobilityProfileDraft,
    ) -> Result<MapMobilityProfileRecord, StoreError> {
        validate_public_key("profile_key", &draft.profile_key, "mobility-")?;
        validate_text("family", &draft.family, 64)?;
        validate_text("name", &draft.name, 256)?;
        validate_positive("profile_version", draft.profile_version)?;
        validate_validity(draft.valid_from, draft.valid_until)?;
        validate_json(
            "canonical_json",
            &draft.canonical_json,
            MAX_CATALOG_JSON_BYTES,
        )?;
        let now = Utc::now();
        let record_key = format!("{}:{}", draft.profile_key, draft.profile_version);
        let content = MapMobilityProfileContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            profile_key: draft.profile_key.clone(),
            family: draft.family,
            name: draft.name,
            profile_version: draft.profile_version,
            valid_from: draft.valid_from,
            valid_until: draft.valid_until,
            canonical_json: draft.canonical_json,
            created_at: now,
            updated_at: now,
        };
        create_only(
            self,
            map_record("map_mobility_profile", &record_key),
            content,
        )
        .await?;
        self.map_mobility_profile(
            draft.identity.tenant_id,
            &draft.profile_key,
            draft.profile_version,
        )
        .await?
        .ok_or(StoreError::MissingRecord {
            operation: "map mobility profile creation readback",
        })
    }

    pub async fn map_mobility_profile(
        &self,
        tenant_id: TenantId,
        profile_key: &str,
        version: i64,
    ) -> Result<Option<MapMobilityProfileRecord>, StoreError> {
        validate_public_key("profile_key", profile_key, "mobility-")?;
        validate_positive("profile_version", version)?;
        let key = format!("{profile_key}:{version}");
        select_one(self, map_record("map_mobility_profile", &key), tenant_id).await
    }

    pub async fn list_map_mobility_profiles(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<MapMobilityProfileRecord>, StoreError> {
        select_tenant_list(self, "SELECT * FROM map_mobility_profile WHERE tenant = $tenant ORDER BY name ASC, profile_version DESC;", tenant_id).await
    }

    pub async fn create_map_restriction(
        &self,
        mut draft: MapRestrictionDraft,
    ) -> Result<MapRestrictionRecord, StoreError> {
        validate_public_key("restriction_key", &draft.restriction_key, "restriction-")?;
        validate_text("kind", &draft.kind, 128)?;
        validate_text("effect_kind", &draft.effect_kind, 64)?;
        normalize_values(
            "affected_mobility_families",
            &mut draft.affected_mobility_families,
            64,
        )?;
        if draft.affected_mobility_families.is_empty() {
            return Err(invalid_map(
                "affected_mobility_families",
                "must not be empty",
            ));
        }
        validate_validity(draft.valid_from, draft.valid_until)?;
        validate_json(
            "canonical_json",
            &draft.canonical_json,
            MAX_CATALOG_JSON_BYTES,
        )?;
        if let Some(cancelled_by) = &draft.cancelled_by {
            validate_public_key("cancelled_by", cancelled_by, "restriction-")?;
        }
        let now = Utc::now();
        let content = MapRestrictionContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            restriction_key: draft.restriction_key.clone(),
            kind: draft.kind,
            effect_kind: draft.effect_kind,
            affected_mobility_families: draft.affected_mobility_families,
            valid_from: draft.valid_from,
            valid_until: draft.valid_until,
            cancelled_by: draft.cancelled_by,
            canonical_json: draft.canonical_json,
            record_version: 1,
            created_at: now,
            updated_at: now,
        };
        create_only(
            self,
            map_record("map_restriction", &draft.restriction_key),
            content,
        )
        .await?;
        self.map_restriction(draft.identity.tenant_id, &draft.restriction_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map restriction creation readback",
            })
    }

    pub async fn map_restriction(
        &self,
        tenant_id: TenantId,
        restriction_key: &str,
    ) -> Result<Option<MapRestrictionRecord>, StoreError> {
        validate_public_key("restriction_key", restriction_key, "restriction-")?;
        select_one(
            self,
            map_record("map_restriction", restriction_key),
            tenant_id,
        )
        .await
    }

    pub async fn list_map_restrictions(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<MapRestrictionRecord>, StoreError> {
        select_tenant_list(
            self,
            "SELECT * FROM map_restriction WHERE tenant = $tenant ORDER BY valid_from DESC;",
            tenant_id,
        )
        .await
    }

    pub async fn replace_map_restriction(
        &self,
        tenant_id: TenantId,
        restriction_key: &str,
        expected_record_version: i64,
        valid_until: Option<DateTime<Utc>>,
        cancelled_by: Option<String>,
        canonical_json: String,
    ) -> Result<MapRestrictionRecord, StoreError> {
        validate_public_key("restriction_key", restriction_key, "restriction-")?;
        validate_json("canonical_json", &canonical_json, MAX_CATALOG_JSON_BYTES)?;
        if let Some(cancelled_by) = &cancelled_by {
            validate_public_key("cancelled_by", cancelled_by, "restriction-")?;
        }
        let mut response = self.client().query("UPDATE $record MERGE { valid_until: $valid_until, cancelled_by: $cancelled_by, canonical_json: $canonical_json, record_version: $next_version, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
            .bind(("record", map_record("map_restriction", restriction_key)))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("valid_until", valid_until))
            .bind(("cancelled_by", cancelled_by))
            .bind(("canonical_json", canonical_json))
            .bind(("expected", expected_record_version))
            .bind(("next_version", expected_record_version + 1))
            .await?.check()?;
        response
            .take::<Option<MapRestrictionRecord>>(0)?
            .ok_or_else(|| StoreError::MapRecordConflict {
                entity: "restriction",
                key: restriction_key.to_owned(),
            })
    }

    pub async fn create_map_operational_snapshot(
        &self,
        draft: MapOperationalSnapshotDraft,
    ) -> Result<MapOperationalSnapshotRecord, StoreError> {
        validate_public_key("snapshot_key", &draft.snapshot_key, "snapshot-")?;
        validate_json(
            "canonical_json",
            &draft.canonical_json,
            MAX_CATALOG_JSON_BYTES,
        )?;
        let content = MapOperationalSnapshotContent {
            tenant: draft.tenant_id.record_id(),
            snapshot_key: draft.snapshot_key.clone(),
            departure_time: draft.departure_time,
            canonical_json: draft.canonical_json,
            created_at: Utc::now(),
        };
        create_only(
            self,
            map_record("map_operational_snapshot", &draft.snapshot_key),
            content,
        )
        .await?;
        select_one(
            self,
            map_record("map_operational_snapshot", &draft.snapshot_key),
            draft.tenant_id,
        )
        .await?
        .ok_or(StoreError::MissingRecord {
            operation: "map operational snapshot creation readback",
        })
    }

    pub async fn create_map_route(
        &self,
        draft: MapRouteDraft,
    ) -> Result<MapRouteRecord, StoreError> {
        validate_public_key("route_key", &draft.route_key, "route-")?;
        validate_public_key(
            "mobility_profile_key",
            &draft.mobility_profile_key,
            "mobility-",
        )?;
        validate_positive("mobility_profile_version", draft.mobility_profile_version)?;
        validate_public_key(
            "operational_snapshot_key",
            &draft.operational_snapshot_key,
            "snapshot-",
        )?;
        validate_sha256("cache_digest_sha256", &draft.cache_digest_sha256)?;
        validate_json(
            "canonical_json",
            &draft.canonical_json,
            MAX_ROUTE_JSON_BYTES,
        )?;
        let now = Utc::now();
        let content = MapRouteContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            route_key: draft.route_key.clone(),
            status: draft.status,
            mobility_profile_key: draft.mobility_profile_key,
            mobility_profile_version: draft.mobility_profile_version,
            operational_snapshot_key: draft.operational_snapshot_key,
            departure_time: draft.departure_time,
            arrival_time: draft.arrival_time,
            cache_digest_sha256: draft.cache_digest_sha256,
            canonical_json: draft.canonical_json,
            created_at: now,
            updated_at: now,
        };
        create_only(self, map_record("map_route", &draft.route_key), content).await?;
        self.map_route(draft.identity.tenant_id, &draft.route_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map route creation readback",
            })
    }

    pub async fn map_route(
        &self,
        tenant_id: TenantId,
        route_key: &str,
    ) -> Result<Option<MapRouteRecord>, StoreError> {
        validate_public_key("route_key", route_key, "route-")?;
        select_one(self, map_record("map_route", route_key), tenant_id).await
    }

    pub async fn list_map_routes(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<MapRouteRecord>, StoreError> {
        select_tenant_list(
            self,
            "SELECT * FROM map_route WHERE tenant = $tenant ORDER BY created_at DESC;",
            tenant_id,
        )
        .await
    }

    pub async fn create_map_route_dependency(
        &self,
        draft: MapRouteDependencyDraft,
    ) -> Result<MapRouteDependencyRecord, StoreError> {
        validate_public_key("route_key", &draft.route_key, "route-")?;
        validate_text("dependency_key", &draft.dependency_key, 256)?;
        let key = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!(
                "{}:{:?}:{}:{}",
                draft.tenant_id, draft.dependency_kind, draft.route_key, draft.dependency_key
            )
            .as_bytes(),
        )
        .to_string();
        let content = MapRouteDependencyContent {
            tenant: draft.tenant_id.record_id(),
            route_key: draft.route_key,
            dependency_kind: draft.dependency_kind,
            dependency_key: draft.dependency_key,
            created_at: Utc::now(),
        };
        create_only(self, map_record("map_route_dependency", &key), content).await?;
        select_one(
            self,
            map_record("map_route_dependency", &key),
            draft.tenant_id,
        )
        .await?
        .ok_or(StoreError::MissingRecord {
            operation: "map route dependency creation readback",
        })
    }

    pub async fn invalidate_map_routes_by_dependency(
        &self,
        tenant_id: TenantId,
        dependency_kind: crate::MapDependencyKind,
        dependency_key: &str,
    ) -> Result<u64, StoreError> {
        validate_text("dependency_key", dependency_key, 256)?;
        let mut response = self.client().query("LET $routes = SELECT VALUE route_key FROM map_route_dependency WHERE tenant = $tenant AND dependency_kind = $kind AND dependency_key = $key; UPDATE map_route SET status = 'invalidated', updated_at = time::now() WHERE tenant = $tenant AND route_key IN $routes AND status != 'invalidated' RETURN AFTER;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("kind", dependency_kind))
            .bind(("key", dependency_key.to_owned()))
            .await?.check()?;
        let records: Vec<MapRouteRecord> = response.take(1)?;
        Ok(records.len() as u64)
    }

    pub async fn create_map_route_matrix(
        &self,
        draft: MapRouteMatrixDraft,
    ) -> Result<MapRouteMatrixRecord, StoreError> {
        validate_public_key("matrix_key", &draft.matrix_key, "matrix-")?;
        validate_public_key(
            "mobility_profile_key",
            &draft.mobility_profile_key,
            "mobility-",
        )?;
        validate_positive("mobility_profile_version", draft.mobility_profile_version)?;
        validate_public_key(
            "operational_snapshot_key",
            &draft.operational_snapshot_key,
            "snapshot-",
        )?;
        if let Some(value) = &draft.canonical_json {
            validate_json("canonical_json", value, MAX_ROUTE_JSON_BYTES)?;
        }
        if let Some(value) = &draft.artifact_uri {
            validate_text("artifact_uri", value, 2_048)?;
        }
        let content = MapRouteMatrixContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            matrix_key: draft.matrix_key.clone(),
            mobility_profile_key: draft.mobility_profile_key,
            mobility_profile_version: draft.mobility_profile_version,
            operational_snapshot_key: draft.operational_snapshot_key,
            artifact_uri: draft.artifact_uri,
            canonical_json: draft.canonical_json,
            created_at: Utc::now(),
        };
        create_only(
            self,
            map_record("map_route_matrix", &draft.matrix_key),
            content,
        )
        .await?;
        self.map_route_matrix(draft.identity.tenant_id, &draft.matrix_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map route matrix creation readback",
            })
    }

    pub async fn map_route_matrix(
        &self,
        tenant_id: TenantId,
        matrix_key: &str,
    ) -> Result<Option<MapRouteMatrixRecord>, StoreError> {
        validate_public_key("matrix_key", matrix_key, "matrix-")?;
        select_one(self, map_record("map_route_matrix", matrix_key), tenant_id).await
    }

    pub async fn list_map_route_matrices(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<MapRouteMatrixRecord>, StoreError> {
        select_tenant_list(
            self,
            "SELECT * FROM map_route_matrix WHERE tenant = $tenant ORDER BY created_at DESC;",
            tenant_id,
        )
        .await
    }

    pub async fn set_map_route_state(
        &self,
        tenant_id: TenantId,
        route_key: &str,
        state: MapRouteState,
        canonical_json: String,
    ) -> Result<MapRouteRecord, StoreError> {
        validate_public_key("route_key", route_key, "route-")?;
        validate_json("canonical_json", &canonical_json, MAX_ROUTE_JSON_BYTES)?;
        let mut response = self.client().query("UPDATE $record MERGE { status: $state, canonical_json: $canonical_json, updated_at: time::now() } WHERE tenant = $tenant RETURN AFTER;")
            .bind(("record", map_record("map_route", route_key))).bind(("tenant", tenant_id.record_id())).bind(("state", state)).bind(("canonical_json", canonical_json)).await?.check()?;
        response
            .take::<Option<MapRouteRecord>>(0)?
            .ok_or_else(|| StoreError::MapRecordConflict {
                entity: "route",
                key: route_key.to_owned(),
            })
    }

    pub async fn create_map_acquisition(
        &self,
        draft: MapAcquisitionDraft,
    ) -> Result<MapAcquisitionRecord, StoreError> {
        validate_acquisition_draft(&draft)?;
        if let Some(existing) = self
            .map_acquisition_for_idempotency(
                draft.identity.tenant_id,
                draft.identity.principal_id.record_id(),
                &draft.idempotency_key,
            )
            .await?
        {
            if existing.source_key == draft.source_key
                && existing.canonical_json == draft.canonical_json
            {
                return Ok(existing);
            }
            return Err(StoreError::MapRecordConflict {
                entity: "acquisition idempotency key",
                key: draft.idempotency_key,
            });
        }
        let now = Utc::now();
        let content = MapAcquisitionContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            acquisition_key: draft.acquisition_key.clone(),
            source_key: draft.source_key,
            idempotency_key: draft.idempotency_key,
            status: draft.status,
            phase: draft.phase,
            staged_release_key: draft.staged_release_key,
            canonical_json: draft.canonical_json,
            record_version: 1,
            created_at: now,
            updated_at: now,
        };
        create_only(
            self,
            map_record("map_acquisition", &draft.acquisition_key),
            content,
        )
        .await?;
        self.map_acquisition(draft.identity.tenant_id, &draft.acquisition_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map acquisition creation readback",
            })
    }

    pub async fn map_acquisition(
        &self,
        tenant_id: TenantId,
        acquisition_key: &str,
    ) -> Result<Option<MapAcquisitionRecord>, StoreError> {
        validate_public_key("acquisition_key", acquisition_key, "acquisition-")?;
        select_one(
            self,
            map_record("map_acquisition", acquisition_key),
            tenant_id,
        )
        .await
    }

    pub async fn list_map_acquisitions(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<MapAcquisitionRecord>, StoreError> {
        select_tenant_list(
            self,
            "SELECT * FROM map_acquisition WHERE tenant = $tenant ORDER BY created_at DESC;",
            tenant_id,
        )
        .await
    }

    pub async fn update_map_acquisition(
        &self,
        tenant_id: TenantId,
        acquisition_key: &str,
        expected_record_version: i64,
        status: MapAcquisitionState,
        phase: &str,
        staged_release_key: Option<String>,
        canonical_json: String,
    ) -> Result<MapAcquisitionRecord, StoreError> {
        validate_public_key("acquisition_key", acquisition_key, "acquisition-")?;
        validate_text("phase", phase, 128)?;
        validate_json("canonical_json", &canonical_json, MAX_CATALOG_JSON_BYTES)?;
        if let Some(key) = &staged_release_key {
            validate_public_key("staged_release_key", key, "release-")?;
        }
        let mut response = self.client().query("UPDATE $record MERGE { status: $status, phase: $phase, staged_release_key: $staged_release_key, canonical_json: $canonical_json, record_version: $next_version, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
            .bind(("record", map_record("map_acquisition", acquisition_key))).bind(("tenant", tenant_id.record_id())).bind(("status", status)).bind(("phase", phase.to_owned())).bind(("staged_release_key", staged_release_key)).bind(("canonical_json", canonical_json)).bind(("expected", expected_record_version)).bind(("next_version", expected_record_version + 1)).await?.check()?;
        response
            .take::<Option<MapAcquisitionRecord>>(0)?
            .ok_or_else(|| StoreError::MapRecordConflict {
                entity: "acquisition",
                key: acquisition_key.to_owned(),
            })
    }

    pub async fn map_acquisition_for_idempotency(
        &self,
        tenant_id: TenantId,
        owner: RecordId,
        key: &str,
    ) -> Result<Option<MapAcquisitionRecord>, StoreError> {
        validate_text("idempotency_key", key, 256)?;
        let mut response = self.client().query("SELECT * FROM map_acquisition WHERE tenant = $tenant AND owner = $owner AND idempotency_key = $key LIMIT 1;")
            .bind(("tenant", tenant_id.record_id())).bind(("owner", owner)).bind(("key", key.to_owned())).await?.check()?;
        let records: Vec<MapAcquisitionRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }
}

async fn create_only<T: SurrealValue>(
    store: &PlatformStore,
    record: RecordId,
    content: T,
) -> Result<(), StoreError> {
    store
        .client()
        .query("CREATE ONLY $record CONTENT $content RETURN NONE;")
        .bind(("record", record))
        .bind(("content", content))
        .await?
        .check()?;
    Ok(())
}

async fn select_one<T>(
    store: &PlatformStore,
    record: RecordId,
    tenant_id: TenantId,
) -> Result<Option<T>, StoreError>
where
    T: for<'de> Deserialize<'de> + SurrealValue,
{
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $record WHERE tenant = $tenant;")
        .bind(("record", record))
        .bind(("tenant", tenant_id.record_id()))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

async fn select_tenant_list<T>(
    store: &PlatformStore,
    query: &'static str,
    tenant_id: TenantId,
) -> Result<Vec<T>, StoreError>
where
    T: for<'de> Deserialize<'de> + SurrealValue,
{
    let mut response = store
        .client()
        .query(query)
        .bind(("tenant", tenant_id.record_id()))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

fn validate_source_draft(draft: &mut MapSourceDraft) -> Result<(), StoreError> {
    validate_public_key("source_key", &draft.source_key, "source-")?;
    validate_public_key("dataset_key", &draft.dataset_key, "dataset-")?;
    validate_text("name", &draft.name, 256)?;
    validate_text("adapter_kind", &draft.adapter_kind, 128)?;
    validate_text("authority_class", &draft.authority_class, 128)?;
    normalize_values("map_families", &mut draft.map_families, 64)?;
    if draft.map_families.is_empty() {
        return Err(invalid_map("map_families", "must not be empty"));
    }
    validate_json(
        "canonical_json",
        &draft.canonical_json,
        MAX_CATALOG_JSON_BYTES,
    )
}

fn validate_release_draft(draft: &MapReleaseDraft) -> Result<(), StoreError> {
    validate_public_key("release_key", &draft.release_key, "release-")?;
    validate_public_key("dataset_key", &draft.dataset_key, "dataset-")?;
    validate_public_key("source_key", &draft.source_key, "source-")?;
    validate_text("version_label", &draft.version_label, 256)?;
    validate_sha256("source_digest_sha256", &draft.source_digest_sha256)?;
    validate_validity(draft.valid_from, draft.valid_until)?;
    validate_json(
        "canonical_json",
        &draft.canonical_json,
        MAX_CATALOG_JSON_BYTES,
    )
}

fn validate_acquisition_draft(draft: &MapAcquisitionDraft) -> Result<(), StoreError> {
    validate_public_key("acquisition_key", &draft.acquisition_key, "acquisition-")?;
    validate_public_key("source_key", &draft.source_key, "source-")?;
    validate_text("idempotency_key", &draft.idempotency_key, 256)?;
    validate_text("phase", &draft.phase, 128)?;
    if let Some(key) = &draft.staged_release_key {
        validate_public_key("staged_release_key", key, "release-")?;
    }
    validate_json(
        "canonical_json",
        &draft.canonical_json,
        MAX_CATALOG_JSON_BYTES,
    )
}

fn validate_public_key(
    field: &'static str,
    value: &str,
    prefix: &'static str,
) -> Result<(), StoreError> {
    let raw = value
        .strip_prefix(prefix)
        .ok_or_else(|| invalid_map(field, "must use the canonical prefix followed by a UUIDv7"))?;
    let uuid = Uuid::parse_str(raw)
        .map_err(|_| invalid_map(field, "must use the canonical prefix followed by a UUIDv7"))?;
    if uuid.get_version_num() != 7 {
        return Err(invalid_map(
            field,
            "must use the canonical prefix followed by a UUIDv7",
        ));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), StoreError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid_map(
            field,
            "must be non-empty, bounded, and contain no control characters",
        ));
    }
    Ok(())
}

fn validate_positive(field: &'static str, value: i64) -> Result<(), StoreError> {
    if value < 1 {
        return Err(invalid_map(field, "must be positive"));
    }
    Ok(())
}

fn validate_validity(
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
) -> Result<(), StoreError> {
    if valid_until.is_some_and(|until| until <= valid_from) {
        return Err(invalid_map("valid_until", "must be later than valid_from"));
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), StoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_map(
            field,
            "must contain 64 hexadecimal SHA-256 characters",
        ));
    }
    Ok(())
}

fn validate_json(field: &'static str, value: &str, max: usize) -> Result<(), StoreError> {
    if value.len() > max || serde_json::from_str::<serde_json::Value>(value).is_err() {
        return Err(invalid_map(field, "must be valid bounded JSON"));
    }
    Ok(())
}

fn normalize_values(
    field: &'static str,
    values: &mut Vec<String>,
    max: usize,
) -> Result<(), StoreError> {
    for value in values.iter() {
        validate_text(field, value, max)?;
    }
    values.sort();
    values.dedup();
    Ok(())
}

fn map_record(table: &'static str, key: &str) -> RecordId {
    RecordId::new(table, key.to_owned())
}

fn invalid_map(field: &'static str, reason: &'static str) -> StoreError {
    StoreError::InvalidMapField { field, reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_map_keys_require_the_expected_uuid_v7_prefix() {
        assert!(
            validate_public_key("route_key", &format!("route-{}", Uuid::now_v7()), "route-")
                .is_ok()
        );
        assert!(
            validate_public_key("route_key", &format!("route-{}", Uuid::new_v4()), "route-")
                .is_err()
        );
        assert!(
            validate_public_key(
                "route_key",
                &format!("location-{}", Uuid::now_v7()),
                "route-"
            )
            .is_err()
        );
    }

    #[test]
    fn canonical_json_is_parsed_and_bounded() {
        assert!(validate_json("canonical_json", "{}", 32).is_ok());
        assert!(validate_json("canonical_json", "not-json", 32).is_err());
        assert!(validate_json("canonical_json", "{\"too_long\":true}", 4).is_err());
    }
}
