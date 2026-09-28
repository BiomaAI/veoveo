use super::*;
use std::time::Duration;

#[tokio::test]
async fn additive_fence_schema_preserves_retained_authorities() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.b.clone());
        let registry = files.registry();
        let owner = scope(&db.a, "retained-authorities").await;
        for (kind, path) in [
            (AuthorityDatasetKind::Tzdb, &files.tzdb),
            (AuthorityDatasetKind::LeapSeconds, &files.first_leaps),
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
        }
        let pointers: Vec<crate::persistence::TimeActiveAuthorityRecord> =
            db.a.client()
                .query("SELECT * FROM time_active_authority ORDER BY id;")
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
        let releases = catalog.list_releases(&owner).await.unwrap();
        // Reconstruct the pre-migration schema in this owned database. The normal
        // fixture startup already qualified the complete migration runner.
        db.a.client()
            .query("REMOVE TABLE time_authority_activation_fence;")
            .await
            .unwrap()
            .check()
            .unwrap();
        let migration = veoveo_platform_store::migrations()
            .iter()
            .find(|entry| entry.filename == "0096_time_activation_fence.surql")
            .unwrap();
        db.a.client()
            .query("BEGIN TRANSACTION;")
            .query(migration.sql)
            .query("COMMIT TRANSACTION;")
            .await
            .unwrap()
            .check()
            .unwrap();
        let after: Vec<crate::persistence::TimeActiveAuthorityRecord> =
            db.a.client()
                .query("SELECT * FROM time_active_authority ORDER BY id;")
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
        assert_eq!(after, pointers);
        assert_eq!(catalog.list_releases(&owner).await.unwrap(), releases);
        let (replacement, _) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.next_leaps,
        )
        .await;
        activate(
            &catalog,
            &registry,
            &owner,
            &replacement,
            TimeWriteGuard::Existing(TimeVersion::FIRST),
        )
        .await;
        let engine = registry.authority_engine(&catalog, &owner).await.unwrap();
        assert_eq!(
            engine.authority().effective().leap_seconds().release_id(),
            &replacement.release_id
        );
    })
    .await
    .expect("retained authority migration qualification exceeded 90 seconds");
}

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
        let db = TestDb::with_backend(crate::test_store::StoreBackend::RocksDb).await;
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
async fn changed_preflight_inputs_reject_publication_and_roll_back_the_fence() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
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
        let fence = RecordId::new(
            "time_authority_activation_fence",
            owner.identity.tenant_id.to_string(),
        );
        let token: Option<Uuid> =
            db.a.client()
                .query("SELECT VALUE token FROM ONLY $fence;")
                .bind(("fence", fence.clone()))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
        assert!(token.is_some());
        for target in [&leaps, &tzdb] {
            let draft = prepare(&registry, &catalog, &owner, &tzdb, TimeWriteGuard::Absent).await;
            let record = RecordId::new("time_authority_release", target.release_id.to_string());
            db.a.client()
                .query("UPDATE $record SET artifact_path = '/changed-after-preflight' RETURN NONE;")
                .bind(("record", record.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(catalog.commit_activation(&owner, draft).await.is_err());
            db.a.client()
                .query("UPDATE $record SET artifact_path = $path RETURN NONE;")
                .bind(("record", record))
                .bind(("path", target.artifact_path.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            let after: Option<Uuid> =
                db.a.client()
                    .query("SELECT VALUE token FROM ONLY $fence;")
                    .bind(("fence", fence.clone()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap()
                    .take(0)
                    .unwrap();
            assert_eq!(
                after, token,
                "the failed publication must roll back its fence write"
            );
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
            .query("DELETE time_active_authority WHERE tenant = $tenant RETURN NONE;")
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
async fn failed_file_preflight_never_creates_an_activation_fence() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
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
        let fence = RecordId::new(
            "time_authority_activation_fence",
            owner.identity.tenant_id.to_string(),
        );
        let token: Option<Uuid> =
            db.a.client()
                .query("SELECT VALUE token FROM ONLY $fence;")
                .bind(("fence", fence))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
        assert!(token.is_none());
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
