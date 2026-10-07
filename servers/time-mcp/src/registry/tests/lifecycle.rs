use super::*;

async fn read_body(db: &PlatformStore, record: &RecordId) -> String {
    db.client()
        .query(include_str!(
            "../../tests/queries/read_canonical_json.surql"
        ))
        .bind(("record", record.clone()))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take::<Option<String>>(0)
        .unwrap()
        .expect("fixture record must have a canonical body")
}

#[tokio::test]
async fn stored_columns_supply_versions_with_current_body_validation() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.b.clone());
        let owner = scope(&db.a, "lifecycle-versions").await;
        let (release, acquisition) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        let event = catalog
            .create_event(
                &owner,
                TemporalEvent {
                    event_id: TemporalEventId::parse(format!("event-{}", Uuid::now_v7())).unwrap(),
                    name: "fixture".into(),
                    due: resolved(&TemporalEngine::new(files.bootstrap.clone())),
                    state: TemporalEventState::Scheduled,
                    record_version: TimeVersion::FIRST,
                },
                "fixture".into(),
            )
            .await
            .unwrap();
        let records = [
            RecordId::new("time_source", release.source_id.to_string()),
            RecordId::new("time_authority_release", release.release_id.to_string()),
            RecordId::new("time_acquisition", acquisition.acquisition_id.to_string()),
            RecordId::new("time_temporal_event", event.event_id.to_string()),
        ];
        let mut bodies = Vec::new();
        for record in &records {
            bodies.push(
                serde_json::from_str::<serde_json::Value>(&read_body(&db.a, record).await).unwrap(),
            );
        }
        for malformed in [
            serde_json::json!(0),
            serde_json::json!(u64::MAX),
            serde_json::json!(-1),
            serde_json::json!("1"),
            serde_json::Value::Null,
        ] {
            for (record, body) in records.iter().zip(&mut bodies) {
                body["recordVersion"] = malformed.clone();
                set(&db.a, record.clone(), "canonical_json", body.to_string()).await;
            }
            assert!(catalog.source(&owner, &release.source_id).await.is_err());
            assert!(catalog.release(&owner, &release.release_id).await.is_err());
            assert!(
                catalog
                    .acquisition(&owner, &acquisition.acquisition_id)
                    .await
                    .is_err()
            );
            assert!(catalog.event(&owner, &event.event_id).await.is_err());
        }
        for (record, body) in records.iter().zip(&mut bodies) {
            body["recordVersion"] = 1.into();
            set(&db.a, record.clone(), "canonical_json", body.to_string()).await;
            set(&db.a, record.clone(), "record_version", 0_i64).await;
        }
        assert!(catalog.source(&owner, &release.source_id).await.is_err());
        assert!(catalog.release(&owner, &release.release_id).await.is_err());
        assert!(
            catalog
                .acquisition(&owner, &acquisition.acquisition_id)
                .await
                .is_err()
        );
        assert!(catalog.event(&owner, &event.event_id).await.is_err());
    })
    .await
    .expect("current lifecycle version qualification exceeded 90 seconds");
}
