mod activation;
mod digest;
mod lifecycle;
use super::*;
use crate::{contract::*, test_store::TestDb};
use chrono::Utc;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{PlatformStore, PlatformTable, PrincipalKind, ResourceInvalidation};
use veoveo_types::Sha256Digest;

struct AuthorityFiles {
    root: tempfile::TempDir,
    tzdb: PathBuf,
    bootstrap_leaps: PathBuf,
    first_leaps: PathBuf,
    next_leaps: PathBuf,
    bootstrap: AuthorityContext,
}

impl AuthorityFiles {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let tzdb = root.path().join("tzdb");
        tokio::fs::create_dir_all(tzdb.join("Mission"))
            .await
            .unwrap();
        // Same Linux tzdata prerequisite as the existing temporal-engine tests.
        tokio::fs::copy("/usr/share/zoneinfo/UTC", tzdb.join("UTC"))
            .await
            .unwrap();
        tokio::fs::copy("/usr/share/zoneinfo/UTC", tzdb.join("Mission/Test"))
            .await
            .unwrap();
        let bootstrap_leaps = root.path().join("bootstrap.list");
        let first_leaps = root.path().join("first.list");
        let next_leaps = root.path().join("next.list");
        for (path, offset) in [
            (&bootstrap_leaps, 10),
            (&first_leaps, 11),
            (&next_leaps, 12),
        ] {
            tokio::fs::write(path, format!("2272060800 {offset}\n"))
                .await
                .unwrap();
        }
        let bootstrap = AuthorityContext::from_paths(
            EffectiveTimeAuthority {
                tzdb: bootstrap_reference(
                    "time-release-bootstrap-tzdb",
                    AuthorityDatasetKind::Tzdb,
                ),
                leap_seconds: bootstrap_reference(
                    "time-release-bootstrap-leaps",
                    AuthorityDatasetKind::LeapSeconds,
                ),
            },
            &tzdb,
            LeapSecondTable::from_path(&bootstrap_leaps).await.unwrap(),
        )
        .unwrap();
        Self {
            root,
            tzdb,
            bootstrap_leaps,
            first_leaps,
            next_leaps,
            bootstrap,
        }
    }

    fn registry(&self) -> AuthorityRegistry {
        AuthorityRegistry::new(
            self.bootstrap.clone(),
            self.tzdb.clone(),
            self.bootstrap_leaps.clone(),
        )
    }
}

fn bootstrap_reference(id: &str, kind: AuthorityDatasetKind) -> TimeAuthorityReference {
    let release_id = AuthorityReleaseId::new(id).unwrap();
    TimeAuthorityReference {
        release_uri: TimeAuthorityReleaseUri::new(&release_id),
        release_id,
        dataset_kind: kind,
        version_label: "fixture".into(),
        source: TimeAuthoritySource::Bootstrap,
        source_digest: Sha256Digest::from_hex("a".repeat(64)).unwrap(),
    }
}

async fn scope(store: &PlatformStore, tenant: &str) -> TimeAccessContext {
    TimeAccessContext {
        identity: store
            .ensure_identity(
                tenant,
                "owner",
                "https://example.test",
                "owner",
                PrincipalKind::Service,
            )
            .await
            .unwrap(),
    }
}

async fn stage(
    catalog: &TimeCatalog,
    scope: &TimeAccessContext,
    kind: AuthorityDatasetKind,
    path: &std::path::Path,
) -> (AuthorityRelease, TimeAcquisition) {
    let source = catalog
        .create_source(
            scope,
            crate::NewTimeSource {
                source_id: TimeSourceId::new(format!("time-source-{}", Uuid::now_v7())).unwrap(),
                name: "fixture".into(),
                dataset_kind: kind,
                url: "https://example.test/data".into(),
                expected_content_type: "text/plain".into(),
                enabled: true,
                record_version: crate::SourceCreationVersion,
            },
        )
        .await
        .unwrap();
    let now = Utc::now();
    let bytes = match kind {
        AuthorityDatasetKind::Tzdb => tokio::fs::read(path.join("UTC")).await.unwrap(),
        AuthorityDatasetKind::LeapSeconds => tokio::fs::read(path).await.unwrap(),
    };
    let release = catalog
        .create_release(
            scope,
            AuthorityRelease {
                release_id: AuthorityReleaseId::new(format!("time-release-{}", Uuid::now_v7()))
                    .unwrap(),
                source_id: source.source_id.clone(),
                dataset_kind: kind,
                state: AuthorityReleaseState::Staged,
                version_label: "fixture".into(),
                source_url: source.url,
                source_digest_sha256: hex::encode(Sha256::digest(bytes)).parse().unwrap(),
                artifact_path: path.to_str().unwrap().into(),
                retrieved_at: now,
                validated_at: now,
                record_version: crate::TimeVersion::new(1).unwrap(),
            },
        )
        .await
        .unwrap();
    let acquisition = catalog
        .create_acquisition(
            scope,
            TimeAcquisition {
                acquisition_id: TimeAcquisitionId::new(format!(
                    "time-acquisition-{}",
                    Uuid::now_v7()
                ))
                .unwrap(),
                source_id: source.source_id,
                expected_source_digest_sha256: Some(release.source_digest_sha256.clone()),
                status: TimeAcquisitionStatus::Succeeded,
                phase: "staged".into(),
                staged_release_id: Some(release.release_id.clone()),
                message: "".into(),
                created_at: now,
                updated_at: now,
                record_version: crate::TimeVersion::new(1).unwrap(),
            },
            Uuid::now_v7().to_string(),
        )
        .await
        .unwrap();
    (release, acquisition)
}

