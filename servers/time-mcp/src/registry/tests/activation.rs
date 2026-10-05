use super::*;
use std::time::Duration;

async fn prepare(
    registry: &AuthorityRegistry,
    catalog: &TimeCatalog,
    scope: &TimeAccessContext,
    release: &AuthorityRelease,
    pointer: TimeWriteGuard,
) -> ActivationDraft {
    registry
        .preflight_activation(
            catalog,
            scope,
            &release.release_id,
            TimeVersion::FIRST,
            pointer,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn activation_serializes_both_families_from_the_preflighted_pair() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database(crate::test_store::StoreBackend::RocksDb).await;
        let files = AuthorityFiles::new().await;
        let left = TimeCatalog::new(db.a.clone());
        let right = TimeCatalog::new(db.b.clone());
        let registry = files.registry();
        let replica = files.registry();
        let owner = scope(&db.a, "pair-activation").await;
        for generation in 0..4 {
            let (tzdb, _) = stage(&left, &owner, AuthorityDatasetKind::Tzdb, &files.tzdb).await;
            let (leaps, _) = stage(
                &right,
                &owner,
                AuthorityDatasetKind::LeapSeconds,
                &files.first_leaps,
            )
            .await;
            let guard = TimeWriteGuard::new(generation).unwrap();
            let tzdb_draft = prepare(&registry, &left, &owner, &tzdb, guard).await;
            let leap_draft = prepare(&replica, &right, &owner, &leaps, guard).await;
            let (tzdb_result, leap_result) = tokio::join!(
                left.commit_activation(&owner, tzdb_draft),
                right.commit_activation(&owner, leap_draft),
            );
            assert_ne!(
                tzdb_result.is_ok(),
                leap_result.is_ok(),
                "only one decision from the same pair may commit: {tzdb_result:?}, {leap_result:?}"
            );
            let (winner, loser) = if tzdb_result.is_ok() {
                (&tzdb, &leaps)
            } else {
                (&leaps, &tzdb)
            };
            let unchanged = left
                .release(&owner, &loser.release_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(unchanged.state, AuthorityReleaseState::Staged);
            assert_eq!(unchanged.record_version.get(), 1);
            let committed = left
                .release(&owner, &winner.release_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(committed.state, AuthorityReleaseState::Active);
            // A caller must load the changed pair again before retrying.
            activate(&right, &replica, &owner, loser, guard).await;
            let pair = left.active_releases(&owner).await.unwrap();
            assert_eq!(pair.len(), 2);
            assert!(pair.iter().any(|r| r.release_id == tzdb.release_id));
            assert!(pair.iter().any(|r| r.release_id == leaps.release_id));
        }
    })
    .await
    .expect("cross-family activation qualification exceeded 90 seconds");
}

#[tokio::test]
async fn changed_preflight_inputs_reject_publication_without_advancing_authority() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.b.clone());
        let registry = files.registry();
        let owner = scope(&db.a, "changed-preflight").await;
        let foreign = scope(&db.a, "foreign-preflight").await;
        let (leaps, _) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        activate(&catalog, &registry, &owner, &leaps, TimeWriteGuard::Absent).await;
        let (tzdb, _) = stage(&catalog, &owner, AuthorityDatasetKind::Tzdb, &files.tzdb).await;
        for target in [&leaps, &tzdb] {
            let draft = prepare(&registry, &catalog, &owner, &tzdb, TimeWriteGuard::Absent).await;
            let record = RecordId::new("time_authority_release", target.release_id.to_string());
            db.a.client()
                .query(include_str!(
                    "../../tests/queries/change_artifact_path.surql"
                ))
                .bind(("record", record.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(catalog.commit_activation(&owner, draft).await.is_err());
            db.a.client()
                .query(include_str!(
                    "../../tests/queries/restore_artifact_path.surql"
                ))
                .bind(("record", record))
                .bind(("path", target.artifact_path.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            let pair = catalog.active_releases(&owner).await.unwrap();
            assert_eq!(pair.len(), 1);
            assert_eq!(pair[0].release_id, leaps.release_id);
            let candidate = catalog
                .release(&owner, &tzdb.release_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(candidate.state, AuthorityReleaseState::Staged);
            assert_eq!(candidate.record_version.get(), 1);
        }
        let draft = prepare(&registry, &catalog, &owner, &tzdb, TimeWriteGuard::Absent).await;
        assert!(catalog.commit_activation(&foreign, draft).await.is_err());
        assert!(catalog.active_releases(&foreign).await.unwrap().is_empty());

        let draft = prepare(&registry, &catalog, &owner, &tzdb, TimeWriteGuard::Absent).await;
        db.a.client()
            .query(include_str!(
                "../../tests/queries/delete_active_authorities.surql"
            ))
            .bind(("tenant", owner.identity.tenant_id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            catalog.commit_activation(&owner, draft).await.is_err(),
            "deleted other-family pointer"
        );
    })
    .await
    .expect("changed preflight qualification exceeded 90 seconds");
}

#[tokio::test]
async fn failed_file_preflight_never_publishes_an_authority() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.b.clone());
        let registry = files.registry();
        let owner = scope(&db.a, "failed-file-preflight").await;
        let (leaps, _) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        tokio::fs::remove_file(&files.first_leaps).await.unwrap();
        assert!(
            registry
                .activate_release(
                    &catalog,
                    &owner,
                    &leaps.release_id,
                    TimeVersion::FIRST,
                    TimeWriteGuard::Absent
                )
                .await
                .is_err()
        );
        assert!(catalog.active_releases(&owner).await.unwrap().is_empty());
        assert_eq!(
            catalog
                .release(&owner, &leaps.release_id)
                .await
                .unwrap()
                .unwrap()
                .state,
            AuthorityReleaseState::Staged
        );
    })
    .await
    .expect("failed file preflight qualification exceeded 90 seconds");
}

