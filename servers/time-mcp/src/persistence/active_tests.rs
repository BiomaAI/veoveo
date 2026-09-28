use super::*;
use crate::{
    catalog::{TimeAccessContext, TimeCatalog},
    contract::*,
    test_store::TestDb,
};
use std::{collections::BTreeMap, time::Duration};
use surrealdb::types::Value;
use uuid::Uuid;
use veoveo_platform_store::PrincipalKind;

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

async fn release(
    catalog: &TimeCatalog,
    scope: &TimeAccessContext,
    kind: AuthorityDatasetKind,
) -> AuthorityRelease {
    let source = catalog
        .create_source(
            scope,
            crate::NewTimeSource {
                source_id: TimeSourceId::new(format!("time-source-{}", Uuid::now_v7())).unwrap(),
                name: "source".into(),
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
    catalog
        .create_release(
            scope,
            AuthorityRelease {
                release_id: AuthorityReleaseId::new(format!("time-release-{}", Uuid::now_v7()))
                    .unwrap(),
                source_id: source.source_id,
                dataset_kind: kind,
                state: AuthorityReleaseState::Staged,
                version_label: "test".into(),
                source_url: source.url,
                source_digest_sha256: "a".repeat(64).parse().unwrap(),
                artifact_path: "/tmp/time-authority-test".into(),
                retrieved_at: now,
                validated_at: now,
                record_version: crate::TimeVersion::new(1).unwrap(),
            },
        )
        .await
        .unwrap()
}

async fn patch(store: &PlatformStore, record: &RecordId, field: &str, value: Value) {
    store
        .client()
        .query("UPDATE $record MERGE $patch RETURN NONE;")
        .bind(("record", record.clone()))
        .bind(("patch", BTreeMap::from([(field.to_owned(), value)])))
        .await
        .unwrap()
        .check()
        .unwrap();
}

async fn pointer(store: &PlatformStore, record: &RecordId) -> TimeActiveAuthorityRecord {
    store
        .client()
        .select::<Option<TimeActiveAuthorityRecord>>(record.clone())
        .await
        .unwrap()
        .unwrap()
}

async fn raw_release(store: &PlatformStore, record: &RecordId) -> TimeAuthorityReleaseRecord {
    store
        .client()
        .select::<Option<TimeAuthorityReleaseRecord>>(record.clone())
        .await
        .unwrap()
        .unwrap()
}

async fn restore<T: SurrealValue>(store: &PlatformStore, record: &RecordId, content: T) {
    store
        .client()
        .query("UPDATE $record CONTENT $content RETURN NONE;")
        .bind(("record", record.clone()))
        .bind(("content", content))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn active_reads_reject_corrupt_pointers_and_filter_release_relationships_in_sql() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let owner = scope(&db.a, "active-parent").await;
        let foreign = scope(&db.a, "active-parent-other").await;
        let catalog = TimeCatalog::new(db.b.clone());
        assert!(catalog.active_releases(&owner).await.unwrap().is_empty());
        let current = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        let candidate = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        let other = release(&catalog, &foreign, AuthorityDatasetKind::Tzdb).await;
        for (scope, release) in [(&owner, &current), (&foreign, &other)] {
            catalog
                .activate_release(
                    scope,
                    &release.release_id,
                    TimeVersion::FIRST,
                    TimeWriteGuard::Absent,
                )
                .await
                .unwrap();
        }
        let key = time_record(
            "time_active_authority",
            format!("{}:tzdb", owner.identity.tenant_id),
        );
        let original = pointer(&db.a, &key).await;
        let release_key = time_record("time_authority_release", &current.release_id);
        let original_release = raw_release(&db.a, &release_key).await;
        let candidate_key = time_record("time_authority_release", &candidate.release_id);
        let original_candidate = raw_release(&db.a, &candidate_key).await;
        assert_eq!(
            catalog.active_releases(&owner).await.unwrap()[0].release_id,
            current.release_id
        );

        for (field, bad) in [
            ("record_version", 0_i64.into_value()),
            ("record_version", (-1_i64).into_value()),
            ("record_version", 2_i64.into_value()), // A replacement must carry history.
            ("dataset_kind", "leap_seconds".into_value()),
            ("release_key", "time-release-private-invalid".into_value()),
            ("release_key", other.release_id.to_string().into_value()),
            ("release_key", candidate.release_id.to_string().into_value()), // Staged.
            (
                "release_key",
                format!("time-release-{}", Uuid::now_v7()).into_value(),
            ),
            (
                "previous_release_key",
                current.release_id.to_string().into_value(),
            ),
            (
                "previous_release_key",
                other.release_id.to_string().into_value(),
            ),
        ] {
            patch(&db.a, &key, field, bad).await;
            let retained = pointer(&db.a, &key).await;
            let error = catalog.active_releases(&owner).await.unwrap_err();
            assert!(!error.to_string().contains("private-invalid"));
            assert!(
                catalog
                    .activate_release(
                        &owner,
                        &candidate.release_id,
                        TimeVersion::FIRST,
                        TimeWriteGuard::Existing(TimeVersion::FIRST)
                    )
                    .await
                    .is_err(),
                "{field}"
            );
            assert_eq!(pointer(&db.a, &key).await, retained);
            assert_eq!(raw_release(&db.a, &candidate_key).await, original_candidate);
            assert_eq!(
                catalog.active_releases(&foreign).await.unwrap()[0].release_id,
                other.release_id
            );
            restore(&db.a, &key, original.clone()).await;
        }

        // A denied or wrong-family release must be absent from the SQL result,
        // before any public-body decoder runs. The visible pointer still fails.
        for (field, bad) in [
            (
                "tenant",
                foreign.identity.tenant_id.record_id().into_value(),
            ),
            ("dataset_kind", "leap_seconds".into_value()),
            ("release_key", other.release_id.to_string().into_value()),
            ("state", "staged".into_value()),
            ("state", "retired".into_value()),
            ("state", "quarantined".into_value()),
        ] {
            patch(&db.a, &release_key, field, bad).await;
            patch(
                &db.a,
                &release_key,
                "canonical_json",
                "private malformed body".into_value(),
            )
            .await;
            let retained = raw_release(&db.a, &release_key).await;
            let mut response =
                db.b.client()
                    .query(ACTIVE_AUTHORITIES)
                    .bind(("tenant", owner.identity.tenant_id.record_id()))
                    .bind(("kind", None::<TimeDatasetKind>))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            let rows: Vec<ActiveAuthorityRow> = response.take(0).unwrap();
            assert_eq!(rows.len(), 1);
            assert!(rows[0].release.is_none(), "{field}");
            let error = catalog.active_releases(&owner).await.unwrap_err();
            assert!(error.to_string().contains("active_authority.release"));
            assert!(!error.to_string().contains("private malformed body"));
            assert!(
                catalog
                    .activate_release(
                        &owner,
                        &candidate.release_id,
                        TimeVersion::FIRST,
                        TimeWriteGuard::Existing(TimeVersion::FIRST)
                    )
                    .await
                    .is_err()
            );
            assert_eq!(raw_release(&db.a, &release_key).await, retained);
            assert_eq!(raw_release(&db.a, &candidate_key).await, original_candidate);
            assert_eq!(pointer(&db.a, &key).await, original);
            restore(&db.a, &release_key, original_release.clone()).await;
        }

        for version in [0_i64, -1] {
            patch(&db.a, &release_key, "record_version", version.into_value()).await;
            assert!(catalog.active_releases(&owner).await.is_err());
            assert!(
                catalog
                    .activate_release(
                        &owner,
                        &candidate.release_id,
                        TimeVersion::FIRST,
                        TimeWriteGuard::Existing(TimeVersion::FIRST)
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                raw_release(&db.a, &release_key).await.record_version,
                version
            );
            assert_eq!(raw_release(&db.a, &candidate_key).await, original_candidate);
            restore(&db.a, &release_key, original_release.clone()).await;
        }

        // The unique tenant/family index alone does not prove the physical key.
        let mut moved = original.clone();
        moved.id = time_record("time_active_authority", "invalid-physical-key");
        db.a.client()
            .query("DELETE $record; CREATE ONLY $moved CONTENT $content RETURN NONE;")
            .bind(("record", key.clone()))
            .bind(("moved", moved.id.clone()))
            .bind(("content", moved.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(catalog.active_releases(&owner).await.is_err());
        assert_eq!(pointer(&db.a, &moved.id).await, moved);
        db.a.client()
            .query("DELETE $record; CREATE ONLY $moved CONTENT $content RETURN NONE;")
            .bind(("record", moved.id))
            .bind(("moved", key.clone()))
            .bind(("content", original.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();

        let leaps = release(&catalog, &owner, AuthorityDatasetKind::LeapSeconds).await;
        catalog
            .activate_release(
                &owner,
                &leaps.release_id,
                TimeVersion::FIRST,
                TimeWriteGuard::Absent,
            )
            .await
            .unwrap();
        let pair = catalog.active_releases(&owner).await.unwrap();
        assert_eq!(pair.len(), 2);
        assert_eq!(pair[0].release_id, leaps.release_id);
        assert_eq!(pair[1].release_id, current.release_id);
        db.a.client()
            .query("DELETE $record RETURN NONE;")
            .bind(("record", release_key))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(catalog.active_releases(&owner).await.is_err());
        assert_eq!(pointer(&db.a, &key).await, original);
    })
    .await
    .expect("active authority parent qualification exceeded 90 seconds");
}

#[tokio::test]
async fn activation_rechecks_pointer_and_previous_release_relationships_inside_transaction() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let owner = scope(&db.a, "activation-parent").await;
        let catalog = TimeCatalog::new(db.b.clone());
        let first = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        let second = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        let candidate = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        catalog
            .activate_release(
                &owner,
                &first.release_id,
                TimeVersion::FIRST,
                TimeWriteGuard::Absent,
            )
            .await
            .unwrap();
        catalog
            .activate_release(
                &owner,
                &second.release_id,
                TimeVersion::FIRST,
                TimeWriteGuard::Existing(TimeVersion::FIRST),
            )
            .await
            .unwrap();
        let pointer_id = time_record(
            "time_active_authority",
            format!("{}:tzdb", owner.identity.tenant_id),
        );
        let previous_id = time_record("time_authority_release", &second.release_id);
        let candidate_id = time_record("time_authority_release", &candidate.release_id);
        let before_pointer = pointer(&db.a, &pointer_id).await;
        let before_previous = raw_release(&db.a, &previous_id).await;
        let before_candidate = raw_release(&db.a, &candidate_id).await;
        // Each fixture event changes a relationship after the candidate update,
        // before pointer replacement and retirement. It exercises the production
        // transaction after preflight without a mock or a timing-dependent race.
        for (table, assignment) in [
            ("time_active_authority", "dataset_kind = 'leap_seconds'"),
            (
                "time_active_authority",
                "release_key = 'time-release-changed'",
            ),
            ("time_active_authority", "previous_release_key = NONE"),
            ("time_active_authority", "record_version = 3"),
            ("time_active_authority", "tenant = tenant:other"),
            ("time_authority_release", "state = 'quarantined'"),
            ("time_authority_release", "dataset_kind = 'leap_seconds'"),
            (
                "time_authority_release",
                "release_key = 'time-release-changed'",
            ),
            ("time_authority_release", "record_version = 3"),
            ("time_authority_release", "tenant = tenant:other"),
        ] {
            let previous_state = if table == "time_authority_release" {
                "AND state = 'active'"
            } else {
                ""
            };
            let event = format!(
                "DEFINE EVENT OVERWRITE time_test_interleave ON time_authority_release
                WHEN $event = 'UPDATE' AND $before.state = 'staged' AND $after.state = 'active'
                THEN (UPDATE {table} SET {assignment} WHERE tenant = $after.tenant
                    AND dataset_kind = $after.dataset_kind AND id != $after.id {previous_state});"
            );
            db.a.client().query(event).await.unwrap().check().unwrap();
            assert!(
                catalog
                    .activate_release(
                        &owner,
                        &candidate.release_id,
                        TimeVersion::FIRST,
                        TimeWriteGuard::Existing(TimeVersion::new(2).unwrap())
                    )
                    .await
                    .is_err(),
                "{table}: {assignment}"
            );
            assert_eq!(
                pointer(&db.a, &pointer_id).await,
                before_pointer,
                "{assignment}"
            );
            assert_eq!(
                raw_release(&db.a, &previous_id).await,
                before_previous,
                "{assignment}"
            );
            assert_eq!(
                raw_release(&db.a, &candidate_id).await,
                before_candidate,
                "{assignment}"
            );
            db.a.client()
                .query("REMOVE EVENT time_test_interleave ON time_authority_release;")
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        catalog
            .activate_release(
                &owner,
                &candidate.release_id,
                TimeVersion::FIRST,
                TimeWriteGuard::Existing(TimeVersion::new(2).unwrap()),
            )
            .await
            .unwrap();
        assert_eq!(pointer(&db.a, &pointer_id).await.record_version, 3);
        assert_eq!(
            raw_release(&db.a, &previous_id).await.state,
            TimeAuthorityReleaseState::Retired
        );
        assert_eq!(
            catalog.active_releases(&owner).await.unwrap()[0].release_id,
            candidate.release_id
        );
    })
    .await
    .expect("transactional authority parent qualification exceeded 90 seconds");
}