async fn activate(
    catalog: &TimeCatalog,
    registry: &AuthorityRegistry,
    scope: &TimeAccessContext,
    release: &AuthorityRelease,
    expected: TimeWriteGuard,
) {
    registry
        .activate_release(
            catalog,
            scope,
            &release.release_id,
            TimeVersion::FIRST,
            expected,
        )
        .await
        .unwrap();
}

fn resolved(engine: &TemporalEngine) -> TimeInstant {
    engine
        .resolve(&ResolveTimeRequest {
            expression: TimeExpression::Rfc3339 {
                value: "2024-01-01T00:00:00Z".into(),
            },
            additional_uncertainty_nanoseconds: 0,
        })
        .unwrap()
        .instant
}

async fn set(store: &PlatformStore, record: RecordId, field: &str, value: impl SurrealValue) {
    store
        .client()
        .query("UPDATE $record MERGE $patch RETURN NONE;")
        .bind(("record", record))
        .bind((
            "patch",
            BTreeMap::from([(field.to_owned(), value.into_value())]),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn replicas_load_persisted_authority_before_serving_and_validate_cache_reuse() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let files = AuthorityFiles::new().await;
        let registry = files.registry();
        let writer = TimeCatalog::new(db.a.clone());
        let reader = TimeCatalog::new(db.b.clone());
        let owner = scope(&db.a, "registry-owner").await;
        let foreign = scope(&db.a, "registry-other").await;
        let bootstrap = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(bootstrap.authority().effective, files.bootstrap.effective);
        let (first, acquisition) = stage(
            &writer,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        activate(&writer, &registry, &owner, &first, TimeWriteGuard::Absent).await;
        // No notification or manual reload: request-time selection is authoritative.
        let first_engine = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(
            first_engine.authority().binding.leap_seconds_release_id,
            first.release_id
        );
        assert_eq!(
            resolved(&first_engine).tai_seconds_since_1970,
            resolved(&bootstrap).tai_seconds_since_1970 + 1
        );
        let reused = registry.authority_engine(&reader, &owner).await.unwrap();
        assert!(Arc::ptr_eq(
            &first_engine.authority().leap_seconds,
            &reused.authority().leap_seconds
        ));
        assert_eq!(
            registry
                .authority_engine(&reader, &foreign)
                .await
                .unwrap()
                .authority()
                .effective,
            files.bootstrap.effective
        );

        let (tzdb, _) = stage(&writer, &owner, AuthorityDatasetKind::Tzdb, &files.tzdb).await;
        activate(&writer, &registry, &owner, &tzdb, TimeWriteGuard::Absent).await;
        let restarted = files.registry();
        let engine = restarted.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(engine.authority().binding.tzdb_release_id, tzdb.release_id);
        assert_eq!(
            engine.authority().binding.leap_seconds_release_id,
            first.release_id
        );
        assert!(
            engine
                .authority()
                .tzdb
                .available()
                .any(|zone| zone.to_string() == "Mission/Test")
        );
        let epoch_id = MissionEpochId::new("epoch-private").unwrap();
        engine.replace_epochs([MissionEpoch {
            epoch_id: epoch_id.clone(),
            name: "request local".into(),
            version: crate::TimeVersion::new(1).unwrap(),
            instant: resolved(&engine),
        }]);
        let isolated = restarted.authority_engine(&reader, &owner).await.unwrap();
        assert!(
            isolated
                .resolve(&ResolveTimeRequest {
                    expression: TimeExpression::EpochRelative {
                        epoch_id,
                        offset_nanoseconds: 0
                    },
                    additional_uncertainty_nanoseconds: 0
                })
                .is_err()
        );

        // A warm context cannot hide missing or malformed producing provenance.
        let acquisition_id =
            RecordId::new("time_acquisition", acquisition.acquisition_id.to_string());
        set(
            &db.a,
            acquisition_id.clone(),
            "staged_release_key",
            None::<String>,
        )
        .await;
        assert!(registry.authority_engine(&reader, &owner).await.is_err());
        set(
            &db.a,
            acquisition_id,
            "staged_release_key",
            Some(first.release_id.to_string()),
        )
        .await;
        assert!(registry.authority_engine(&reader, &owner).await.is_ok());
        let release_id = RecordId::new("time_authority_release", first.release_id.to_string());
        let body =
            db.a.client()
                .query("SELECT VALUE canonical_json FROM ONLY $record;")
                .bind(("record", release_id.clone()))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take::<Option<String>>(0)
                .unwrap()
                .unwrap();
        set(&db.a, release_id.clone(), "canonical_json", "{}").await;
        assert!(registry.authority_engine(&reader, &owner).await.is_err());
        assert_eq!(
            registry
                .bootstrap_reference(&files.bootstrap.binding.tzdb_release_id)
                .unwrap(),
            files.bootstrap.effective.tzdb
        );
        assert!(registry.bootstrap_reference(&first.release_id).is_none());
        set(&db.a, release_id.clone(), "canonical_json", body).await;
        assert!(registry.authority_engine(&reader, &owner).await.is_ok());

        // Explicit reload failure evicts the old context; repair can recover it.
        tokio::fs::remove_file(&files.first_leaps).await.unwrap();
        assert!(registry.reload(&reader, &owner).await.is_err());
        assert!(registry.authority_engine(&reader, &owner).await.is_err());
        tokio::fs::write(&files.first_leaps, "2272060800 11\n")
            .await
            .unwrap();
        assert!(registry.authority_engine(&reader, &owner).await.is_ok());
        db.a.client()
            .query("DELETE $record RETURN NONE;")
            .bind(("record", release_id))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(registry.authority_engine(&reader, &owner).await.is_err());
        assert_eq!(
            registry
                .authority_engine(&reader, &foreign)
                .await
                .unwrap()
                .authority()
                .effective,
            files.bootstrap.effective
        );
    })
    .await
    .expect("authority registry qualification exceeded 90 seconds");
}

#[tokio::test]
async fn live_and_reconciliation_invalidate_contexts_without_becoming_a_freshness_dependency() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let files = AuthorityFiles::new().await;
        let registry = files.registry();
        let writer = TimeCatalog::new(db.a.clone());
        let reader = TimeCatalog::new(db.b.clone());
        let owner = scope(&db.a, "registry-observation").await;
        let tables = vec![
            PlatformTable::TimeActiveAuthority,
            PlatformTable::TimeAuthorityRelease,
            PlatformTable::TimeAcquisition,
        ];
        let mut changes = db.b.resource_changes(tables.clone());
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), changes.next())
                .await
                .unwrap(),
            Some(ResourceInvalidation::Reconcile)
        );
        registry.invalidate().await;
        let bootstrap = registry.authority_engine(&reader, &owner).await.unwrap();
        let (first, _) = stage(
            &writer,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        activate(&writer, &registry, &owner, &first, TimeWriteGuard::Absent).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), changes.next())
                .await
                .unwrap(),
            Some(ResourceInvalidation::Live)
        );
        registry.invalidate().await;
        let first_engine = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_ne!(
            first_engine.authority().binding,
            bootstrap.authority().binding
        );
        drop(changes);
        let (next, _) = stage(
            &writer,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.next_leaps,
        )
        .await;
        activate(
            &writer,
            &registry,
            &owner,
            &next,
            TimeWriteGuard::Existing(TimeVersion::FIRST),
        )
        .await;
        let next_engine = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(
            next_engine.authority().binding.leap_seconds_release_id,
            next.release_id
        );
        assert_eq!(
            resolved(&next_engine).tai_seconds_since_1970,
            resolved(&first_engine).tai_seconds_since_1970 + 1
        );
        let mut changes = db.b.resource_changes(tables);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), changes.next())
                .await
                .unwrap(),
            Some(ResourceInvalidation::Reconcile)
        );
        registry.invalidate().await;
        let reloaded = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(
            reloaded.authority().effective,
            next_engine.authority().effective
        );
        assert!(!Arc::ptr_eq(
            &reloaded.authority().leap_seconds,
            &next_engine.authority().leap_seconds
        ));
    })
    .await
    .expect("authority observation qualification exceeded 90 seconds");
}

