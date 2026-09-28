use super::*;
use uuid::Uuid;
use veoveo_platform_store::PrincipalKind;

#[tokio::test]
async fn time_authority_activation_retires_the_previous_release_atomically() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let identity =
            db.a.ensure_identity(
                "tenant-time",
                "time-admin",
                "https://veoveo.local/services",
                "time-admin",
                PrincipalKind::Service,
            )
            .await
            .unwrap();
        let store = TimePersistence::new(db.b.clone());
        let source_key = TimeSourceId::new(format!("time-source-{}", Uuid::now_v7())).unwrap();
        store
            .create_time_source(TimeSourceDraft {
                identity: identity.clone(),
                source_key: source_key.clone(),
                name: "IANA leap seconds".to_owned(),
                dataset_kind: TimeDatasetKind::LeapSeconds,
                source_url: "https://example.com/leap-seconds.list".to_owned(),
                expected_content_type: "text/plain".to_owned(),
                enabled: true,
                canonical_json: serde_json::json!({"source_id": source_key}).to_string(),
            })
            .await
            .unwrap();
        let create_release =
            |release_key: AuthorityReleaseId, digest: String| TimeAuthorityReleaseDraft {
                identity: identity.clone(),
                release_key,
                source_key: source_key.clone(),
                dataset_kind: TimeDatasetKind::LeapSeconds,
                state: TimeAuthorityReleaseState::Staged,
                version_label: format!("iana-{}", &digest[..12]),
                source_url: "https://example.com/leap-seconds.list".to_owned(),
                source_digest_sha256: digest,
                artifact_path: "/var/lib/veoveo/time/releases/test/leap-seconds.list".to_owned(),
                retrieved_at: Utc::now(),
                validated_at: Utc::now(),
                canonical_json: serde_json::json!({"state": "staged"}).to_string(),
            };

        let first_key =
            AuthorityReleaseId::new(format!("time-release-{}", Uuid::now_v7())).unwrap();
        store
            .create_time_authority_release(create_release(first_key.clone(), "a".repeat(64)))
            .await
            .unwrap();
        let first = store
            .activate_time_authority_release(
                &identity,
                &first_key,
                1,
                0,
                serde_json::json!({"state": "active"}).to_string(),
            )
            .await
            .unwrap();
        assert_eq!(first.state, TimeAuthorityReleaseState::Active);

        let second_key =
            AuthorityReleaseId::new(format!("time-release-{}", Uuid::now_v7())).unwrap();
        store
            .create_time_authority_release(create_release(second_key.clone(), "b".repeat(64)))
            .await
            .unwrap();
        let second = store
            .activate_time_authority_release(
                &identity,
                &second_key,
                1,
                1,
                serde_json::json!({"state": "active"}).to_string(),
            )
            .await
            .unwrap();
        assert_eq!(second.state, TimeAuthorityReleaseState::Active);
        let retired = store
            .time_authority_release(identity.tenant_id, &first_key)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retired.state, TimeAuthorityReleaseState::Retired);
        assert_eq!(retired.record_version, 3);
        let pointer = store
            .active_time_authority(identity.tenant_id, TimeDatasetKind::LeapSeconds)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pointer.release_key, second_key.as_str());
        assert_eq!(
            pointer.previous_release_key.as_deref(),
            Some(first_key.as_str())
        );
        assert_eq!(pointer.record_version, 2);
        let left_key = AuthorityReleaseId::new(format!("time-release-{}", Uuid::now_v7())).unwrap();
        let right_key =
            AuthorityReleaseId::new(format!("time-release-{}", Uuid::now_v7())).unwrap();
        for (key, digest) in [(&left_key, "c"), (&right_key, "d")] {
            store
                .create_time_authority_release(create_release(key.clone(), digest.repeat(64)))
                .await
                .unwrap();
        }
        let other = TimePersistence::new(db.a.clone());
        let (left, right) = tokio::join!(
            store.activate_time_authority_release(&identity, &left_key, 1, 2, "{}".into()),
            other.activate_time_authority_release(&identity, &right_key, 1, 2, "{}".into()),
        );
        assert_ne!(
            left.is_ok(),
            right.is_ok(),
            "only one replacement may advance the pointer"
        );
        let (winner, loser) = if left.is_ok() {
            (&left_key, &right_key)
        } else {
            (&right_key, &left_key)
        };
        let pointer = other
            .active_time_authority(identity.tenant_id, TimeDatasetKind::LeapSeconds)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pointer.release_key, winner.as_str());
        assert_eq!(pointer.record_version, 3);
        let loser = other
            .time_authority_release(identity.tenant_id, loser)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loser.state, TimeAuthorityReleaseState::Staged);
        assert_eq!(loser.record_version, 1);
        assert_eq!(
            other
                .time_authority_release(identity.tenant_id, &second_key)
                .await
                .unwrap()
                .unwrap()
                .state,
            TimeAuthorityReleaseState::Retired
        );
    })
    .await
    .expect("authority activation qualification exceeded 90 seconds");
}