#[tokio::test]
async fn locked_activation_inputs_detect_pointer_and_release_repairs() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database(crate::test_store::StoreBackend::RocksDb).await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.a.clone());
        let registry = files.registry();
        let owner = scope(&db.a, "locked-inputs").await;
        let mut releases = Vec::new();
        for (kind, path) in [
            (AuthorityDatasetKind::LeapSeconds, &files.first_leaps),
            (AuthorityDatasetKind::Tzdb, &files.tzdb),
        ] {
            let (release, _) = stage(&catalog, &owner, kind, path).await;
            activate(
                &catalog,
                &registry,
                &owner,
                &release,
                TimeWriteGuard::Absent,
            )
            .await;
            releases.push(RecordId::new(
                "time_authority_release",
                release.release_id.to_string(),
            ));
        }
        let pointers = ["leap_seconds", "tzdb"].map(|kind| {
            RecordId::new(
                "time_active_authority",
                format!("{}:{kind}", owner.identity.tenant_id),
            )
        });
        db.a.client()
            .query(include_str!(
                "../../tests/queries/define_activation_probe.surql"
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        for changed in pointers.iter().chain(releases.iter()) {
            let transaction = db.a.client().clone().begin().await.unwrap();
            transaction
                .query(include_str!(
                    "../../persistence/queries/activation_locks.surql"
                ))
                .bind(("pointers", pointers.clone()))
                .bind(("releases", releases.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            // A separate writer can repair either family or its source metadata.
            db.b.client()
                .query(include_str!(
                    "../../tests/queries/increment_record_version.surql"
                ))
                .bind(("changed", changed.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            transaction
                .query(include_str!(
                    "../../tests/queries/create_activation_probe.surql"
                ))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                transaction.commit().await.is_err(),
                "locked input changed: {changed:?}"
            );
            let count: Option<i64> =
                db.b.client()
                    .query(include_str!(
                        "../../tests/queries/count_activation_probes.surql"
                    ))
                    .await
                    .unwrap()
                    .check()
                    .unwrap()
                    .take(0)
                    .unwrap();
            assert_eq!(count, Some(0), "conflicting decision must roll back");
        }
    })
    .await
    .expect("locked activation inputs exceeded 90 seconds");
}