#[tokio::test]
async fn event_batches_reuse_authority_and_skip_registered_or_terminal_events() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = TestDb::new().await;
        let files = AuthorityFiles::new().await;
        let registry = files.registry();
        let catalog = TimeCatalog::new(db.b.clone());
        let owner = scope(&db.a, "registry-events").await;
        let (release, _) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        activate(
            &catalog,
            &registry,
            &owner,
            &release,
            TimeWriteGuard::Absent,
        )
        .await;
        let engine = registry.authority_engine(&catalog, &owner).await.unwrap();
        let due = engine
            .resolve(&ResolveTimeRequest {
                expression: TimeExpression::Rfc3339 {
                    value: (Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
                },
                additional_uncertainty_nanoseconds: 0,
            })
            .unwrap()
            .instant;
        let acquisitions = crate::acquisition::AcquisitionService::new(
            crate::acquisition::AcquisitionServiceConfig {
                scratch_root: files.root.path().join("scratch"),
                release_root: files.root.path().join("releases"),
                zic_executable: "/usr/sbin/zic".into(),
                maximum_source_bytes: 1024,
                maximum_expanded_bytes: 4096,
                timeout: Duration::from_secs(5),
            },
            catalog.clone(),
        )
        .unwrap();
        let state = Arc::new(crate::state::TimeApplication {
            tasks: veoveo_task_runtime::TaskRuntime::new(db.b.clone(), "time", "fixture"),
            catalog,
            authorities: registry,
            clock: crate::clock::ClockMonitor::new(
                crate::clock::ClockSource::System,
                Duration::from_secs(1),
            ),
            acquisitions: Arc::new(acquisitions),
            subscriptions: Arc::new(veoveo_mcp_contract::SubscriptionHub::new()),
            event_watchers: Arc::default(),
        });
        let mut events = Vec::new();
        for index in 0..4 {
            events.push(
                state
                    .catalog
                    .create_event(
                        &owner,
                        TemporalEvent {
                            event_id: TemporalEventId::new(format!("event-{}", Uuid::now_v7()))
                                .unwrap(),
                            name: format!("event {index}"),
                            due: due.clone(),
                            state: TemporalEventState::Scheduled,
                            record_version: crate::TimeVersion::new(1).unwrap(),
                        },
                        format!("fixture-{index}"),
                    )
                    .await
                    .unwrap(),
            );
        }
        state
            .schedule_events(owner.clone(), events[..3].iter().cloned())
            .await
            .unwrap();
        assert_eq!(state.event_watchers.lock().await.len(), 3);
        let record = RecordId::new("time_authority_release", release.release_id.to_string());
        let body =
            db.a.client()
                .query("SELECT VALUE canonical_json FROM ONLY $record;")
                .bind(("record", record.clone()))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take::<Option<String>>(0)
                .unwrap()
                .unwrap();
        set(&db.a, record.clone(), "canonical_json", "{}").await;
        // Existing watchers need no new clock selection, even if the catalog is
        // unavailable for a new watcher. The fourth event must remain unregistered.
        state
            .schedule_events(owner.clone(), events[..3].iter().cloned())
            .await
            .unwrap();
        assert!(
            state
                .schedule_events(owner.clone(), events.clone())
                .await
                .is_err()
        );
        assert_eq!(state.event_watchers.lock().await.len(), 3);
        let cancelled = state
            .catalog
            .cancel_event(&owner, &events[3].event_id, TimeVersion::FIRST)
            .await
            .unwrap();
        state
            .schedule_event(owner.clone(), cancelled)
            .await
            .unwrap();
        assert_eq!(state.event_watchers.lock().await.len(), 3);
        set(&db.a, record, "canonical_json", body).await;
        for event in &events[..3] {
            state.cancel_event_watcher(&owner, &event.event_id).await;
        }
        assert!(state.event_watchers.lock().await.is_empty());
    })
    .await
    .expect("event authority batch qualification exceeded 90 seconds");
}
